#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def control_display(brightness):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-device.so.0", "libcapi-system-device.so.1"]
        )

        lib.device_display_set_brightness.argtypes = [ctypes.c_int, ctypes.c_int]
        lib.device_display_set_brightness.restype = ctypes.c_int

        lib.device_display_get_max_brightness.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.device_display_get_max_brightness.restype = ctypes.c_int

        max_b = ctypes.c_int()
        tizen_capi_utils.check_return(
            lib.device_display_get_max_brightness(0, ctypes.byref(max_b)),
            "Failed to get max brightness"
        )

        clamped = max(0, min(brightness, max_b.value))
        tizen_capi_utils.check_return(
            lib.device_display_set_brightness(0, clamped),
            "Failed to set brightness"
        )

        return {
            "status": "success",
            "brightness_set": clamped,
            "max_brightness": max_b.value,
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    b = args.get("brightness", 50)
    print(json.dumps(control_display(int(b))))
