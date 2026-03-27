//! tizen-sys: FFI bindings for Tizen-SPECIFIC native APIs ONLY.
//!
//! General-purpose libraries (HTTP, JSON, SQLite) are handled by
//! standard Rust crates (ureq, serde_json, rusqlite).
//! This crate provides FFI only for APIs unique to the Tizen platform.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::os::raw::{c_char, c_int, c_void, c_uint};

// ─────────────────────────────────────────
// dlog — Tizen logging
// ─────────────────────────────────────────
pub mod dlog {
    use std::os::raw::{c_char, c_int};

    pub const DLOG_ERROR: c_int = 3;
    pub const DLOG_WARN: c_int = 4;
    pub const DLOG_INFO: c_int = 5;
    pub const DLOG_DEBUG: c_int = 6;

    extern "C" {
        pub fn dlog_print(prio: c_int, tag: *const c_char, fmt: *const c_char, ...) -> c_int;
    }
}

// ─────────────────────────────────────────
// Tizen Core — Main loop
// ─────────────────────────────────────────
pub mod tizen_core {
    use std::os::raw::{c_char, c_int, c_void};

    pub type tizen_core_task_h = *mut c_void;

    extern "C" {
        pub fn tizen_core_init() -> c_int;
        pub fn tizen_core_shutdown() -> c_int;
        pub fn tizen_core_task_create(
            name: *const c_char,
            use_thread: c_int,
            task: *mut tizen_core_task_h,
        ) -> c_int;
        pub fn tizen_core_task_destroy(task: tizen_core_task_h) -> c_int;
        pub fn tizen_core_task_run(task: tizen_core_task_h) -> c_int;
        pub fn tizen_core_task_quit(task: tizen_core_task_h) -> c_int;
    }
}

// ─────────────────────────────────────────
// vconf — Tizen device configuration
// ─────────────────────────────────────────
pub mod vconf {
    use std::os::raw::{c_char, c_int};

    extern "C" {
        pub fn vconf_get_str(key: *const c_char) -> *mut c_char;
        pub fn vconf_get_int(key: *const c_char, val: *mut c_int) -> c_int;
        pub fn vconf_set_str(key: *const c_char, val: *const c_char) -> c_int;
        pub fn vconf_set_int(key: *const c_char, val: c_int) -> c_int;
    }
}

// ─────────────────────────────────────────
// pkgmgr — Tizen package manager
// ─────────────────────────────────────────
pub mod pkgmgr {
    use std::os::raw::{c_char, c_int, c_void};

    pub type pkgmgr_client = c_void;

    pub const PC_LISTENING: c_int = 0;

    pub type pkgmgr_handler = unsafe extern "C" fn(
        target_type: c_int,
        pkg_name: *const c_char,
        event_type: *const c_char,
        key: *const c_char,
        val: *const c_char,
        pmsg: *const c_void,
        data: *mut c_void,
    ) -> c_int;

    extern "C" {
        pub fn pkgmgr_client_new(client_type: c_int) -> *mut pkgmgr_client;
        pub fn pkgmgr_client_free(client: *mut pkgmgr_client) -> c_int;
        pub fn pkgmgr_client_listen_status(
            client: *mut pkgmgr_client,
            handler: pkgmgr_handler,
            data: *mut c_void,
        ) -> c_int;
    }
}

// ─────────────────────────────────────────
// libsoup-2.4 — HTTP Server (Tizen WebDashboard)
// ─────────────────────────────────────────
pub mod soup {
    use std::os::raw::{c_char, c_int, c_uint, c_void};

    pub type gboolean = c_int;
    pub type guint = c_uint;
    pub type gpointer = *mut c_void;
    pub type gsize = usize;
    pub type GType = usize;
    pub type GError = c_void;
    pub type GMainLoop = c_void;
    pub type GMainContext = c_void;

    pub type SoupServer = c_void;
    pub type SoupMessage = c_void;
    pub type SoupMessageHeaders = c_void;

    pub type SoupServerCallback = unsafe extern "C" fn(
        server: *mut SoupServer,
        msg: *mut SoupMessage,
        path: *const c_char,
        query: *mut c_void,
        client: *mut c_void,
        user_data: gpointer,
    );

    pub const SOUP_MEMORY_COPY: c_int = 1;

    extern "C" {
        pub fn g_object_unref(object: gpointer);
        pub fn g_object_new(object_type: GType, first_property_name: *const c_char, ...) -> gpointer;
        pub fn g_main_loop_new(context: *mut GMainContext, is_running: gboolean) -> *mut GMainLoop;
        pub fn g_main_loop_run(loop_: *mut GMainLoop);
        pub fn g_main_loop_quit(loop_: *mut GMainLoop);
        pub fn g_main_loop_unref(loop_: *mut GMainLoop);

        pub fn soup_server_get_type() -> GType;
        pub fn soup_server_listen_all(
            server: *mut SoupServer, port: guint, options: c_int, error: *mut *mut GError,
        ) -> gboolean;
        pub fn soup_server_add_handler(
            server: *mut SoupServer, path: *const c_char,
            callback: SoupServerCallback, user_data: gpointer,
            destroy: Option<unsafe extern "C" fn(gpointer)>,
        );
        pub fn soup_server_disconnect(server: *mut SoupServer);
        pub fn soup_message_set_status(msg: *mut SoupMessage, status_code: guint);
        pub fn soup_message_set_response(
            msg: *mut SoupMessage, content_type: *const c_char,
            resp_use: c_int, resp_body: *const c_char, resp_length: gsize,
        );
        pub fn soup_message_headers_append(
            hdrs: *mut SoupMessageHeaders, name: *const c_char, value: *const c_char,
        );
    }
}

// ─────────────────────────────────────────
// capi-appfw-event — Tizen system events
// ─────────────────────────────────────────
pub mod app_event {
    use std::os::raw::{c_char, c_int, c_void};

    pub type event_handler_h = *mut c_void;

    pub type app_event_cb = unsafe extern "C" fn(
        event_name: *const c_char,
        event_data: *mut c_void,
        user_data: *mut c_void,
    );

    extern "C" {
        pub fn event_add_event_handler(
            event_name: *const c_char,
            callback: app_event_cb,
            user_data: *mut c_void,
            handler: *mut event_handler_h,
        ) -> c_int;
        pub fn event_remove_event_handler(handler: event_handler_h) -> c_int;
    }
}

// ─────────────────────────────────────────
// app_control — Tizen application control (launch, share, etc.)
// ─────────────────────────────────────────
pub mod app_control {
    use std::os::raw::{c_char, c_int, c_void};

    pub type app_control_h = *mut c_void;

    pub const APP_CONTROL_ERROR_NONE: c_int = 0;

    pub type app_control_reply_cb = unsafe extern "C" fn(
        request: app_control_h,
        reply: app_control_h,
        result: c_int,
        user_data: *mut c_void,
    );

    extern "C" {
        pub fn app_control_create(app_control: *mut app_control_h) -> c_int;
        pub fn app_control_destroy(app_control: app_control_h) -> c_int;
        pub fn app_control_set_operation(
            app_control: app_control_h,
            operation: *const c_char,
        ) -> c_int;
        pub fn app_control_set_app_id(
            app_control: app_control_h,
            app_id: *const c_char,
        ) -> c_int;
        pub fn app_control_set_uri(
            app_control: app_control_h,
            uri: *const c_char,
        ) -> c_int;
        pub fn app_control_add_extra_data(
            app_control: app_control_h,
            key: *const c_char,
            value: *const c_char,
        ) -> c_int;
        pub fn app_control_get_extra_data(
            app_control: app_control_h,
            key: *const c_char,
            value: *mut *mut c_char,
        ) -> c_int;
        pub fn app_control_send_launch_request(
            app_control: app_control_h,
            callback: Option<app_control_reply_cb>,
            user_data: *mut c_void,
        ) -> c_int;
    }
}

// ─────────────────────────────────────────
// system_info — Tizen device information
// ─────────────────────────────────────────
pub mod system_info {
    use std::os::raw::{c_char, c_int, c_void};

    pub const SYSTEM_INFO_ERROR_NONE: c_int = 0;

    extern "C" {
        pub fn system_info_get_platform_string(
            key: *const c_char,
            value: *mut *mut c_char,
        ) -> c_int;
        pub fn system_info_get_platform_int(
            key: *const c_char,
            value: *mut c_int,
        ) -> c_int;
        pub fn system_info_get_platform_bool(
            key: *const c_char,
            value: *mut c_int,
        ) -> c_int;
    }
}

// ─────────────────────────────────────────
// alarm — Tizen alarm API for scheduled tasks
// ─────────────────────────────────────────
pub mod alarm {
    use std::os::raw::{c_char, c_int, c_void};

    pub type alarm_id_t = c_int;

    extern "C" {
        pub fn alarm_schedule_after_delay(
            app_control: *mut c_void,
            delay: c_int,
            period: c_int,
            alarm_id: *mut alarm_id_t,
        ) -> c_int;
        pub fn alarm_cancel(alarm_id: alarm_id_t) -> c_int;
        pub fn alarm_cancel_all() -> c_int;
    }
}

// ─────────────────────────────────────────
// bundle — Tizen data bundle (used by app_control extras)
// ─────────────────────────────────────────
pub mod bundle {
    use std::os::raw::{c_char, c_int, c_void};

    pub type bundle = c_void;

    extern "C" {
        pub fn bundle_create() -> *mut bundle;
        pub fn bundle_free(b: *mut bundle) -> c_int;
        pub fn bundle_add_str(
            b: *mut bundle,
            key: *const c_char,
            str_val: *const c_char,
        ) -> c_int;
        pub fn bundle_get_str(
            b: *mut bundle,
            key: *const c_char,
            str_val: *mut *mut c_char,
        ) -> c_int;
    }
}
