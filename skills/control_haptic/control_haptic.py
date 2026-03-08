#!/usr/bin/env python3
import ctypes
import json
import os
import sys
import time

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def control_haptic(duration_ms=500):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-device.so.0", "libcapi-system-device.so.1"]
        )

        # int device_haptic_open(int device_index, haptic_device_h *device_handle)
        lib.device_haptic_open.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_void_p)
        ]
        lib.device_haptic_open.restype = ctypes.c_int

        # int device_haptic_vibrate(haptic_device_h device_handle,
        #   int duration, int feedback, haptic_effect_h *effect_handle)
        lib.device_haptic_vibrate.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
            ctypes.POINTER(ctypes.c_void_p)
        ]
        lib.device_haptic_vibrate.restype = ctypes.c_int

        # int device_haptic_stop(haptic_device_h device_handle,
        #   haptic_effect_h effect_handle)
        lib.device_haptic_stop.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        lib.device_haptic_stop.restype = ctypes.c_int

        # int device_haptic_close(haptic_device_h device_handle)
        lib.device_haptic_close.argtypes = [ctypes.c_void_p]
        lib.device_haptic_close.restype = ctypes.c_int

        handle = ctypes.c_void_p()
        tizen_capi_utils.check_return(
            lib.device_haptic_open(0, ctypes.byref(handle)),
            "Failed to open haptic device"
        )

        effect = ctypes.c_void_p()
        ret = lib.device_haptic_vibrate(handle, duration_ms, 100, ctypes.byref(effect))

        if ret != 0:
            lib.device_haptic_close(handle)
            return {"error": f"Failed to vibrate (code: {ret})"}

        # Wait for vibration to finish
        time.sleep(duration_ms / 1000.0)

        lib.device_haptic_stop(handle, effect)
        lib.device_haptic_close(handle)

        return {
            "status": "success",
            "duration_ms": duration_ms,
            "message": f"Device vibrated for {duration_ms}ms",
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    ms = args.get("duration_ms", 500)
    print(json.dumps(control_haptic(int(ms))))
