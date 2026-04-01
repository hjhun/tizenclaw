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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[tokio::main]
async fn main() {
    // ── Phase 1: Detect platform & initialize paths ──
    let platform = libtizenclaw::PlatformContext::detect();
    platform.paths.ensure_dirs();

    // Fix OpenSSL vendored TLS handshake on Tizen by explicitly exporting the System CA bundle
    if std::path::Path::new("/etc/ssl/ca-bundle.pem").exists() {
        std::env::set_var("SSL_CERT_FILE", "/etc/ssl/ca-bundle.pem");
    }

    // ── Phase 2: Initialize logging (platform-aware) ──
    // The platform logger is loaded dynamically from the platform context
    common::logging::init_with_logger(Some(platform.logger.clone()));
    log::info!("═══════════════════════════════════════");
    log::info!("  TizenClaw Daemon v1.0.0");
    log::info!("  Platform: {}", platform.platform_name());
    log::info!("  Data dir: {:?}", platform.paths.data_dir);
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

    // ── Phase 5: Start ToolWatcher ──
    log::info!("[Boot] Starting ToolWatcher...");
    let mut tool_watcher = core::tool_watcher::ToolWatcher::new(
        platform.paths.tools_dir.to_string_lossy().to_string()
    );
    let agent_clone_watcher = agent.clone();
    tool_watcher.set_change_callback(move || {
        agent_clone_watcher.reload_tools();
    });
    let _watcher_handle = tool_watcher.start();

    // ── Phase 6: Start TaskScheduler ──
    log::info!("[Boot] Starting TaskScheduler...");
    let task_scheduler = core::task_scheduler::TaskScheduler::new();
    let scheduler_config = platform.paths.config_dir.join("scheduler_config.json");
    task_scheduler.load_config(&scheduler_config.to_string_lossy());
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
    channel_registry.load_config(&channel_config_path.to_string_lossy());

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
        if let Some(ch) = channel::channel_factory::create_channel(&dashboard_config) {
            channel_registry.register(ch);
            log::info!("[Boot] WebDashboard registered (port 9090)");
        }
    }

    channel_registry.start_all();

    log::info!("[Boot] TizenClaw daemon ready.");

    // ── Phase 9: Startup LLM Context Indexing ──
    let startup_agent = agent.clone();
    tokio::spawn(async move {
        // Wait 5 seconds to ensure IPC/channels are completely established
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
