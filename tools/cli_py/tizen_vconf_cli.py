#!/usr/bin/env python3
"""tizen-vconf-cli — Python port"""
import ctypes, json, sys, signal

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_vconf = _load("libvconf.so.0")
_glib = _load("libglib-2.0.so.0")

# vconf type constants
VCONF_OK = 0
VCONF_TYPE_INT = 0x04
VCONF_TYPE_BOOL = 0x08
VCONF_TYPE_DOUBLE = 0x10
VCONF_TYPE_STRING = 0x20

def vconf_get(key):
    if not _vconf: return json.dumps({"error":"vconf not available"})
    kb = key.encode()
    # Try int
    iv = ctypes.c_int()
    if _vconf.vconf_get_int(kb, ctypes.byref(iv)) == VCONF_OK:
        return json.dumps({"key":key,"type":"int","value":iv.value})
    # Try bool
    bv = ctypes.c_int()
    if _vconf.vconf_get_bool(kb, ctypes.byref(bv)) == VCONF_OK:
        return json.dumps({"key":key,"type":"bool","value":bool(bv.value)})
    # Try double
    dv = ctypes.c_double()
    if _vconf.vconf_get_dbl(kb, ctypes.byref(dv)) == VCONF_OK:
        return json.dumps({"key":key,"type":"double","value":dv.value})
    # Try string
    _vconf.vconf_get_str.restype = ctypes.c_char_p
    sv = _vconf.vconf_get_str(kb)
    if sv: return json.dumps({"key":key,"type":"string","value":sv.decode()})
    return json.dumps({"error":"key not found","key":key})

def vconf_set(key, value):
    if not _vconf: return json.dumps({"error":"vconf not available"})
    kb = key.encode()
    # Detect type from existing key
    iv = ctypes.c_int()
    if _vconf.vconf_get_int(kb, ctypes.byref(iv)) == VCONF_OK:
        r = _vconf.vconf_set_int(kb, int(value))
        return json.dumps({"status":"ok" if r == VCONF_OK else "error","code":r})
    bv = ctypes.c_int()
    if _vconf.vconf_get_bool(kb, ctypes.byref(bv)) == VCONF_OK:
        b = 1 if value in ("true","1","True") else 0
        r = _vconf.vconf_set_bool(kb, b)
        return json.dumps({"status":"ok" if r == VCONF_OK else "error","code":r})
    dv = ctypes.c_double()
    if _vconf.vconf_get_dbl(kb, ctypes.byref(dv)) == VCONF_OK:
        r = _vconf.vconf_set_dbl(kb, ctypes.c_double(float(value)))
        return json.dumps({"status":"ok" if r == VCONF_OK else "error","code":r})
    # Default: string
    r = _vconf.vconf_set_str(kb, value.encode())
    return json.dumps({"status":"ok" if r == VCONF_OK else "error","code":r})

# callback for watch
KEYCHANGED_CB = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_void_p)
_g_loop = None

def _on_key_changed(node, user_data):
    if not _vconf: return
    name = _vconf.vconf_keynode_get_name(node)
    ktype = _vconf.vconf_keynode_get_type(node)
    r = {"event":"changed","key":name.decode() if name else ""}
    if ktype == VCONF_TYPE_INT:
        r["type"] = "int"; r["value"] = _vconf.vconf_keynode_get_int(node)
    elif ktype == VCONF_TYPE_BOOL:
        r["type"] = "bool"; r["value"] = bool(_vconf.vconf_keynode_get_bool(node))
    elif ktype == VCONF_TYPE_DOUBLE:
        _vconf.vconf_keynode_get_dbl.restype = ctypes.c_double
        r["type"] = "double"; r["value"] = _vconf.vconf_keynode_get_dbl(node)
    elif ktype == VCONF_TYPE_STRING:
        _vconf.vconf_keynode_get_str.restype = ctypes.c_char_p
        v = _vconf.vconf_keynode_get_str(node)
        r["type"] = "string"; r["value"] = v.decode() if v else ""
    print(json.dumps(r), flush=True)

def vconf_watch(key):
    if not _vconf or not _glib: return json.dumps({"error":"vconf/glib not available"})
    global _g_loop
    kb = key.encode()
    cb = KEYCHANGED_CB(_on_key_changed)

    def sig_handler(sig, frame):
        global _g_loop
        if _g_loop: _glib.g_main_loop_quit(_g_loop)

    signal.signal(signal.SIGINT, sig_handler)
    signal.signal(signal.SIGTERM, sig_handler)

    _vconf.vconf_notify_key_changed(kb, cb, None)
    # Print initial
    print(vconf_get(key), flush=True)
    _g_loop = _glib.g_main_loop_new(None, False)
    _glib.g_main_loop_run(_g_loop)
    _glib.g_main_loop_unref(_g_loop)
    _vconf.vconf_ignore_key_changed(kb, cb)
    return ""

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: tizen-vconf-cli <get|set|watch> <key> [value]", file=sys.stderr); sys.exit(1)
    cmd, key = sys.argv[1], sys.argv[2]
    if cmd == "get": print(vconf_get(key))
    elif cmd == "set":
        if len(sys.argv) < 4: print("Value required", file=sys.stderr); sys.exit(1)
        print(vconf_set(key, sys.argv[3]))
    elif cmd == "watch": vconf_watch(key)
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
