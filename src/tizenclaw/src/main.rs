//! TizenClaw Rust Daemon — full daemon entry point.
//!
//! Initializes logging, AgentCore, IPC server, and runs
//! the main loop until SIGTERM/SIGINT is received.

// Suppress unused warnings during C++ → Rust migration.
// TODO: Remove once all modules are wired into the daemon.
#![allow(unused)]

pub mod common;
pub mod infra;
pub mod storage;
pub mod llm;
pub mod core;
pub mod channel;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" fn signal_handler(_sig: libc::c_int) {
    RUNNING.store(false, Ordering::SeqCst);
}

fn main() {
    // Initialize logging (dlog backend)
    common::logging::init();
    log::info!("═══════════════════════════════════════");
    log::info!("  TizenClaw Rust Daemon v1.0.0");
    log::info!("═══════════════════════════════════════");

    // Set up signal handlers
    unsafe {
        libc::signal(libc::SIGINT, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGTERM, signal_handler as libc::sighandler_t);
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    // Initialize AgentCore
    log::info!("[Boot] Initializing AgentCore...");
    let mut agent = core::agent_core::AgentCore::new();
    if !agent.initialize() {
        log::error!("AgentCore initialization failed");
    }
    let agent = Arc::new(Mutex::new(agent));

    // Start IPC server
    log::info!("[Boot] Starting IPC server...");
    let ipc = core::ipc_server::IpcServer::new();
    let ipc_handle = ipc.start(agent.clone());

    // Initialize channels
    log::info!("[Boot] Initializing channels...");
    let mut channel_registry = channel::ChannelRegistry::new();

    // Load from config if available
    let channel_config_path = "/opt/usr/share/tizenclaw/config/channel_config.json";
    channel_registry.load_config(channel_config_path);

    // Always ensure web_dashboard is started on port 9090
    let has_dashboard = channel_registry.has_channel("web_dashboard");
    if !has_dashboard {
        let dashboard_config = channel::ChannelConfig {
            name: "web_dashboard".into(),
            channel_type: "web_dashboard".into(),
            enabled: true,
            settings: serde_json::json!({
                "port": 9090,
                "localhost_only": false,
                "web_root": "/opt/usr/share/tizenclaw/web"
            }),
        };
        if let Some(ch) = channel::channel_factory::create_channel(&dashboard_config) {
            channel_registry.register(ch);
            log::info!("[Boot] WebDashboard registered (port 9090)");
        }
    }

    channel_registry.start_all();

    log::info!("[Boot] TizenClaw daemon ready.");

    // Main loop — sleep until signal received
    while RUNNING.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // Shutdown
    log::info!("TizenClaw daemon shutting down...");
    channel_registry.stop_all();
    ipc.stop();
    let _ = ipc_handle.join();

    if let Ok(mut a) = agent.lock() {
        a.shutdown();
    }

    log::info!("TizenClaw daemon stopped.");
}
