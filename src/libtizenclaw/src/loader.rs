//! Plugin loader — runtime `dlopen`-based plugin discovery and loading.
//!
//! Scans plugin directories for `.so` files, loads each one, and calls
//! the standard C ABI entry point to obtain platform trait implementations.
//!
//! ## Plugin ABI Contract
//!
//! Each plugin `.so` must export the following C function:
//!
//! ```c
//! // Returns a JSON string describing the plugin's capabilities.
//! // The caller must free the returned string with claw_plugin_free_string().
//! const char* claw_plugin_info();
//!
//! // Free a string returned by claw_plugin_info().
//! void claw_plugin_free_string(const char* s);
//! ```
//!
//! The JSON returned by `claw_plugin_info()` must contain:
//! ```json
//! {
//!   "plugin_id": "tizen",
//!   "platform_name": "Tizen",
//!   "version": "1.0.0",
//!   "priority": 100,
//!   "capabilities": ["logging", "system_info", "package_manager", "app_control"]
//! }
//! ```
//!
//! ## Plugin Discovery
//!
//! The loader scans the following directories in order:
//! 1. `/opt/usr/share/tizenclaw/plugins` (Tizen production)
//! 2. `/usr/lib/tizenclaw/plugins` (system-wide)
//! 3. `/usr/local/lib/tizenclaw/plugins` (local)
//! 4. `$TIZENCLAW_DATA_DIR/plugins` (user override)

use crate::generic_linux;
use crate::paths::PlatformPaths;
use crate::PlatformContext;
use std::path::{Path, PathBuf};

/// Metadata about a discovered plugin, parsed from `claw_plugin_info()` JSON.
#[derive(Debug, Clone)]
pub struct PluginMeta {
    pub plugin_id: String,
    pub platform_name: String,
    pub version: String,
    pub priority: u32,
    pub capabilities: Vec<String>,
    pub so_path: PathBuf,
}

/// A loaded plugin library handle (keeps the .so alive).
#[allow(dead_code)]
pub struct LoadedPlugin {
    pub meta: PluginMeta,
    lib: libloading::Library,
}

/// Attempt to load platform plugins from the given directories.
///
/// Returns `Some(PlatformContext)` if a compatible plugin is found,
/// `None` if no plugins match (caller should fall back to generic).
pub fn try_load_platform_plugins(
    plugin_dirs: &[PathBuf],
    paths: &PlatformPaths,
) -> Option<PlatformContext> {
    let mut discovered: Vec<PluginMeta> = Vec::new();

    for dir in plugin_dirs {
        if !dir.is_dir() {
            continue;
        }
        discover_plugins_in_dir(dir, &mut discovered);
    }

    if discovered.is_empty() {
        return None;
    }

    // Sort by priority (highest first)
    discovered.sort_by(|a, b| b.priority.cmp(&a.priority));

    // Try to load the highest-priority plugin
    for meta in &discovered {
        match try_activate_plugin(meta, paths) {
            Ok(ctx) => {
                log::info!(
                    "Loaded platform plugin: {} v{} (priority {})",
                    meta.platform_name, meta.version, meta.priority
                );
                return Some(ctx);
            }
            Err(e) => {
                log::warn!(
                    "Failed to activate plugin '{}' from {:?}: {}",
                    meta.plugin_id, meta.so_path, e
                );
            }
        }
    }

    None
}

/// Scan a directory for `.so` plugin files and extract their metadata.
fn discover_plugins_in_dir(dir: &Path, out: &mut Vec<PluginMeta>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Only load .so files that match our plugin naming conventions
        if !name.ends_with(".so") {
            continue;
        }

        match probe_plugin_info(&path) {
            Ok(meta) => {
                log::info!(
                    "Discovered plugin: {} ({}) at {:?}",
                    meta.platform_name, meta.plugin_id, path
                );
                out.push(meta);
            }
            Err(e) => {
                log::debug!("Skipping {:?}: {}", path, e);
            }
        }
    }
}

/// Load a .so file temporarily to call `claw_plugin_info()` and parse the result.
fn probe_plugin_info(so_path: &Path) -> Result<PluginMeta, String> {
    unsafe {
        let lib = libloading::Library::new(so_path)
            .map_err(|e| format!("dlopen failed: {}", e))?;

        // Look for claw_plugin_info symbol
        let info_fn: libloading::Symbol<unsafe extern "C" fn() -> *const std::os::raw::c_char> =
            lib.get(b"claw_plugin_info")
                .map_err(|e| format!("symbol 'claw_plugin_info' not found: {}", e))?;

        let ptr = info_fn();
        if ptr.is_null() {
            return Err("claw_plugin_info() returned null".into());
        }

        let json_str = std::ffi::CStr::from_ptr(ptr)
            .to_string_lossy()
            .to_string();

        // Try to free the string if the function exists
        if let Ok(free_fn) = lib.get::<unsafe extern "C" fn(*const std::os::raw::c_char)>(
            b"claw_plugin_free_string",
        ) {
            free_fn(ptr);
        }

        // Parse JSON
        let val: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("invalid plugin info JSON: {}", e))?;

        Ok(PluginMeta {
            plugin_id: val["plugin_id"].as_str().unwrap_or("unknown").to_string(),
            platform_name: val["platform_name"].as_str().unwrap_or("Unknown").to_string(),
            version: val["version"].as_str().unwrap_or("0.0.0").to_string(),
            priority: val["priority"].as_u64().unwrap_or(0) as u32,
            capabilities: val["capabilities"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            so_path: so_path.to_path_buf(),
        })
    }
}

fn try_activate_plugin(
    meta: &PluginMeta,
    paths: &PlatformPaths,
) -> Result<PlatformContext, String> {
    // Verify the plugin is compatible by checking if it can be loaded
    let lib = unsafe {
        std::sync::Arc::new(
            libloading::Library::new(&meta.so_path)
                .map_err(|e| format!("dlopen failed: {}", e))?
        )
    };

    // Try to load the logger C ABI function
    let log_fn = unsafe {
        lib.get::<unsafe extern "C" fn(i32, *const std::os::raw::c_char, *const std::os::raw::c_char)>(
            b"claw_plugin_log",
        )
        .ok()
        .map(|sym| *sym)
    };

    let logger: std::sync::Arc<dyn crate::PlatformLogger> = if let Some(f) = log_fn {
        std::sync::Arc::new(PluginLogger {
            _lib: lib.clone(),
            log_fn: f,
        })
    } else {
        std::sync::Arc::new(generic_linux::StderrLogger)
    };

    // Create a PluginPlatform wrapper
    let plugin_platform = PluginPlatform {
        meta: meta.clone(),
        _lib: lib.clone(),
    };

    Ok(PlatformContext {
        logger,
        system_info: Box::new(generic_linux::LinuxSystemInfo),
        package_manager: Box::new(generic_linux::GenericPackageManager),
        app_control: Box::new(generic_linux::GenericAppControl),
        platform: Box::new(plugin_platform),
        paths: paths.clone(),
    })
}

struct PluginLogger {
    _lib: std::sync::Arc<libloading::Library>,
    log_fn: unsafe extern "C" fn(i32, *const std::os::raw::c_char, *const std::os::raw::c_char),
}

impl crate::PlatformLogger for PluginLogger {
    fn log(&self, level: crate::LogLevel, tag: &str, msg: &str) {
        use std::ffi::CString;
        let lvl = match level {
            crate::LogLevel::Error => 0,
            crate::LogLevel::Warn => 1,
            crate::LogLevel::Info => 2,
            crate::LogLevel::Debug => 3,
        };
        // Escape '%' to prevent format string attacks in dlog
        let escaped = msg.replace('%', "%%");
        if let (Ok(t), Ok(m)) = (CString::new(tag), CString::new(escaped)) {
            unsafe {
                (self.log_fn)(lvl, t.as_ptr(), m.as_ptr());
            }
        }
    }
}

/// A PlatformPlugin backed by a dynamically loaded .so.
struct PluginPlatform {
    meta: PluginMeta,
    _lib: std::sync::Arc<libloading::Library>,
}

impl crate::PlatformPlugin for PluginPlatform {
    fn platform_name(&self) -> &str {
        &self.meta.platform_name
    }

    fn plugin_id(&self) -> &str {
        &self.meta.plugin_id
    }

    fn version(&self) -> &str {
        &self.meta.version
    }

    fn priority(&self) -> u32 {
        self.meta.priority
    }
}

/// List all discovered plugin metadata from the standard plugin directories.
pub fn list_available_plugins(paths: &PlatformPaths) -> Vec<PluginMeta> {
    let plugin_dirs = vec![
        paths.plugins_dir.clone(),
        PathBuf::from("/usr/lib/tizenclaw/plugins"),
        PathBuf::from("/usr/local/lib/tizenclaw/plugins"),
    ];

    let mut discovered = Vec::new();
    for dir in &plugin_dirs {
        if dir.is_dir() {
            discover_plugins_in_dir(dir, &mut discovered);
        }
    }
    discovered.sort_by(|a, b| b.priority.cmp(&a.priority));
    discovered
}
