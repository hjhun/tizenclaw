//! Tizen-specific adapters implementing claw-platform traits.
//!
//! Wraps Tizen native APIs (vconf, pkgmgr, app_control, system_info)
//! behind the standard platform trait interfaces.

use crate::framework::{
    AppControlProvider, LogLevel, PackageInfo, PackageManagerProvider, PlatformLogger,
    PlatformPlugin, SystemInfoProvider, SystemEventProvider
};
use serde_json::{json, Value};
use std::process::Command;

// ─────────────────────────────────────────
// TizenPlatform — PlatformPlugin
// ─────────────────────────────────────────

pub struct TizenPlatform;

impl PlatformPlugin for TizenPlatform {
    fn platform_name(&self) -> &str { "Tizen" }
    fn plugin_id(&self) -> &str { "tizen" }
    fn version(&self) -> &str { "1.0.0" }
    fn priority(&self) -> u32 { 100 }

    fn is_compatible(&self) -> bool {
        // Check for Tizen-specific markers
        std::path::Path::new("/etc/tizen-release").exists()
            || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
    }

    fn initialize(&mut self) -> bool {
        // Initialize tizen-core main loop (if needed)
        unsafe {
            crate::tizen_sys::tizen_core::tizen_core_init();
        }
        true
    }

    fn shutdown(&mut self) {
        unsafe {
            crate::tizen_sys::tizen_core::tizen_core_shutdown();
        }
    }
}

// ─────────────────────────────────────────
// TizenSystemInfo — SystemInfoProvider
// ─────────────────────────────────────────

pub struct TizenSystemInfo;

impl SystemInfoProvider for TizenSystemInfo {
    fn get_os_version(&self) -> Option<String> {
        // Try /etc/tizen-release
        std::fs::read_to_string("/etc/tizen-release")
            .ok()
            .map(|s| s.trim().to_string())
    }

    fn get_device_profile(&self) -> Value {
        let mut profile = json!({});

        // CPU info
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            let cores = cpuinfo.matches("processor").count();
            profile["cpu_cores"] = json!(cores);
            for line in cpuinfo.lines() {
                if line.starts_with("model name") {
                    if let Some(name) = line.split(':').nth(1) {
                        profile["cpu_model"] = json!(name.trim());
                        break;
                    }
                }
            }
        }

        // Memory
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let kb: u64 = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok()).unwrap_or(0);
                    profile["memory_mb"] = json!(kb / 1024);
                    break;
                }
            }
        }

        // OS version (Tizen-specific)
        if let Some(ver) = self.get_os_version() {
            profile["os_version"] = json!(ver);
        }

        // Display resolution
        if let Ok(fb) = std::fs::read_to_string("/sys/class/graphics/fb0/virtual_size") {
            profile["display_resolution"] = json!(fb.trim());
        }

        profile
    }

    fn get_battery_level(&self) -> Option<u32> {
        std::fs::read_to_string("/sys/class/power_supply/battery/capacity")
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }
}

// ─────────────────────────────────────────
// TizenPackageManager — PackageManagerProvider
// ─────────────────────────────────────────

pub struct TizenPackageManager;

impl PackageManagerProvider for TizenPackageManager {
    fn list_packages(&self) -> Vec<PackageInfo> {
        let output = Command::new("pkgcmd")
            .args(["--list", "-t", "0"])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_tizen_pkg_list(&stdout)
            }
            Err(e) => {
                log::warn!("TizenPackageManager: pkgcmd failed: {}", e);
                Vec::new()
            }
        }
    }

    fn get_package_info(&self, pkg_id: &str) -> Option<PackageInfo> {
        let output = Command::new("pkgcmd")
            .args(["--info", "-n", pkg_id])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                Some(parse_tizen_pkg_info(&stdout, pkg_id))
            }
            _ => None,
        }
    }

    fn get_packages_by_metadata_key(&self, key: &str) -> Vec<PackageInfo> {
        unsafe {
            use crate::tizen_sys::pkgmgr_info::*;
            let mut filter: pkgmgrinfo_pkginfo_metadata_filter_h = std::ptr::null_mut();
            if pkgmgrinfo_pkginfo_metadata_filter_create(&mut filter) != PMINFO_R_OK {
                return vec![];
            }
            let c_key = std::ffi::CString::new(key).unwrap();
            pkgmgrinfo_pkginfo_metadata_filter_add(filter, c_key.as_ptr(), std::ptr::null());

            let mut pkg_ids: Vec<String> = Vec::new();

            unsafe extern "C" fn filter_cb(handle: pkgmgrinfo_pkginfo_h, user_data: *mut std::os::raw::c_void) -> std::os::raw::c_int {
                let vec_ptr = user_data as *mut Vec<String>;
                let mut c_pkgid: *mut std::os::raw::c_char = std::ptr::null_mut();
                if pkgmgrinfo_pkginfo_get_pkgid(handle, &mut c_pkgid) == PMINFO_R_OK && !c_pkgid.is_null() {
                    let s = std::ffi::CStr::from_ptr(c_pkgid).to_string_lossy().into_owned();
                    log::warn!("Pkgmgr filter matched plugin: {}", s);
                    (*vec_ptr).push(s);
                }
                0 // Return 0 (true/success in some Tizen APIs) to naturally traverse multiple plugins
            }

            pkgmgrinfo_pkginfo_metadata_filter_foreach(
                filter,
                filter_cb,
                &mut pkg_ids as *mut _ as *mut std::os::raw::c_void
            );

            pkgmgrinfo_pkginfo_metadata_filter_destroy(filter);

            pkg_ids.into_iter().map(|id| PackageInfo {
                pkg_id: id,
                installed: true,
                ..Default::default()
            }).collect()
        }
    }

    fn get_package_metadata_value(&self, pkg_id: &str, key: &str) -> Option<String> {
        unsafe {
            use crate::tizen_sys::pkgmgr_info::*;
            let mut pkginfo: pkgmgrinfo_pkginfo_h = std::ptr::null_mut();
            let c_pkgid = std::ffi::CString::new(pkg_id).unwrap();
            let uid = libc::getuid() as std::os::raw::c_int;
            
            if pkgmgrinfo_pkginfo_get_usr_pkginfo(c_pkgid.as_ptr(), uid, &mut pkginfo) != PMINFO_R_OK || pkginfo.is_null() {
                // Fallback to system package info if user-specific info fails
                if pkgmgrinfo_pkginfo_get_pkginfo(c_pkgid.as_ptr(), &mut pkginfo) != PMINFO_R_OK || pkginfo.is_null() {
                    return None;
                }
            }

            let mut c_val: *mut std::os::raw::c_char = std::ptr::null_mut();
            let c_key = std::ffi::CString::new(key).unwrap();
            let mut result = None;

            if pkgmgrinfo_pkginfo_get_metadata_value(pkginfo, c_key.as_ptr(), &mut c_val) == PMINFO_R_OK && !c_val.is_null() {
                result = Some(std::ffi::CStr::from_ptr(c_val).to_string_lossy().into_owned());
            }

            pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
            result
        }
    }

    fn get_package_root_path(&self, pkg_id: &str) -> Option<String> {
        unsafe {
            use crate::tizen_sys::pkgmgr_info::*;
            let mut pkginfo: pkgmgrinfo_pkginfo_h = std::ptr::null_mut();
            let c_pkgid = std::ffi::CString::new(pkg_id).unwrap();
            let uid = libc::getuid() as std::os::raw::c_int;

            if pkgmgrinfo_pkginfo_get_usr_pkginfo(c_pkgid.as_ptr(), uid, &mut pkginfo) != PMINFO_R_OK || pkginfo.is_null() {
                // Fallback to system package info if user-specific info fails
                if pkgmgrinfo_pkginfo_get_pkginfo(c_pkgid.as_ptr(), &mut pkginfo) != PMINFO_R_OK || pkginfo.is_null() {
                    return None;
                }
            }

            let mut c_val: *mut std::os::raw::c_char = std::ptr::null_mut();
            let mut result = None;

            if pkgmgrinfo_pkginfo_get_root_path(pkginfo, &mut c_val) == PMINFO_R_OK && !c_val.is_null() {
                result = Some(std::ffi::CStr::from_ptr(c_val).to_string_lossy().into_owned());
            }

            pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo);
            result
        }
    }
}

pub fn parse_tizen_pkg_list(output: &str) -> Vec<PackageInfo> {
    let mut packages = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            packages.push(PackageInfo {
                pkg_id: parts[0].trim().to_string(),
                version: parts.get(1).unwrap_or(&"").to_string(),
                pkg_type: parts.get(2).unwrap_or(&"").to_string(),
                installed: true,
                ..Default::default()
            });
        }
    }
    packages
}

pub fn parse_tizen_pkg_info(output: &str, pkg_id: &str) -> PackageInfo {
    let mut info = PackageInfo {
        pkg_id: pkg_id.to_string(),
        installed: true,
        ..Default::default()
    };
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some((key, val)) = trimmed.split_once(':') {
            let key = key.trim().to_lowercase();
            let val = val.trim().to_string();
            match key.as_str() {
                "version" => info.version = val,
                "type" => info.pkg_type = val,
                "label" => info.label = val,
                "mainappid" | "main_appid" => info.app_id = val,
                _ => {}
            }
        }
    }
    info
}

// ─────────────────────────────────────────
// TizenAppControl — AppControlProvider
// ─────────────────────────────────────────

pub struct TizenAppControl;

impl AppControlProvider for TizenAppControl {
    fn launch_app(&self, app_id: &str) -> Result<(), String> {
        unsafe {
            use crate::tizen_sys::app_control::*;

            let mut handle: app_control_h = std::ptr::null_mut();
            if app_control_create(&mut handle) != APP_CONTROL_ERROR_NONE {
                return Err("Failed to create app_control".into());
            }

            let c_op = std::ffi::CString::new("http://tizen.org/appcontrol/operation/default")
                .unwrap();
            app_control_set_operation(handle, c_op.as_ptr());

            let c_id = std::ffi::CString::new(app_id)
                .map_err(|_| "Invalid app_id".to_string())?;
            app_control_set_app_id(handle, c_id.as_ptr());

            let result = app_control_send_launch_request(handle, None, std::ptr::null_mut());
            app_control_destroy(handle);

            if result == APP_CONTROL_ERROR_NONE {
                Ok(())
            } else {
                Err(format!("app_control_send_launch_request failed: {}", result))
            }
        }
    }
}
