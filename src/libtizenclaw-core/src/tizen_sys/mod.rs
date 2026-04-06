//! tizen-sys: FFI bindings dynamically loaded via `libloading`
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use libloading::{Library, Symbol};
use std::sync::LazyLock;

static LIB_DLOG: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libdlog.so.0")
        .or_else(|_| Library::new("libdlog.so"))
        .ok()
});
static LIB_TIZEN_CORE: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libtizen-core.so.0")
        .or_else(|_| Library::new("libtizen-core.so"))
        .ok()
});
static LIB_VCONF: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libvconf.so.0")
        .or_else(|_| Library::new("libvconf.so"))
        .ok()
});
static LIB_PKGMGR: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libpkgmgr-client.so.0")
        .or_else(|_| Library::new("libpkgmgr-client.so"))
        .ok()
});
static LIB_PKGMGR_INFO: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libpkgmgr-info.so.0")
        .or_else(|_| Library::new("libpkgmgr-info.so"))
        .ok()
});
static LIB_SOUP: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libsoup-2.4.so.1")
        .or_else(|_| Library::new("libsoup-2.4.so"))
        .ok()
});
static LIB_APP_EVENT: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libcapi-appfw-event.so.0")
        .or_else(|_| Library::new("libcapi-appfw-event.so"))
        .ok()
});
static LIB_APP_CONTROL: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libcapi-appfw-app-manager.so.0")
        .or_else(|_| Library::new("libcapi-appfw-app-manager.so"))
        .ok()
});
static LIB_SYSTEM_INFO: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libcapi-system-info.so.0")
        .or_else(|_| Library::new("libcapi-system-info.so"))
        .ok()
});
static LIB_ALARM: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libcapi-appfw-alarm.so.0")
        .or_else(|_| Library::new("libcapi-appfw-alarm.so"))
        .ok()
});
static LIB_BUNDLE: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libbundle.so.0")
        .or_else(|_| Library::new("libbundle.so"))
        .ok()
});
static LIB_AUL: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libaul.so.0")
        .or_else(|_| Library::new("libaul.so"))
        .ok()
});
static LIB_ACTION: LazyLock<Option<Library>> = LazyLock::new(|| unsafe {
    Library::new("libcapi-appfw-tizen-action.so.1")
        .or_else(|_| Library::new("libtizen-action.so.1"))
        .or_else(|_| Library::new("libtizen-action.so"))
        .ok()
});

macro_rules! dlsym_call {
    ($lib:expr, $sym:expr, $sig:ty, $fallback:expr $(, $args:expr)*) => {{
        if let Some(lib) = $lib.as_ref() {
            unsafe {
                if let Ok(func) = lib.get::<Symbol<$sig>>($sym) {
                    return func($($args),*);
                }
            }
        }
        $fallback
    }};
}

// ─────────────────────────────────────────
// dlog — Tizen logging
// ─────────────────────────────────────────
pub mod dlog {
    use super::*;
    use std::os::raw::{c_char, c_int};

    pub const DLOG_ERROR: c_int = 6;
    pub const DLOG_WARN: c_int = 5;
    pub const DLOG_INFO: c_int = 4;
    pub const DLOG_DEBUG: c_int = 3;

    pub unsafe fn dlog_print(prio: c_int, tag: *const c_char, fmt: *const c_char) -> c_int {
        dlsym_call!(
            LIB_DLOG,
            b"dlog_print\0",
            unsafe extern "C" fn(c_int, *const c_char, *const c_char) -> c_int,
            {
                let tag_str = std::ffi::CStr::from_ptr(tag).to_string_lossy();
                let fmt_str = std::ffi::CStr::from_ptr(fmt).to_string_lossy();
                println!("[MOCK DLOG {}] [{}] {}", prio, tag_str, fmt_str);
                0
            },
            prio,
            tag,
            fmt
        )
    }
}

// ─────────────────────────────────────────
// Tizen Core — Main loop
// ─────────────────────────────────────────
pub mod tizen_core {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type tizen_core_task_h = *mut c_void;
    pub type tizen_core_h = *mut c_void;

    pub unsafe fn tizen_core_init() -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_init\0",
            unsafe extern "C" fn() -> c_int,
            0
        )
    }

    pub unsafe fn tizen_core_shutdown() -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_shutdown\0",
            unsafe extern "C" fn() -> c_int,
            0
        )
    }

    pub unsafe fn tizen_core_task_create(
        name: *const c_char,
        use_thread: c_int,
        task: *mut tizen_core_task_h,
    ) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_task_create\0",
            unsafe extern "C" fn(*const c_char, c_int, *mut tizen_core_task_h) -> c_int,
            0,
            name,
            use_thread,
            task
        )
    }

    pub unsafe fn tizen_core_task_destroy(task: tizen_core_task_h) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_task_destroy\0",
            unsafe extern "C" fn(tizen_core_task_h) -> c_int,
            0,
            task
        )
    }

    pub unsafe fn tizen_core_task_run(task: tizen_core_task_h) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_task_run\0",
            unsafe extern "C" fn(tizen_core_task_h) -> c_int,
            0,
            task
        )
    }

    pub unsafe fn tizen_core_task_quit(task: tizen_core_task_h) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_task_quit\0",
            unsafe extern "C" fn(tizen_core_task_h) -> c_int,
            0,
            task
        )
    }

    pub unsafe fn tizen_core_task_get_tizen_core(
        task: tizen_core_task_h,
        core: *mut tizen_core_h,
    ) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_task_get_tizen_core\0",
            unsafe extern "C" fn(tizen_core_task_h, *mut tizen_core_h) -> c_int,
            -1,
            task,
            core
        )
    }

    pub unsafe fn tizen_core_get_glib_context(core: tizen_core_h) -> *mut c_void {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_get_glib_context\0",
            unsafe extern "C" fn(tizen_core_h) -> *mut c_void,
            std::ptr::null_mut(),
            core
        )
    }

    pub type tizen_core_task_cb = unsafe extern "C" fn(*mut c_void) -> bool;
    pub type tizen_core_source_h = *mut c_void;

    pub unsafe fn tizen_core_add_idle_job(
        core: tizen_core_h,
        callback: tizen_core_task_cb,
        user_data: *mut c_void,
        source: *mut tizen_core_source_h,
    ) -> c_int {
        dlsym_call!(
            LIB_TIZEN_CORE,
            b"tizen_core_add_idle_job\0",
            unsafe extern "C" fn(
                tizen_core_h,
                tizen_core_task_cb,
                *mut c_void,
                *mut tizen_core_source_h,
            ) -> c_int,
            -1,
            core,
            callback,
            user_data,
            source
        )
    }
}

// ─────────────────────────────────────────
// vconf — Tizen device configuration
// ─────────────────────────────────────────
pub mod vconf {
    use super::*;
    use std::os::raw::{c_char, c_int};

    pub unsafe fn vconf_get_str(key: *const c_char) -> *mut c_char {
        dlsym_call!(
            LIB_VCONF,
            b"vconf_get_str\0",
            unsafe extern "C" fn(*const c_char) -> *mut c_char,
            std::ptr::null_mut(),
            key
        )
    }

    pub unsafe fn vconf_get_int(key: *const c_char, val: *mut c_int) -> c_int {
        dlsym_call!(
            LIB_VCONF,
            b"vconf_get_int\0",
            unsafe extern "C" fn(*const c_char, *mut c_int) -> c_int,
            {
                *val = 0;
                0
            },
            key,
            val
        )
    }

    pub unsafe fn vconf_set_str(key: *const c_char, val: *const c_char) -> c_int {
        dlsym_call!(
            LIB_VCONF,
            b"vconf_set_str\0",
            unsafe extern "C" fn(*const c_char, *const c_char) -> c_int,
            0,
            key,
            val
        )
    }

    pub unsafe fn vconf_set_int(key: *const c_char, val: c_int) -> c_int {
        dlsym_call!(
            LIB_VCONF,
            b"vconf_set_int\0",
            unsafe extern "C" fn(*const c_char, c_int) -> c_int,
            0,
            key,
            val
        )
    }
}

// ─────────────────────────────────────────
// pkgmgr — Tizen package manager
// ─────────────────────────────────────────
pub mod pkgmgr {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type pkgmgr_client = c_void;
    pub const PC_LISTENING: c_int = 1; // PC_REQUEST=0, PC_LISTENING=1, PC_BROADCAST=2
    pub const PKGMGR_CLIENT_STATUS_ALL: c_int = 0;

    pub type pkgmgr_handler = unsafe extern "C" fn(
        u32,
        c_int,
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_void,
        *mut c_void,
    ) -> c_int;

    pub unsafe fn pkgmgr_client_new(client_type: c_int) -> *mut pkgmgr_client {
        dlsym_call!(
            LIB_PKGMGR,
            b"pkgmgr_client_new\0",
            unsafe extern "C" fn(c_int) -> *mut pkgmgr_client,
            std::ptr::null_mut(),
            client_type
        )
    }

    pub unsafe fn pkgmgr_client_free(client: *mut pkgmgr_client) -> c_int {
        dlsym_call!(
            LIB_PKGMGR,
            b"pkgmgr_client_free\0",
            unsafe extern "C" fn(*mut pkgmgr_client) -> c_int,
            0,
            client
        )
    }

    pub unsafe fn pkgmgr_client_set_status_type(
        client: *mut pkgmgr_client,
        status_type: c_int,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR,
            b"pkgmgr_client_set_status_type\0",
            unsafe extern "C" fn(*mut pkgmgr_client, c_int) -> c_int,
            -1,
            client,
            status_type
        )
    }

    pub unsafe fn pkgmgr_client_listen_status(
        client: *mut pkgmgr_client,
        handler: pkgmgr_handler,
        data: *mut c_void,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR,
            b"pkgmgr_client_listen_status\0",
            unsafe extern "C" fn(*mut pkgmgr_client, pkgmgr_handler, *mut c_void) -> c_int,
            0,
            client,
            handler,
            data
        )
    }
}

// ─────────────────────────────────────────
// pkgmgr-info — Tizen package information
// ─────────────────────────────────────────
pub mod pkgmgr_info {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;

    pub type pkgmgrinfo_pkginfo_h = *mut c_void;
    pub type pkgmgrinfo_pkginfo_filter_h = *mut c_void;
    pub const PMINFO_R_OK: c_int = 0;

    pub unsafe fn pkgmgrinfo_pkginfo_filter_create(
        filter: *mut pkgmgrinfo_pkginfo_filter_h,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_filter_create\0",
            unsafe extern "C" fn(*mut pkgmgrinfo_pkginfo_filter_h) -> c_int,
            -1,
            filter
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_metadata_filter_create(
        filter: *mut pkgmgrinfo_pkginfo_filter_h,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_metadata_filter_create\0",
            unsafe extern "C" fn(*mut pkgmgrinfo_pkginfo_filter_h) -> c_int,
            -1,
            filter
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_metadata_filter_add(
        filter: pkgmgrinfo_pkginfo_filter_h,
        key: *const c_char,
        val: *const c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_metadata_filter_add\0",
            unsafe extern "C" fn(
                pkgmgrinfo_pkginfo_filter_h,
                *const c_char,
                *const c_char,
            ) -> c_int,
            -1,
            filter,
            key,
            val
        )
    }

    pub type pkgmgrinfo_pkginfo_metadata_filter_cb =
        unsafe extern "C" fn(pkgmgrinfo_pkginfo_h, *mut c_void) -> c_int;

    pub unsafe fn pkgmgrinfo_pkginfo_metadata_filter_foreach(
        filter: pkgmgrinfo_pkginfo_filter_h,
        callback: pkgmgrinfo_pkginfo_metadata_filter_cb,
        user_data: *mut c_void,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_metadata_filter_foreach\0",
            unsafe extern "C" fn(
                pkgmgrinfo_pkginfo_filter_h,
                pkgmgrinfo_pkginfo_metadata_filter_cb,
                *mut c_void,
            ) -> c_int,
            -1,
            filter,
            callback,
            user_data
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_filter_destroy(filter: pkgmgrinfo_pkginfo_filter_h) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_filter_destroy\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_filter_h) -> c_int,
            -1,
            filter
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_metadata_filter_destroy(
        filter: pkgmgrinfo_pkginfo_filter_h,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_metadata_filter_destroy\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_filter_h) -> c_int,
            -1,
            filter
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_pkgid(
        handle: pkgmgrinfo_pkginfo_h,
        pkgid: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_pkgid\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_h, *mut *mut c_char) -> c_int,
            -1,
            handle,
            pkgid
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_pkginfo(
        pkgid: *const c_char,
        pkginfo: *mut pkgmgrinfo_pkginfo_h,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_pkginfo\0",
            unsafe extern "C" fn(*const c_char, *mut pkgmgrinfo_pkginfo_h) -> c_int,
            -1,
            pkgid,
            pkginfo
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_usr_pkginfo(
        pkgid: *const c_char,
        uid: c_int,
        pkginfo: *mut pkgmgrinfo_pkginfo_h,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_usr_pkginfo\0",
            unsafe extern "C" fn(*const c_char, c_int, *mut pkgmgrinfo_pkginfo_h) -> c_int,
            -1,
            pkgid,
            uid,
            pkginfo
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_root_path(
        pkginfo: pkgmgrinfo_pkginfo_h,
        path: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_root_path\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_h, *mut *mut c_char) -> c_int,
            -1,
            pkginfo,
            path
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_res_path(
        pkginfo: pkgmgrinfo_pkginfo_h,
        path: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_res_path\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_h, *mut *mut c_char) -> c_int,
            -1,
            pkginfo,
            path
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_get_metadata_value(
        pkginfo: pkgmgrinfo_pkginfo_h,
        key: *const c_char,
        value: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_get_metadata_value\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_h, *const c_char, *mut *mut c_char) -> c_int,
            -1,
            pkginfo,
            key,
            value
        )
    }

    pub unsafe fn pkgmgrinfo_pkginfo_destroy_pkginfo(pkginfo: pkgmgrinfo_pkginfo_h) -> c_int {
        dlsym_call!(
            LIB_PKGMGR_INFO,
            b"pkgmgrinfo_pkginfo_destroy_pkginfo\0",
            unsafe extern "C" fn(pkgmgrinfo_pkginfo_h) -> c_int,
            -1,
            pkginfo
        )
    }
}

// ─────────────────────────────────────────
// GLib — Main loop (resolved via libsoup-2.4 which depends on libglib-2.0)
// ─────────────────────────────────────────
pub mod glib {
    use super::*;
    use std::os::raw::{c_int, c_void};

    pub type GMainLoop = c_void;
    pub type GMainContext = c_void;

    pub unsafe fn g_main_context_new() -> *mut GMainContext {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_new\0",
            unsafe extern "C" fn() -> *mut GMainContext,
            std::ptr::null_mut()
        )
    }

    pub unsafe fn g_main_context_push_thread_default(context: *mut GMainContext) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_push_thread_default\0",
            unsafe extern "C" fn(*mut GMainContext),
            (),
            context
        )
    }

    pub unsafe fn g_main_context_pop_thread_default(context: *mut GMainContext) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_pop_thread_default\0",
            unsafe extern "C" fn(*mut GMainContext),
            (),
            context
        )
    }

    pub unsafe fn g_main_context_unref(context: *mut GMainContext) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_unref\0",
            unsafe extern "C" fn(*mut GMainContext),
            (),
            context
        )
    }

    pub unsafe fn g_main_loop_new(context: *mut GMainContext, is_running: c_int) -> *mut GMainLoop {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_new\0",
            unsafe extern "C" fn(*mut GMainContext, c_int) -> *mut GMainLoop,
            std::ptr::null_mut(),
            context,
            is_running
        )
    }

    pub unsafe fn g_main_loop_run(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_run\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }

    pub unsafe fn g_main_loop_quit(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_quit\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }

    pub unsafe fn g_main_loop_unref(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_unref\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }
}

// ─────────────────────────────────────────
// libsoup-2.4 — HTTP Server
// ─────────────────────────────────────────
pub mod soup {
    use super::*;
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
        *mut SoupServer,
        *mut SoupMessage,
        *const c_char,
        *mut c_void,
        *mut c_void,
        gpointer,
    );

    pub const SOUP_MEMORY_COPY: c_int = 1;

    pub unsafe fn g_object_unref(object: gpointer) {
        dlsym_call!(
            LIB_SOUP,
            b"g_object_unref\0",
            unsafe extern "C" fn(gpointer),
            (),
            object
        )
    }

    pub unsafe fn g_object_new(object_type: GType, first_property_name: *const c_char) -> gpointer {
        dlsym_call!(
            LIB_SOUP,
            b"g_object_new\0",
            unsafe extern "C" fn(GType, *const c_char) -> gpointer,
            std::ptr::null_mut(),
            object_type,
            first_property_name
        )
    }

    pub unsafe fn g_main_context_push_thread_default(context: *mut GMainContext) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_push_thread_default\0",
            unsafe extern "C" fn(*mut GMainContext),
            (),
            context
        )
    }

    pub unsafe fn g_main_context_new() -> *mut GMainContext {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_context_new\0",
            unsafe extern "C" fn() -> *mut GMainContext,
            std::ptr::null_mut()
        )
    }

    pub unsafe fn g_main_loop_new(
        context: *mut GMainContext,
        is_running: gboolean,
    ) -> *mut GMainLoop {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_new\0",
            unsafe extern "C" fn(*mut GMainContext, gboolean) -> *mut GMainLoop,
            std::ptr::null_mut(),
            context,
            is_running
        )
    }

    pub unsafe fn g_main_loop_run(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_run\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }

    pub unsafe fn g_main_loop_quit(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_quit\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }

    pub unsafe fn g_main_loop_unref(loop_: *mut GMainLoop) {
        dlsym_call!(
            LIB_SOUP,
            b"g_main_loop_unref\0",
            unsafe extern "C" fn(*mut GMainLoop),
            (),
            loop_
        )
    }

    pub unsafe fn soup_server_get_type() -> GType {
        dlsym_call!(
            LIB_SOUP,
            b"soup_server_get_type\0",
            unsafe extern "C" fn() -> GType,
            0
        )
    }

    pub unsafe fn soup_server_listen_all(
        server: *mut SoupServer,
        port: guint,
        options: c_int,
        error: *mut *mut GError,
    ) -> gboolean {
        dlsym_call!(
            LIB_SOUP,
            b"soup_server_listen_all\0",
            unsafe extern "C" fn(*mut SoupServer, guint, c_int, *mut *mut GError) -> gboolean,
            1,
            server,
            port,
            options,
            error
        )
    }

    pub unsafe fn soup_server_add_handler(
        server: *mut SoupServer,
        path: *const c_char,
        callback: SoupServerCallback,
        user_data: gpointer,
        destroy: Option<unsafe extern "C" fn(gpointer)>,
    ) {
        dlsym_call!(
            LIB_SOUP,
            b"soup_server_add_handler\0",
            unsafe extern "C" fn(
                *mut SoupServer,
                *const c_char,
                SoupServerCallback,
                gpointer,
                Option<unsafe extern "C" fn(gpointer)>,
            ),
            (),
            server,
            path,
            callback,
            user_data,
            destroy
        )
    }

    pub unsafe fn soup_server_disconnect(server: *mut SoupServer) {
        dlsym_call!(
            LIB_SOUP,
            b"soup_server_disconnect\0",
            unsafe extern "C" fn(*mut SoupServer),
            (),
            server
        )
    }

    pub unsafe fn soup_message_set_status(msg: *mut SoupMessage, status_code: guint) {
        dlsym_call!(
            LIB_SOUP,
            b"soup_message_set_status\0",
            unsafe extern "C" fn(*mut SoupMessage, guint),
            (),
            msg,
            status_code
        )
    }

    pub unsafe fn soup_message_set_response(
        msg: *mut SoupMessage,
        content_type: *const c_char,
        resp_use: c_int,
        resp_body: *const c_char,
        resp_length: gsize,
    ) {
        dlsym_call!(
            LIB_SOUP,
            b"soup_message_set_response\0",
            unsafe extern "C" fn(*mut SoupMessage, *const c_char, c_int, *const c_char, gsize),
            (),
            msg,
            content_type,
            resp_use,
            resp_body,
            resp_length
        )
    }

    pub unsafe fn soup_message_headers_append(
        hdrs: *mut SoupMessageHeaders,
        name: *const c_char,
        value: *const c_char,
    ) {
        dlsym_call!(
            LIB_SOUP,
            b"soup_message_headers_append\0",
            unsafe extern "C" fn(*mut SoupMessageHeaders, *const c_char, *const c_char),
            (),
            hdrs,
            name,
            value
        )
    }
}

// ─────────────────────────────────────────
// capi-appfw-event — Tizen system events
// ─────────────────────────────────────────
pub mod app_event {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type event_handler_h = *mut c_void;
    pub type app_event_cb = unsafe extern "C" fn(*const c_char, *mut c_void, *mut c_void);

    pub unsafe fn event_add_event_handler(
        event_name: *const c_char,
        callback: app_event_cb,
        user_data: *mut c_void,
        handler: *mut event_handler_h,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_EVENT,
            b"event_add_event_handler\0",
            unsafe extern "C" fn(
                *const c_char,
                app_event_cb,
                *mut c_void,
                *mut event_handler_h,
            ) -> c_int,
            0,
            event_name,
            callback,
            user_data,
            handler
        )
    }

    pub unsafe fn event_remove_event_handler(handler: event_handler_h) -> c_int {
        dlsym_call!(
            LIB_APP_EVENT,
            b"event_remove_event_handler\0",
            unsafe extern "C" fn(event_handler_h) -> c_int,
            0,
            handler
        )
    }
}

// ─────────────────────────────────────────
// app_control — Tizen application control
// ─────────────────────────────────────────
pub mod app_control {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type app_control_h = *mut c_void;
    pub const APP_CONTROL_ERROR_NONE: c_int = 0;

    pub type app_control_reply_cb =
        unsafe extern "C" fn(app_control_h, app_control_h, c_int, *mut c_void);

    pub unsafe fn app_control_create(app_control: *mut app_control_h) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_create\0",
            unsafe extern "C" fn(*mut app_control_h) -> c_int,
            0,
            app_control
        )
    }

    pub unsafe fn app_control_destroy(app_control: app_control_h) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_destroy\0",
            unsafe extern "C" fn(app_control_h) -> c_int,
            0,
            app_control
        )
    }

    pub unsafe fn app_control_set_operation(
        app_control: app_control_h,
        operation: *const c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_set_operation\0",
            unsafe extern "C" fn(app_control_h, *const c_char) -> c_int,
            0,
            app_control,
            operation
        )
    }

    pub unsafe fn app_control_set_app_id(
        app_control: app_control_h,
        app_id: *const c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_set_app_id\0",
            unsafe extern "C" fn(app_control_h, *const c_char) -> c_int,
            0,
            app_control,
            app_id
        )
    }

    pub unsafe fn app_control_set_uri(app_control: app_control_h, uri: *const c_char) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_set_uri\0",
            unsafe extern "C" fn(app_control_h, *const c_char) -> c_int,
            0,
            app_control,
            uri
        )
    }

    pub unsafe fn app_control_add_extra_data(
        app_control: app_control_h,
        key: *const c_char,
        value: *const c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_add_extra_data\0",
            unsafe extern "C" fn(app_control_h, *const c_char, *const c_char) -> c_int,
            0,
            app_control,
            key,
            value
        )
    }

    pub unsafe fn app_control_get_extra_data(
        app_control: app_control_h,
        key: *const c_char,
        value: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_get_extra_data\0",
            unsafe extern "C" fn(app_control_h, *const c_char, *mut *mut c_char) -> c_int,
            0,
            app_control,
            key,
            value
        )
    }

    pub unsafe fn app_control_send_launch_request(
        app_control: app_control_h,
        callback: Option<app_control_reply_cb>,
        user_data: *mut c_void,
    ) -> c_int {
        dlsym_call!(
            LIB_APP_CONTROL,
            b"app_control_send_launch_request\0",
            unsafe extern "C" fn(app_control_h, Option<app_control_reply_cb>, *mut c_void) -> c_int,
            0,
            app_control,
            callback,
            user_data
        )
    }
}

// ─────────────────────────────────────────
// system_info — Tizen device information
// ─────────────────────────────────────────
pub mod system_info {
    use super::*;
    use std::os::raw::{c_char, c_int};

    pub const SYSTEM_INFO_ERROR_NONE: c_int = 0;

    pub unsafe fn system_info_get_platform_string(
        key: *const c_char,
        value: *mut *mut c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_SYSTEM_INFO,
            b"system_info_get_platform_string\0",
            unsafe extern "C" fn(*const c_char, *mut *mut c_char) -> c_int,
            0,
            key,
            value
        )
    }

    pub unsafe fn system_info_get_platform_int(key: *const c_char, value: *mut c_int) -> c_int {
        dlsym_call!(
            LIB_SYSTEM_INFO,
            b"system_info_get_platform_int\0",
            unsafe extern "C" fn(*const c_char, *mut c_int) -> c_int,
            0,
            key,
            value
        )
    }

    pub unsafe fn system_info_get_platform_bool(key: *const c_char, value: *mut c_int) -> c_int {
        dlsym_call!(
            LIB_SYSTEM_INFO,
            b"system_info_get_platform_bool\0",
            unsafe extern "C" fn(*const c_char, *mut c_int) -> c_int,
            0,
            key,
            value
        )
    }
}

// ─────────────────────────────────────────
// alarm — Tizen alarm API for scheduled tasks
// ─────────────────────────────────────────
pub mod alarm {
    use super::*;
    use std::os::raw::{c_int, c_void};

    pub type alarm_id_t = c_int;

    pub unsafe fn alarm_schedule_after_delay(
        app_control: *mut c_void,
        delay: c_int,
        period: c_int,
        alarm_id: *mut alarm_id_t,
    ) -> c_int {
        dlsym_call!(
            LIB_ALARM,
            b"alarm_schedule_after_delay\0",
            unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut alarm_id_t) -> c_int,
            0,
            app_control,
            delay,
            period,
            alarm_id
        )
    }

    pub unsafe fn alarm_cancel(alarm_id: alarm_id_t) -> c_int {
        dlsym_call!(
            LIB_ALARM,
            b"alarm_cancel\0",
            unsafe extern "C" fn(alarm_id_t) -> c_int,
            0,
            alarm_id
        )
    }

    pub unsafe fn alarm_cancel_all() -> c_int {
        dlsym_call!(
            LIB_ALARM,
            b"alarm_cancel_all\0",
            unsafe extern "C" fn() -> c_int,
            0
        )
    }
}

// ─────────────────────────────────────────
// bundle — Tizen data bundle
// ─────────────────────────────────────────
pub mod bundle {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type bundle = c_void;

    pub unsafe fn bundle_create() -> *mut bundle {
        dlsym_call!(
            LIB_BUNDLE,
            b"bundle_create\0",
            unsafe extern "C" fn() -> *mut bundle,
            std::ptr::null_mut()
        )
    }

    pub unsafe fn bundle_free(b: *mut bundle) -> c_int {
        dlsym_call!(
            LIB_BUNDLE,
            b"bundle_free\0",
            unsafe extern "C" fn(*mut bundle) -> c_int,
            0,
            b
        )
    }

    pub unsafe fn bundle_add_str(
        b: *mut bundle,
        key: *const c_char,
        str_val: *const c_char,
    ) -> c_int {
        dlsym_call!(
            LIB_BUNDLE,
            b"bundle_add_str\0",
            unsafe extern "C" fn(*mut bundle, *const c_char, *const c_char) -> c_int,
            0,
            b,
            key,
            str_val
        )
    }
}

// ─────────────────────────────────────────
// aul — Tizen application utility launcher
// ─────────────────────────────────────────
pub mod aul {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub unsafe fn aul_launch_app(app_id: *const c_char, bundle: *mut c_void) -> c_int {
        dlsym_call!(
            LIB_AUL,
            b"aul_launch_app\0",
            unsafe extern "C" fn(*const c_char, *mut c_void) -> c_int,
            -1,
            app_id,
            bundle
        )
    }

    pub unsafe fn aul_open_app(app_id: *const c_char) -> c_int {
        dlsym_call!(
            LIB_AUL,
            b"aul_open_app\0",
            unsafe extern "C" fn(*const c_char) -> c_int,
            -1,
            app_id
        )
    }
}

// ─────────────────────────────────────────
// action — Tizen Action Framework
// ─────────────────────────────────────────
pub mod action {
    use super::*;
    use std::os::raw::{c_char, c_int, c_void};

    pub type action_client_h = *mut c_void;
    pub type action_h = *mut c_void;
    pub type action_event_handler_h = *mut c_void;

    pub const ACTION_ERROR_NONE: c_int = 0;

    #[repr(C)]
    pub enum action_event_type_e {
        ACTION_EVENT_TYPE_INSTALL = 0,
        ACTION_EVENT_TYPE_UNINSTALL = 1,
        ACTION_EVENT_TYPE_UPDATE = 2,
    }

    pub type action_foreach_action_cb = unsafe extern "C" fn(action_h, *mut c_void) -> bool;
    pub type action_result_cb = unsafe extern "C" fn(c_int, *const c_char, *mut c_void);
    pub type action_event_cb =
        unsafe extern "C" fn(*const c_char, action_event_type_e, *mut c_void);

    pub unsafe fn action_client_create(client: *mut action_client_h) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_create\0",
            unsafe extern "C" fn(*mut action_client_h) -> c_int,
            -1,
            client
        )
    }

    pub unsafe fn action_client_destroy(client: action_client_h) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_destroy\0",
            unsafe extern "C" fn(action_client_h) -> c_int,
            -1,
            client
        )
    }

    pub unsafe fn action_client_get_action(
        client: action_client_h,
        name: *const c_char,
        action: *mut action_h,
    ) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_get_action\0",
            unsafe extern "C" fn(action_client_h, *const c_char, *mut action_h) -> c_int,
            -1,
            client,
            name,
            action
        )
    }

    pub unsafe fn action_client_foreach_action(
        client: action_client_h,
        cb: action_foreach_action_cb,
        user_data: *mut c_void,
    ) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_foreach_action\0",
            unsafe extern "C" fn(action_client_h, action_foreach_action_cb, *mut c_void) -> c_int,
            -1,
            client,
            cb,
            user_data
        )
    }

    pub unsafe fn action_client_execute(
        client: action_client_h,
        model: *const c_char,
        cb: action_result_cb,
        user_data: *mut c_void,
    ) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_execute\0",
            unsafe extern "C" fn(
                action_client_h,
                *const c_char,
                action_result_cb,
                *mut c_void,
            ) -> c_int,
            -1,
            client,
            model,
            cb,
            user_data
        )
    }

    pub unsafe fn action_client_add_event_handler(
        client: action_client_h,
        cb: action_event_cb,
        user_data: *mut c_void,
        handler: *mut action_event_handler_h,
    ) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_add_event_handler\0",
            unsafe extern "C" fn(
                action_client_h,
                action_event_cb,
                *mut c_void,
                *mut action_event_handler_h,
            ) -> c_int,
            -1,
            client,
            cb,
            user_data,
            handler
        )
    }

    pub unsafe fn action_client_remove_event_handler(
        client: action_client_h,
        handler: action_event_handler_h,
    ) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_client_remove_event_handler\0",
            unsafe extern "C" fn(action_client_h, action_event_handler_h) -> c_int,
            -1,
            client,
            handler
        )
    }

    pub unsafe fn action_clone(action: action_h, clone: *mut action_h) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_clone\0",
            unsafe extern "C" fn(action_h, *mut action_h) -> c_int,
            -1,
            action,
            clone
        )
    }

    pub unsafe fn action_get_name(action: action_h, name: *mut *mut c_char) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_get_name\0",
            unsafe extern "C" fn(action_h, *mut *mut c_char) -> c_int,
            -1,
            action,
            name
        )
    }

    pub unsafe fn action_get_schema(action: action_h, json_schema: *mut *mut c_char) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_get_schema\0",
            unsafe extern "C" fn(action_h, *mut *mut c_char) -> c_int,
            -1,
            action,
            json_schema
        )
    }

    pub unsafe fn action_destroy(action: action_h) -> c_int {
        dlsym_call!(
            LIB_ACTION,
            b"action_destroy\0",
            unsafe extern "C" fn(action_h) -> c_int,
            -1,
            action
        )
    }
}
