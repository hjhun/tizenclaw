#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# system_settings_key_e values (from system_settings_keys.h)
SYSTEM_SETTINGS_KEY_INCOMING_CALL_RINGTONE = 0
SYSTEM_SETTINGS_KEY_WALLPAPER_HOME_SCREEN = 1
SYSTEM_SETTINGS_KEY_WALLPAPER_LOCK_SCREEN = 2
SYSTEM_SETTINGS_KEY_FONT_SIZE = 3
SYSTEM_SETTINGS_KEY_FONT_TYPE = 4
SYSTEM_SETTINGS_KEY_LOCALE_COUNTRY = 7
SYSTEM_SETTINGS_KEY_LOCALE_LANGUAGE = 8
SYSTEM_SETTINGS_KEY_LOCALE_TIMEFORMAT_24HOUR = 9
SYSTEM_SETTINGS_KEY_LOCALE_TIMEZONE = 10
SYSTEM_SETTINGS_KEY_DEVICE_NAME = 16
SYSTEM_SETTINGS_KEY_SOUND_ENABLED = 23
SYSTEM_SETTINGS_KEY_VIBRATION_ENABLED = 28


def get_system_settings():
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-system-settings.so.0",
             "libcapi-system-system-settings.so.1"]
        )

        # int system_settings_get_value_string(int key, char **value)
        lib.system_settings_get_value_string.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.system_settings_get_value_string.restype = ctypes.c_int

        # int system_settings_get_value_bool(int key, bool *value)
        lib.system_settings_get_value_bool.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_bool)
        ]
        lib.system_settings_get_value_bool.restype = ctypes.c_int

        # int system_settings_get_value_int(int key, int *value)
        lib.system_settings_get_value_int.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.system_settings_get_value_int.restype = ctypes.c_int

        def get_str(key):
            val = ctypes.c_char_p()
            if lib.system_settings_get_value_string(key, ctypes.byref(val)) == 0:
                return val.value.decode("utf-8") if val.value else ""
            return ""

        def get_bool(key):
            val = ctypes.c_bool()
            if lib.system_settings_get_value_bool(key, ctypes.byref(val)) == 0:
                return val.value
            return None

        def get_int(key):
            val = ctypes.c_int()
            if lib.system_settings_get_value_int(key, ctypes.byref(val)) == 0:
                return val.value
            return None

        font_size_map = {
            0: "small", 1: "normal", 2: "large", 3: "huge", 4: "giant"
        }
        fs = get_int(SYSTEM_SETTINGS_KEY_FONT_SIZE)

        return {
            "locale_country": get_str(SYSTEM_SETTINGS_KEY_LOCALE_COUNTRY),
            "locale_language": get_str(SYSTEM_SETTINGS_KEY_LOCALE_LANGUAGE),
            "timezone": get_str(SYSTEM_SETTINGS_KEY_LOCALE_TIMEZONE),
            "time_format_24h": get_bool(SYSTEM_SETTINGS_KEY_LOCALE_TIMEFORMAT_24HOUR),
            "device_name": get_str(SYSTEM_SETTINGS_KEY_DEVICE_NAME),
            "ringtone_path": get_str(SYSTEM_SETTINGS_KEY_INCOMING_CALL_RINGTONE),
            "wallpaper_home": get_str(SYSTEM_SETTINGS_KEY_WALLPAPER_HOME_SCREEN),
            "wallpaper_lock": get_str(SYSTEM_SETTINGS_KEY_WALLPAPER_LOCK_SCREEN),
            "font_type": get_str(SYSTEM_SETTINGS_KEY_FONT_TYPE),
            "font_size": font_size_map.get(fs, "unknown") if fs is not None else "unknown",
            "sound_enabled": get_bool(SYSTEM_SETTINGS_KEY_SOUND_ENABLED),
            "vibration_enabled": get_bool(SYSTEM_SETTINGS_KEY_VIBRATION_ENABLED),
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_system_settings()))
