#!/usr/bin/env python3
"""tizen-app-manager-cli — Python port"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_appmgr = _load("libcapi-appfw-app-manager.so.0")
_appctrl = _load("libcapi-appfw-app-control.so.0")
_pkgmgr = _load("libcapi-appfw-package-manager.so.0")

# Callback type for app iteration
APPCB = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
RUNCB = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)

_apps_list = []

def _app_cb(info, user_data):
    if not _appmgr: return False
    app_id = ctypes.c_char_p()
    _appmgr.app_info_get_app_id(info, ctypes.byref(app_id))
    label = ctypes.c_char_p()
    _appmgr.app_info_get_label(info, ctypes.byref(label))
    _apps_list.append({
        "app_id": app_id.value.decode() if app_id.value else "",
        "label": label.value.decode() if label.value else ""
    })
    return True

def list_apps():
    if not _appmgr: return json.dumps({"error":"app_manager not available"})
    global _apps_list; _apps_list = []
    cb = APPCB(_app_cb)
    _appmgr.app_manager_foreach_app_info(cb, None)
    return json.dumps({"status":"success","apps":_apps_list})

_run_list = []
def _run_cb(ctx, user_data):
    if not _appmgr: return False
    app_id = ctypes.c_char_p()
    _appmgr.app_context_get_app_id(ctx, ctypes.byref(app_id))
    pid = ctypes.c_int()
    _appmgr.app_context_get_pid(ctx, ctypes.byref(pid))
    _run_list.append({"app_id": app_id.value.decode() if app_id.value else "", "pid": pid.value})
    return True

def list_running():
    if not _appmgr: return json.dumps({"error":"app_manager not available"})
    global _run_list; _run_list = []
    cb = RUNCB(_run_cb)
    _appmgr.app_manager_foreach_app_context(cb, None)
    return json.dumps({"status":"success","running_apps":_run_list})

def terminate(app_id):
    if not _appmgr: return json.dumps({"error":"app_manager not available"})
    ctx = ctypes.c_void_p()
    r = _appmgr.app_manager_get_app_context(app_id.encode(), ctypes.byref(ctx))
    if r != 0: return json.dumps({"error":"app not running","code":r})
    r = _appmgr.app_manager_terminate_app(ctx)
    return json.dumps({"status":"success" if r == 0 else "error","code":r})

def launch(app_id, operation, uri, mime):
    if not _appctrl: return json.dumps({"error":"app_control not available"})
    h = ctypes.c_void_p()
    _appctrl.app_control_create(ctypes.byref(h))
    if app_id: _appctrl.app_control_set_app_id(h, app_id.encode())
    if operation: _appctrl.app_control_set_operation(h, operation.encode())
    if uri: _appctrl.app_control_set_uri(h, uri.encode())
    if mime: _appctrl.app_control_set_mime(h, mime.encode())
    r = _appctrl.app_control_send_launch_request(h, None, None)
    _appctrl.app_control_destroy(h)
    return json.dumps({"status":"success" if r == 0 else "error","code":r})

def package_info(pkg_id):
    if not _pkgmgr: return json.dumps({"error":"package_manager not available"})
    h = ctypes.c_void_p()
    r = _pkgmgr.package_info_create(pkg_id.encode(), ctypes.byref(h))
    if r != 0: return json.dumps({"error":"package not found","code":r})
    ver = ctypes.c_char_p()
    _pkgmgr.package_info_get_version(h, ctypes.byref(ver))
    typ = ctypes.c_char_p()
    _pkgmgr.package_info_get_type(h, ctypes.byref(typ))
    _pkgmgr.package_info_destroy(h)
    return json.dumps({"status":"success","package_id":pkg_id,"version":ver.value.decode() if ver.value else "","type":typ.value.decode() if typ.value else ""})

def _get_arg(key, default=""):
    for i in range(2, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-app-manager-cli <list|running|terminate|launch|package-info>", file=sys.stderr); sys.exit(1)
    cmd = sys.argv[1]
    if cmd in ("list", "list-all"): print(list_apps())
    elif cmd in ("running", "running-all"): print(list_running())
    elif cmd == "terminate": print(terminate(_get_arg("--app-id")))
    elif cmd == "launch": print(launch(_get_arg("--app-id"), _get_arg("--operation"), _get_arg("--uri"), _get_arg("--mime")))
    elif cmd == "package-info": print(package_info(_get_arg("--package-id")))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
