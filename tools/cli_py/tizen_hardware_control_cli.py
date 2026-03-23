#!/usr/bin/env python3
"""tizen-hardware-control-cli — Python port"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_device = _load("libcapi-system-device.so.0")
_feedback = _load("libcapi-media-feedback.so.0")

def haptic_vibrate(duration_ms):
    if not _device: return json.dumps({"error":"device lib not available"})
    h = ctypes.c_void_p(); e = ctypes.c_void_p()
    r = _device.device_haptic_open(0, ctypes.byref(h))
    if r != 0: return json.dumps({"error":"haptic_open failed","code":r})
    r = _device.device_haptic_vibrate(h, duration_ms, 100, ctypes.byref(e))
    _device.device_haptic_close(h)
    return json.dumps({"status":"success" if r == 0 else "error","duration_ms":duration_ms,"code":r})

def led_control(action, brightness):
    if not _device: return json.dumps({"error":"device lib not available"})
    if action == "on":
        b = brightness if brightness >= 0 else 100
        r = _device.device_led_play_custom(500, 500, 0x00FF00, 0)  # on_ms, off_ms, color, flags
        return json.dumps({"status":"success" if r == 0 else "error","action":"on","code":r})
    else:
        r = _device.device_led_stop_custom()
        return json.dumps({"status":"success" if r == 0 else "error","action":"off","code":r})

def power_control(action, resource):
    if not _device: return json.dumps({"error":"device lib not available"})
    res_map = {"display":0, "cpu":1}
    rid = res_map.get(resource, 0)
    if action == "lock":
        r = _device.device_power_request_lock(rid, 0)
    else:
        r = _device.device_power_release_lock(rid)
    return json.dumps({"status":"success" if r == 0 else "error","action":action,"resource":resource,"code":r})

def feedback_play(pattern):
    if not _feedback: return json.dumps({"error":"feedback lib not available"})
    _feedback.feedback_initialize()
    r = _feedback.feedback_play_type(1, int(pattern) if pattern.isdigit() else 0)  # FEEDBACK_TYPE_VIBRATION=1
    _feedback.feedback_deinitialize()
    return json.dumps({"status":"success" if r == 0 else "error","pattern":pattern,"code":r})

def _get_arg(key, default=""):
    for i in range(2, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-hardware-control-cli <haptic|led|power|feedback>", file=sys.stderr); sys.exit(1)
    cmd = sys.argv[1]
    if cmd == "haptic": print(haptic_vibrate(int(_get_arg("--duration", "500"))))
    elif cmd == "led": print(led_control(_get_arg("--action", "on"), int(_get_arg("--brightness", "-1"))))
    elif cmd == "power": print(power_control(_get_arg("--action", "lock"), _get_arg("--resource", "display")))
    elif cmd == "feedback": print(feedback_play(_get_arg("--pattern", "TAP")))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
