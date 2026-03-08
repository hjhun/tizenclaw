#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def control_led(action="on", brightness=None):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-device.so.0", "libcapi-system-device.so.1"]
        )

        # int device_flash_get_max_brightness(int *max_brightness)
        lib.device_flash_get_max_brightness.argtypes = [
            ctypes.POINTER(ctypes.c_int)
        ]
        lib.device_flash_get_max_brightness.restype = ctypes.c_int

        # int device_flash_set_brightness(int brightness)
        lib.device_flash_set_brightness.argtypes = [ctypes.c_int]
        lib.device_flash_set_brightness.restype = ctypes.c_int

        max_b = ctypes.c_int()
        tizen_capi_utils.check_return(
            lib.device_flash_get_max_brightness(ctypes.byref(max_b)),
            "Failed to get max flash brightness"
        )

        if action == "off":
            tizen_capi_utils.check_return(
                lib.device_flash_set_brightness(0),
                "Failed to turn off LED"
            )
            return {"status": "success", "action": "off", "message": "LED turned off"}

        # action == "on"
        if brightness is None:
            brightness = max_b.value
        brightness = max(0, min(brightness, max_b.value))

        tizen_capi_utils.check_return(
            lib.device_flash_set_brightness(brightness),
            "Failed to set LED brightness"
        )

        return {
            "status": "success",
            "action": "on",
            "brightness": brightness,
            "max_brightness": max_b.value,
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    act = args.get("action", "on")
    b = args.get("brightness")
    if b is not None:
        b = int(b)
    print(json.dumps(control_led(act, b)))
