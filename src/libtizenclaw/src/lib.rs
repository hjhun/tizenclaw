//! claw-platform: Platform abstraction layer for TizenClaw.
//!
//! Provides trait-based interfaces for platform-specific functionality
//! and a dynamic plugin loader that scans directories for `.so` plugins.
//!
//! Architecture:
//! - `PlatformPlugin`: Core trait every platform plugin must implement
//! - `GenericLinuxPlatform`: Built-in fallback for standard Linux/Ubuntu
//! - `PluginLoader`: Runtime `dlopen`-based loader scanning plugin dirs
//! - `PlatformContext`: Singleton holding the active platform + all loaded plugins

pub mod generic_linux;
pub mod loader;
pub mod paths;

use serde_json::Value;
use std::path::PathBuf;

// ─────────────────────────────────────────
// Core Traits
// ─────────────────────────────────────────

/// Log severity levels (platform-agnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

/// Core platform plugin trait.
///
/// Every platform plugin (Tizen, Ubuntu, etc.) implements this trait.
/// The daemon loads plugins at runtime via `dlopen` and calls these methods.
pub trait PlatformPlugin: Send + Sync {
    /// Human-readable platform name (e.g., "Tizen", "Ubuntu", "Generic Linux").
    fn platform_name(&self) -> &str;

    /// Unique plugin identifier (e.g., "tizen", "ubuntu-desktop").
    fn plugin_id(&self) -> &str;

    /// Plugin version string.
    fn version(&self) -> &str { "1.0.0" }

    /// Priority for platform detection (higher = preferred).
    /// When multiple plugins claim to be compatible, the highest priority wins.
    fn priority(&self) -> u32 { 0 }

    /// Check if this plugin is compatible with the current environment.
    /// Called during plugin loading to determine which plugin to activate.
    fn is_compatible(&self) -> bool { true }

    /// Initialize the plugin. Called once after loading.
    fn initialize(&mut self) -> bool { true }

    /// Shutdown the plugin. Called once before unloading.
    fn shutdown(&mut self) {}
}

/// Platform-specific logging backend.
pub trait PlatformLogger: Send + Sync {
    /// Write a log message.
    fn log(&self, level: LogLevel, tag: &str, msg: &str);
}

/// Platform-specific system information provider.
pub trait SystemInfoProvider: Send + Sync {
    /// Get OS/platform version string.
    fn get_os_version(&self) -> Option<String>;

    /// Get full device profile as JSON.
    fn get_device_profile(&self) -> Value;

    /// Get battery level (0-100), if available.
    fn get_battery_level(&self) -> Option<u32> { None }

    /// Check if network is available.
    fn is_network_available(&self) -> bool {
        std::net::TcpStream::connect("8.8.8.8:53")
            .map(|_| true)
            .unwrap_or(false)
    }
}

/// Platform-specific package manager interface.
pub trait PackageManagerProvider: Send + Sync {
    /// List installed packages.
    fn list_packages(&self) -> Vec<PackageInfo>;

    /// Get info about a specific package.
    fn get_package_info(&self, pkg_id: &str) -> Option<PackageInfo>;

    /// Check if a package is installed.
    fn is_installed(&self, pkg_id: &str) -> bool {
        self.get_package_info(pkg_id).is_some()
    }
}

/// Platform-specific application control.
pub trait AppControlProvider: Send + Sync {
    /// Launch an application by ID.
    fn launch_app(&self, app_id: &str) -> Result<(), String>;

    /// List running applications.
    fn list_running_apps(&self) -> Vec<String> { vec![] }
}

/// Platform-specific system event monitoring.
pub trait SystemEventProvider: Send + Sync {
    /// Start monitoring system events.
    fn start(&mut self) -> bool { true }

    /// Stop monitoring.
    fn stop(&mut self) {}
}

// ─────────────────────────────────────────
// Data Types
// ─────────────────────────────────────────

/// Basic info about an installed package.
#[derive(Debug, Clone, Default)]
pub struct PackageInfo {
    pub pkg_id: String,
    pub app_id: String,
    pub label: String,
    pub version: String,
    pub pkg_type: String,
    pub installed: bool,
}

// ─────────────────────────────────────────
// Platform Context (Singleton)
// ─────────────────────────────────────────

/// Holds the active platform configuration and all loaded plugin capabilities.
///
/// Created once at daemon boot via `PlatformContext::detect()`.
pub struct PlatformContext {
    /// Active platform plugin.
    pub platform: Box<dyn PlatformPlugin>,
    /// Platform logger (from active plugin or generic stderr).
    pub logger: Box<dyn PlatformLogger>,
    /// System info provider.
    pub system_info: Box<dyn SystemInfoProvider>,
    /// Package manager (optional — may be no-op).
    pub package_manager: Box<dyn PackageManagerProvider>,
    /// App controller (optional — may be no-op).
    pub app_control: Box<dyn AppControlProvider>,
    /// Platform-resolved paths.
    pub paths: paths::PlatformPaths,
}

impl PlatformContext {
    /// Detect and load the appropriate platform.
    ///
    /// 1. Scan plugin directories for `.so` files
    /// 2. Load each plugin, check `is_compatible()`
    /// 3. Select the highest-priority compatible plugin
    /// 4. Fall back to `GenericLinuxPlatform` if no plugin matches
    pub fn detect() -> Self {
        // Determine paths first (used to find plugin directories)
        let platform_paths = paths::PlatformPaths::detect();

        // Try loading platform plugins from the plugins directory
        let plugin_dirs = vec![
            platform_paths.plugins_dir.clone(),
            // Also check standard system paths
            PathBuf::from("/usr/lib/tizenclaw/plugins"),
            PathBuf::from("/usr/local/lib/tizenclaw/plugins"),
        ];

        if let Some(ctx) = loader::try_load_platform_plugins(&plugin_dirs, &platform_paths) {
            return ctx;
        }

        // Fallback: use built-in Generic Linux platform
        log::info!("No platform plugin found, using Generic Linux fallback");
        let generic = generic_linux::GenericLinuxPlatform::new();
        PlatformContext {
            logger: Box::new(generic_linux::StderrLogger),
            system_info: Box::new(generic_linux::LinuxSystemInfo),
            package_manager: Box::new(generic_linux::GenericPackageManager),
            app_control: Box::new(generic_linux::GenericAppControl),
            platform: Box::new(generic),
            paths: platform_paths,
        }
    }

    /// Get the platform name.
    pub fn platform_name(&self) -> &str {
        self.platform.platform_name()
    }
}
