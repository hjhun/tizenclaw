//! TizenClaw Rust Daemon — full daemon entry point.
//!
//! Initializes platform detection, logging, AgentCore, IPC server,
//! and runs the main loop until SIGTERM/SIGINT is received.
//!
//! Build modes:
//!   cargo build          → Generic Linux (Ubuntu) — no Tizen libs needed
//!   deploy.sh (GBS)      → Tizen — libtizenclaw_plugin.so provides dlog, etc.

// Suppress unused warnings during migration.
// TODO: Remove once all modules are wired into the daemon.
#![allow(unused)]

pub mod common;
pub mod infra;
pub mod storage;
pub mod llm;
pub mod core;
pub mod channel;
pub mod network;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[tokio::main]
async fn main() {
    // ── Phase 1: Detect platform & initialize paths ──
    let platform = libtizenclaw_core::framework::PlatformContext::detect();
    platform.paths.ensure_dirs();

    // Fix OpenSSL vendored TLS handshake on Tizen by explicitly exporting the System CA bundle
    if std::path::Path::new("/etc/ssl/ca-bundle.pem").exists() {
        std::env::set_var("SSL_CERT_FILE", "/etc/ssl/ca-bundle.pem");
    }

    // ── Phase 2: Initialize logging (platform-aware) ──
    // Initialize file log backend manually targeting specific directory
    if let Err(e) = std::fs::create_dir_all("/opt/usr/share/tizenclaw/logs") {
        log::error!("Failed to create logs dir: {}", e);
    }
    common::logging::FileLogBackend::init("/opt/usr/share/tizenclaw/logs/tizenclaw.log", 10 * 1024 * 1024);
    
    // The global logger handles DLOG routing internally and natively.
    common::logging::init_with_logger();

    // ── Phase 2.5: Pre-initialize HTTP Client ──
    // Force initialization of reqwest::Client on the global multi-threaded
    // Tokio runtime. If initialized lazily inside a spawned IpcServer thread
    // (which uses a temporary single-threaded runtime), the client's reactor 
    // dies when that thread finishes, causing subsequent LLM requests to hang.
    // Pre-initialization also allows the client to multiplex multiple sessions
    // over a shared Hyper connection pool using the same TLS configuration.
    infra::http_client::default_client();

    log::info!("═══════════════════════════════════════");
    log::debug!("  TizenClaw Daemon v1.0.0");
    log::debug!("  Platform: {}", platform.platform_name());
    log::debug!("  Data dir: {:?}", platform.paths.data_dir);
    log::info!("═══════════════════════════════════════");

    // ── Phase 3: Set up signal handlers ──
    unsafe {
        libc::signal(libc::SIGINT, signal_handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, signal_handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    // Share platform context
    let platform = Arc::new(platform);

    // ── Phase 4: Initialize AgentCore ──
    log::info!("[Boot] Initializing AgentCore...");
    let agent = core::agent_core::AgentCore::new(platform.clone());
    if !agent.initialize().await {
        log::error!("AgentCore initialization failed");
    }
    let agent = Arc::new(agent);

    // Register pkgmgr listener for runtime plugin injection
    use libtizenclaw_core::plugin_core::pkgmgr_client::{PkgmgrClient, PkgmgrListener, PkgmgrEventArgs};
    struct AgentPkgmgrListener(Arc<core::agent_core::AgentCore>);
    impl PkgmgrListener for AgentPkgmgrListener {
        fn on_pkgmgr_event(&self, args: Arc<PkgmgrEventArgs>) {
            if args.event_status == "end" {
                let agent_clone = self.0.clone();
                let event_name = args.event_name.clone();
                let pkgid = args.pkgid.clone();
                tokio::spawn(async move {
                    agent_clone.handle_pkgmgr_event(&event_name, &pkgid).await;
                });
            }
        }
    }
    PkgmgrClient::global().add_listener(Arc::new(AgentPkgmgrListener(agent.clone())));

    // ── Phase 5: Start ToolWatcher (Removed) ──
    // ToolWatcher polling has been removed to prevent infinite loops and token waste.
    // Indexing is now driven purely by pkgmgr events and startup existence checks.

    // ── Phase 6: Start TaskScheduler ──
    log::info!("[Boot] Starting TaskScheduler...");
    let task_scheduler = core::task_scheduler::TaskScheduler::new();
    let task_dir = "/opt/usr/share/tizenclaw/tasks";
    let _ = std::fs::create_dir_all(task_dir);
    task_scheduler.load_config(task_dir);
    let _scheduler_handle = task_scheduler.start();

    // ── Phase 7: Start IPC server ──
    log::info!("[Boot] Starting IPC server...");
    let ipc = core::ipc_server::IpcServer::new();
    let ipc_handle = ipc.start(agent.clone());

    // ── Phase 8: Initialize channels ──
    log::info!("[Boot] Initializing channels...");
    let mut channel_registry = channel::ChannelRegistry::new();

    // Load from config if available
    let channel_config_path = platform.paths.config_dir.join("channel_config.json");
    channel_registry.load_config(&channel_config_path.to_string_lossy(), Some(agent.clone()));

    // Always ensure web_dashboard is started on port 9090
    let has_dashboard = channel_registry.has_channel("web_dashboard");
    if !has_dashboard {
        let web_root = platform.paths.web_root.to_string_lossy().to_string();
        let dashboard_config = channel::ChannelConfig {
            name: "web_dashboard".into(),
            channel_type: "web_dashboard".into(),
            enabled: true,
            settings: serde_json::json!({
                "port": 9090,
                "localhost_only": false,
                "web_root": web_root
            }),
        };
        if let Some(ch) = channel::channel_factory::create_channel(&dashboard_config, Some(agent.clone())) {
            channel_registry.register(ch);
            log::info!("[Boot] WebDashboard registered (port 9090)");
        }
    }

    channel_registry.start_all();

    // ── Phase 8.5: Start mDNS Scanner ──
    log::info!("[Boot] Starting mDNS network scanner...");
    let mdns_scanner = network::mdns_discovery::MdnsScanner::new();
    mdns_scanner.start();

    log::info!("[Boot] TizenClaw daemon ready.");

    // ── Phase 9: Startup Tool Indexing (Hybrid: Local Scan + LLM) ──
    // Scans the external tools root plus the TizenClaw-owned embedded
    // descriptor root, then uses a single LLM call to generate
    // high-quality tools.md / index.md.
    // Falls back to template generation if no LLM is available.
    let startup_agent = agent.clone();
    tokio::spawn(async move {
        // Wait for IPC/channels to be fully established
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        startup_agent.run_startup_indexing().await;
    });

    // ── Main loop — sleep until signal received ──
    while RUNNING.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    // ── Shutdown ──
    log::info!("TizenClaw daemon shutting down...");
    channel_registry.stop_all();
    task_scheduler.stop();
    ipc.stop();
    let _ = ipc_handle.join();

    agent.shutdown();

    log::info!("TizenClaw daemon stopped.");
}
