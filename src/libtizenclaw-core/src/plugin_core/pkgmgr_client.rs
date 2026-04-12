use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::{Arc, Mutex};

use crate::tizen_sys::glib::*;
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
    gloop: Mutex<Option<*mut GMainLoop>>,
    listeners: Mutex<Vec<Arc<dyn PkgmgrListener>>>,
}

unsafe impl Send for PkgmgrClient {}
unsafe impl Sync for PkgmgrClient {}

impl Default for PkgmgrClient {
    fn default() -> Self {
        Self::new()
    }
}

impl PkgmgrClient {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            gloop: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
        }
    }

    /// Retrieve the global PkgmgrClient instance
    pub fn global() -> Arc<PkgmgrClient> {
        static INSTANCE: std::sync::LazyLock<Arc<PkgmgrClient>> =
            std::sync::LazyLock::new(|| Arc::new(PkgmgrClient::new()));
        INSTANCE.clone()
    }

    pub fn add_listener(&self, listener: Arc<dyn PkgmgrListener>) {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(listener);
        if listeners.len() == 1 {
            drop(listeners);
            self.start_listening();
        }
    }

    fn start_listening(&self) {
        // Early-exit if already started
        {
            let guard = self.handle.lock().unwrap();
            if guard.is_some() {
                return;
            }
        }

        let global_client = Self::global();

        std::thread::spawn(move || {
            unsafe {
                // -----------------------------------------------------------
                // 1. Create a private GLib main context and push it as the
                //    thread-default so that pkgmgr DBus signals are dispatched
                //    on this thread (mirrors what Tizen glib daemons do).
                // -----------------------------------------------------------
                let ctx = g_main_context_new();
                if ctx.is_null() {
                    log::error!("g_main_context_new() failed");
                    return;
                }
                g_main_context_push_thread_default(ctx);

                let gloop = g_main_loop_new(ctx, 0 /* not running */);
                if gloop.is_null() {
                    log::error!("g_main_loop_new() failed");
                    g_main_context_pop_thread_default(ctx);
                    g_main_context_unref(ctx);
                    return;
                }

                // Store loop handle so stop_listening() can quit it
                {
                    let mut lg = global_client.gloop.lock().unwrap();
                    *lg = Some(gloop);
                }

                // -----------------------------------------------------------
                // 2. Create the pkgmgr listening client — exact C++ pattern:
                //      pkgmgr_client_new(PC_LISTENING)       // type = 4
                //      pkgmgr_client_set_status_type(ALL)
                //      pkgmgr_client_listen_status(handler, self)
                // -----------------------------------------------------------
                let handle = pkgmgr_client_new(PC_LISTENING);
                if handle.is_null() {
                    log::error!(
                        "pkgmgr_client_new(PC_LISTENING) failed — \
                         DBus/cynara not ready or privilege missing"
                    );
                    g_main_loop_unref(gloop);
                    g_main_context_pop_thread_default(ctx);
                    g_main_context_unref(ctx);
                    {
                        let mut lg = global_client.gloop.lock().unwrap();
                        *lg = None;
                    }
                    return;
                }
                log::info!("pkgmgr_client_new(PC_LISTENING) success");

                let rc = pkgmgr_client_set_status_type(handle, PKGMGR_CLIENT_STATUS_ALL);
                if rc < 0 {
                    log::warn!("pkgmgr_client_set_status_type() returned {}", rc);
                }

                let user_data = global_client.as_ref() as *const PkgmgrClient as *mut c_void;
                let rc =
                    pkgmgr_client_listen_status(handle, PkgmgrClient::pkgmgr_handler, user_data);
                if rc < 0 {
                    log::error!("pkgmgr_client_listen_status() returned {}", rc);
                    pkgmgr_client_free(handle);
                    g_main_loop_unref(gloop);
                    g_main_context_pop_thread_default(ctx);
                    g_main_context_unref(ctx);
                    {
                        let mut lg = global_client.gloop.lock().unwrap();
                        *lg = None;
                    }
                    return;
                }
                log::info!("pkgmgr listening started — entering GLib main loop");

                {
                    let mut hg = global_client.handle.lock().unwrap();
                    *hg = Some(handle);
                }

                // -----------------------------------------------------------
                // 3. Block on the GLib main loop — DBus install/uninstall
                //    callbacks are dispatched here via pkgmgr socket watch.
                // -----------------------------------------------------------
                g_main_loop_run(gloop);

                // -----------------------------------------------------------
                // 4. Cleanup after loop exits (triggered by stop_listening)
                // -----------------------------------------------------------
                log::info!("GLib loop exited — cleaning up pkgmgr client");
                {
                    let mut hg = global_client.handle.lock().unwrap();
                    if let Some(h) = hg.take() {
                        if !h.is_null() {
                            pkgmgr_client_free(h);
                        }
                    }
                }
                g_main_loop_unref(gloop);
                g_main_context_pop_thread_default(ctx);
                g_main_context_unref(ctx);
            }
        });
    }

    fn stop_listening(&self) {
        unsafe {
            let mut lg = self.gloop.lock().unwrap();
            if let Some(gloop) = lg.take() {
                g_main_loop_quit(gloop);
                // gloop itself freed inside the spawned thread after run() returns
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

        log::debug!(
            "pkgmgr event: uid={} req={} type={} pkgid={} status={} event={}",
            target_uid,
            req_id,
            s_pkg_type,
            s_pkgid,
            s_event_status,
            s_event_name
        );

        let event = Arc::new(PkgmgrEventArgs {
            target_uid,
            req_id,
            pkg_type: s_pkg_type,
            pkgid: s_pkgid,
            event_status: s_event_status,
            event_name: s_event_name,
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
