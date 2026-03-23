#!/usr/bin/env python3
"""tizen-control-display-cli — Python port"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_device = _load("libcapi-system-device.so.0")

def get_info():
    r = {"status":"success"}
    if _device:
        b = ctypes.c_int()
        if _device.device_display_get_brightness(0, ctypes.byref(b)) == 0: r["brightness"] = b.value
        mb = ctypes.c_int()
        if _device.device_display_get_max_brightness(0, ctypes.byref(mb)) == 0: r["max_brightness"] = mb.value
        st = ctypes.c_int()
        if _device.device_display_get_state(ctypes.byref(st)) == 0:
            r["state"] = {0:"normal",1:"dim",2:"off"}.get(st.value, "unknown")
    return json.dumps(r)

def set_brightness(val):
    if not _device: return json.dumps({"error":"device lib not available"})
    ret = _device.device_display_set_brightness(0, val)
    return json.dumps({"status":"success" if ret == 0 else "error","code":ret})

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-control-display-cli --info | --brightness <N>", file=sys.stderr); sys.exit(1)
    if sys.argv[1] == "--info": print(get_info())
    elif sys.argv[1] == "--brightness" and len(sys.argv) >= 3: print(set_brightness(int(sys.argv[2])))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
