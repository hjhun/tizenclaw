use std::sync::{Arc, Mutex};
use std::os::raw::{c_char, c_int, c_void};
use std::ffi::CStr;
use log::{info, error, warn};

use crate::tizen_sys::pkgmgr::*;

#[derive(Debug, Clone)]
pub struct PkgmgrEventArgs {
    pub target_uid: u32,
    pub req_id: i32,
    pub pkg_type: String,
    pub pkgid: String,
    pub event_status: String, // e.g., "start", "end", "error"
    pub event_name: String,   // e.g., "install", "uninstall"
}

pub trait PkgmgrListener: Send + Sync {
    fn on_pkgmgr_event(&self, args: Arc<PkgmgrEventArgs>);
}

pub struct PkgmgrClient {
    handle: Mutex<Option<*mut pkgmgr_client>>,
    listeners: Mutex<Vec<Arc<dyn PkgmgrListener>>>,
}

unsafe impl Send for PkgmgrClient {}
unsafe impl Sync for PkgmgrClient {}

impl PkgmgrClient {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
        }
    }

    /// Retrieve the global PkgmgrClient instance
    pub fn global() -> Arc<PkgmgrClient> {
        static INSTANCE: std::sync::LazyLock<Arc<PkgmgrClient>> = std::sync::LazyLock::new(|| Arc::new(PkgmgrClient::new()));
        INSTANCE.clone()
    }

    pub fn add_listener(&self, listener: Arc<dyn PkgmgrListener>) {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(listener);
        if listeners.len() == 1 {
            self.start_listening();
        }
    }

    fn start_listening(&self) {
        unsafe {
            let mut handle_guard = self.handle.lock().unwrap();
            if handle_guard.is_some() {
                return;
            }

            let handle = pkgmgr_client_new(PC_LISTENING);
            if handle.is_null() {
                warn!("pkgmgr_client_new() failed: daemon may not receive dynamic package install events");
                return;
            }

            let ret = pkgmgr_client_set_status_type(handle, PKGMGR_CLIENT_STATUS_ALL);
            if ret < 0 {
                error!("pkgmgr_client_set_status_type() failed: {}", ret);
            }

            let ret = pkgmgr_client_listen_status(
                handle,
                Self::pkgmgr_handler,
                std::ptr::null_mut()
            );
            if ret < 0 {
                error!("pkgmgr_client_listen_status() failed: {}", ret);
            }

            *handle_guard = Some(handle);
            info!("Started PkgmgrClient listening.");
        }
    }

    fn stop_listening(&self) {
        unsafe {
            let mut handle_guard = self.handle.lock().unwrap();
            if let Some(handle) = handle_guard.take() {
                pkgmgr_client_free(handle);
                info!("Stopped PkgmgrClient listening.");
            }
        }
    }

    unsafe extern "C" fn pkgmgr_handler(
        target_uid: u32,
        req_id: c_int,
        pkg_type: *const c_char,
        pkgid: *const c_char,
        key: *const c_char,
        val: *const c_char,
        _pmsg: *const c_void,
        _user_data: *mut c_void,
    ) -> c_int {
        if pkg_type.is_null() || pkgid.is_null() || key.is_null() || val.is_null() {
            return 0;
        }

        let s_pkg_type = CStr::from_ptr(pkg_type).to_string_lossy().into_owned();
        let s_pkgid = CStr::from_ptr(pkgid).to_string_lossy().into_owned();
        let s_event_status = CStr::from_ptr(key).to_string_lossy().into_owned();
        let s_event_name = CStr::from_ptr(val).to_string_lossy().into_owned();

        let event = Arc::new(PkgmgrEventArgs {
            target_uid,
            req_id,
            pkg_type: s_pkg_type,
            pkgid: s_pkgid,
            event_status: s_event_status.clone(),
            event_name: s_event_name.clone(),
        });

        // Trigger all listeners
        let global_client = Self::global();
        let listeners = global_client.listeners.lock().unwrap();
        for listener in listeners.iter() {
            listener.on_pkgmgr_event(event.clone());
        }

        0
    }
}

impl Drop for PkgmgrClient {
    fn drop(&mut self) {
        self.stop_listening();
    }
}
