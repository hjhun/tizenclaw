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

pub mod channel;
pub mod common;
pub mod core;
pub mod generic;
pub mod infra;
pub mod llm;
pub mod network;
pub mod storage;
pub mod tizen;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[tokio::main]
async fn main() {
    // ── Phase 1: Detect platform & initialize paths ──
    let platform = libtizenclaw_core::framework::PlatformContext::detect();
    platform.paths.ensure_dirs();
    let boot_log_path = platform.paths.logs_dir.join("tizenclaw.log");
    let mut boot_logger = common::boot_status_logger::BootStatusLogger::new(boot_log_path);

    // Fix OpenSSL vendored TLS handshake on Tizen by explicitly exporting the System CA bundle
    if std::path::Path::new("/etc/ssl/ca-bundle.pem").exists() {
        std::env::set_var("SSL_CERT_FILE", "/etc/ssl/ca-bundle.pem");
    }

    // ── Phase 2: Initialize logging (platform-aware) ──
    if let Err(e) = std::fs::create_dir_all(&platform.paths.logs_dir) {
        log::error!("Failed to create logs dir: {}", e);
    }
    common::logging::FileLogBackend::init(&platform.paths.logs_dir, 10 * 1024 * 1024);
    common::logging::init_with_logger();
    boot_logger.record_status(
        "Logging",
        true,
        &format!(
            "runtime logs under {}",
            platform.paths.logs_dir.to_string_lossy()
        ),
    );

    // ── Phase 2.5: Pre-initialize HTTP Client ──
    infra::http_client::default_client();
    boot_logger.record_status("HTTP client", true, "default client warmed up");

    log::info!("═══════════════════════════════════════");
    log::debug!("  TizenClaw Daemon v1.0.0");
    log::debug!("  Platform: {}", platform.platform_name());
    log::debug!("  Data dir: {:?}", platform.paths.data_dir);
    log::info!("═══════════════════════════════════════");

    // ── Phase 3: Set up signal handlers ──
    unsafe {
        libc::signal(
            libc::SIGINT,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            signal_handler as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    let platform = Arc::new(platform);

    // ── Phase 4: Initialize AgentCore ──
    log::info!("[Boot] Initializing AgentCore...");
    let agent = core::agent_core::AgentCore::new(platform.clone());
    let agent_initialized = agent.initialize().await;
    if !agent_initialized {
        log::error!("AgentCore initialization failed");
    }
    boot_logger.record_status(
        "AgentCore",
        agent_initialized,
        if agent_initialized {
            "agent core initialized"
        } else {
            "agent core initialization failed"
        },
    );
    let agent = Arc::new(agent);

    // Register pkgmgr listener for runtime plugin injection
    use libtizenclaw_core::plugin_core::pkgmgr_client::{
        PkgmgrClient, PkgmgrEventArgs, PkgmgrListener,
    };
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

    // ── Phase 6: Start TaskScheduler ──
    log::info!("[Boot] Starting TaskScheduler...");
    let task_scheduler = core::task_scheduler::TaskScheduler::new();
    let task_dir = platform.paths.data_dir.join("tasks");
    let _ = std::fs::create_dir_all(&task_dir);
    let seeded_tasks = task_scheduler.seed_default_tasks_if_empty(&task_dir.to_string_lossy());
    task_scheduler.load_config(&task_dir.to_string_lossy());
    let scheduler_handle = task_scheduler.start();
    boot_logger.record_status(
        "TaskScheduler",
        scheduler_handle.is_some(),
        &format!("seeded {} default task(s)", seeded_tasks),
    );

    // ── Phase 7: Initialize ChannelRegistry (shared with IPC) ──
    log::info!("[Boot] Initializing channels...");
    let channel_registry = Arc::new(Mutex::new(channel::ChannelRegistry::new()));

    // Load from channel_config.json
    let channel_config_path = platform.paths.config_dir.join("channel_config.json");
    {
        let mut reg = channel_registry.lock().unwrap();
        reg.load_config(&channel_config_path.to_string_lossy(), Some(agent.clone()));

        // Ensure web_dashboard is always registered (auto_start follows config).
        // If not present in config, register with auto_start = true as default.
        if !reg.has_channel("web_dashboard") {
            let web_root = platform.paths.web_root.to_string_lossy().to_string();
            let dashboard_config = channel::ChannelConfig {
                name: "web_dashboard".into(),
                channel_type: "web_dashboard".into(),
                enabled: true,
                settings: serde_json::json!({
                    "port": core::runtime_paths::default_dashboard_port(),
                    "localhost_only": false,
                    "web_root": web_root
                }),
            };
            if let Some(ch) =
                channel::channel_factory::create_channel(&dashboard_config, Some(agent.clone()))
            {
                reg.register(ch, true);
                log::info!(
                    "[Boot] WebDashboard registered (port {}, auto_start=true)",
                    core::runtime_paths::default_dashboard_port()
                );
            }
        }

        reg.start_all();
    }
    boot_logger.record_status("Channels", true, "channel registry initialized");

    // ── Phase 7.5: Start IPC server (with registry reference) ──
    log::info!("[Boot] Starting IPC server...");
    let ipc = core::ipc_server::IpcServer::new();
    let ipc_handle = ipc.start(agent.clone(), channel_registry.clone());
    boot_logger.record_status("IPC server", true, "ipc server thread started");

    // ── Phase 8.5: Start mDNS Scanner ──
    log::info!("[Boot] Starting mDNS network scanner...");
    let mdns_scanner = network::mdns_discovery::MdnsScanner::new();
    mdns_scanner.start();
    boot_logger.record_status("mDNS scanner", true, "network discovery started");

    log::info!("[Boot] TizenClaw daemon ready.");
    boot_logger.record_status("Daemon ready", true, "startup sequence completed");
    log::info!("{}", boot_logger.summary());

    // ── Phase 9: Startup Tool Indexing ──
    let startup_agent = agent.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        startup_agent.run_startup_indexing().await;
    });

    // ── Main loop ──
    while RUNNING.load(Ordering::SeqCst) {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    // ── Shutdown ──
    log::info!("TizenClaw daemon shutting down...");
    channel_registry.lock().unwrap().stop_all();
    task_scheduler.stop();
    ipc.stop();
    let _ = ipc_handle.join();
    agent.shutdown().await;
    log::info!("TizenClaw daemon stopped.");
}
