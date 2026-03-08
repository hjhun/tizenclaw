#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def _get_string(lib, key):
    """Get a string value from system_info."""
    val = ctypes.c_char_p()
    ret = lib.system_info_get_platform_string(
        key.encode("utf-8"), ctypes.byref(val)
    )
    if ret == 0 and val.value:
        result = val.value.decode("utf-8")
        return result
    return ""


def _get_int(lib, key):
    """Get an int value from system_info."""
    val = ctypes.c_int()
    ret = lib.system_info_get_platform_int(
        key.encode("utf-8"), ctypes.byref(val)
    )
    if ret == 0:
        return val.value
    return None


def _get_bool(lib, key):
    """Get a bool value from system_info."""
    val = ctypes.c_bool()
    ret = lib.system_info_get_platform_bool(
        key.encode("utf-8"), ctypes.byref(val)
    )
    if ret == 0:
        return val.value
    return None


def get_system_info():
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-info.so.0", "libcapi-system-info.so.1"]
        )

        # Define function signatures
        lib.system_info_get_platform_string.argtypes = [
            ctypes.c_char_p, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.system_info_get_platform_string.restype = ctypes.c_int

        lib.system_info_get_platform_int.argtypes = [
            ctypes.c_char_p, ctypes.POINTER(ctypes.c_int)
        ]
        lib.system_info_get_platform_int.restype = ctypes.c_int

        lib.system_info_get_platform_bool.argtypes = [
            ctypes.c_char_p, ctypes.POINTER(ctypes.c_bool)
        ]
        lib.system_info_get_platform_bool.restype = ctypes.c_int

        info = {
            "model_name": _get_string(lib, "http://tizen.org/system/model_name"),
            "platform_name": _get_string(lib, "http://tizen.org/system/platform.name"),
            "platform_version": _get_string(lib, "http://tizen.org/feature/platform.version"),
            "build_string": _get_string(lib, "http://tizen.org/system/build.string"),
            "build_type": _get_string(lib, "http://tizen.org/system/build.type"),
            "manufacturer": _get_string(lib, "http://tizen.org/system/manufacturer"),
            "cpu_arch": _get_string(lib, "http://tizen.org/feature/platform.core.cpu.arch"),
            "screen_width": _get_int(lib, "http://tizen.org/feature/screen.width"),
            "screen_height": _get_int(lib, "http://tizen.org/feature/screen.height"),
            "screen_dpi": _get_int(lib, "http://tizen.org/feature/screen.dpi"),
            "features": {
                "bluetooth": _get_bool(lib, "http://tizen.org/feature/network.bluetooth"),
                "wifi": _get_bool(lib, "http://tizen.org/feature/network.wifi"),
                "gps": _get_bool(lib, "http://tizen.org/feature/location.gps"),
                "camera": _get_bool(lib, "http://tizen.org/feature/camera"),
                "nfc": _get_bool(lib, "http://tizen.org/feature/network.nfc"),
                "accelerometer": _get_bool(lib, "http://tizen.org/feature/sensor.accelerometer"),
                "barometer": _get_bool(lib, "http://tizen.org/feature/sensor.barometer"),
                "gyroscope": _get_bool(lib, "http://tizen.org/feature/sensor.gyroscope"),
            },
        }

        return info

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_system_info()))
