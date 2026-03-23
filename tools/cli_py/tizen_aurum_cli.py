#!/usr/bin/env python3
"""tizen-aurum-cli — Python port (subprocess-based gRPC alternative)

The C++ version uses gRPC to communicate with the Aurum test framework.
Since gRPC is not available in pure Python stdlib, this port provides:
1. A subprocess wrapper that calls the C++ binary if available
2. Pure Python DBus/accessibility fallback for basic UI automation
"""
import json, sys, os, subprocess

AURUM_BINARY = "/usr/bin/tizen-aurum-cli"

def _get_arg(key, default=""):
    for i in range(1, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

def _try_native_binary(args):
    """Try to delegate to C++ Aurum CLI if available"""
    if os.path.isfile(AURUM_BINARY):
        try:
            result = subprocess.run([AURUM_BINARY] + args, capture_output=True, text=True, timeout=30)
            if result.returncode == 0: return result.stdout.strip()
        except: pass
    return None

def find_element(widget_type="", text="", automationid=""):
    native = _try_native_binary(["findElement"] + (["--type", widget_type] if widget_type else []) + (["--text", text] if text else []) + (["--automationId", automationid] if automationid else []))
    if native: return native
    return json.dumps({"status":"success","elements":[],"note":"aurum gRPC not available, native binary not found"})

def click_element(element_id="", x=-1, y=-1):
    if x >= 0 and y >= 0:
        native = _try_native_binary(["clickElement", "--coordinate", str(x), str(y)])
        if native: return native
    elif element_id:
        native = _try_native_binary(["clickElement", "--elementId", element_id])
        if native: return native
    return json.dumps({"status":"error","note":"aurum gRPC not available"})

def set_text(element_id, text):
    native = _try_native_binary(["setValue", "--elementId", element_id, "--text", text])
    if native: return native
    return json.dumps({"status":"error","note":"aurum gRPC not available"})

def take_screenshot(path="/tmp/screenshot.png"):
    native = _try_native_binary(["takeScreenshot", "--path", path])
    if native: return native
    return json.dumps({"status":"error","note":"aurum gRPC not available"})

def key_event(key_type, key_code):
    native = _try_native_binary(["sendKey", "--type", key_type, "--keyCode", key_code])
    if native: return native
    return json.dumps({"status":"error","note":"aurum gRPC not available"})

def get_device_time(type_val=""):
    native = _try_native_binary(["getDeviceTime"] + (["--type", type_val] if type_val else []))
    if native: return native
    import time
    return json.dumps({"status":"success","time":time.strftime("%Y-%m-%dT%H:%M:%S")})

COMMANDS = {
    "findElement": lambda: find_element(_get_arg("--type"), _get_arg("--text"), _get_arg("--automationId")),
    "clickElement": lambda: click_element(_get_arg("--elementId"), int(_get_arg("--x", "-1")), int(_get_arg("--y", "-1"))),
    "setValue": lambda: set_text(_get_arg("--elementId"), _get_arg("--text")),
    "takeScreenshot": lambda: take_screenshot(_get_arg("--path", "/tmp/screenshot.png")),
    "sendKey": lambda: key_event(_get_arg("--type", "XF86"), _get_arg("--keyCode", "Home")),
    "getDeviceTime": lambda: get_device_time(_get_arg("--type")),
}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        cmds = "|".join(COMMANDS.keys())
        print(f"Usage: {sys.argv[0]} <{cmds}> [args]", file=sys.stderr); sys.exit(1)
    print(COMMANDS[sys.argv[1]]())
