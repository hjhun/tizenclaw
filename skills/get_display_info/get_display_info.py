#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def get_display_info():
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-device.so.0", "libcapi-system-device.so.1"]
        )

        # int device_display_get_numbers(int *device_number)
        lib.device_display_get_numbers.argtypes = [ctypes.POINTER(ctypes.c_int)]
        lib.device_display_get_numbers.restype = ctypes.c_int

        # int device_display_get_brightness(int display_index, int *brightness)
        lib.device_display_get_brightness.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.device_display_get_brightness.restype = ctypes.c_int

        # int device_display_get_max_brightness(int display_index, int *max_brightness)
        lib.device_display_get_max_brightness.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.device_display_get_max_brightness.restype = ctypes.c_int

        # int device_display_get_state(display_state_e *state)
        lib.device_display_get_state.argtypes = [ctypes.POINTER(ctypes.c_int)]
        lib.device_display_get_state.restype = ctypes.c_int

        num_displays = ctypes.c_int()
        tizen_capi_utils.check_return(
            lib.device_display_get_numbers(ctypes.byref(num_displays)),
            "Failed to get display count"
        )

        state = ctypes.c_int()
        lib.device_display_get_state(ctypes.byref(state))
        state_map = {0: "normal", 1: "dim", 2: "off"}

        displays = []
        for i in range(num_displays.value):
            brightness = ctypes.c_int()
            max_brightness = ctypes.c_int()
            lib.device_display_get_brightness(i, ctypes.byref(brightness))
            lib.device_display_get_max_brightness(i, ctypes.byref(max_brightness))
            displays.append({
                "index": i,
                "brightness": brightness.value,
                "max_brightness": max_brightness.value,
            })

        return {
            "num_displays": num_displays.value,
            "state": state_map.get(state.value, "unknown"),
            "displays": displays,
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_display_info()))
