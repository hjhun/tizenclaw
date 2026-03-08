#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# sound_type_e
SOUND_TYPES = {
    "system": 0,
    "notification": 1,
    "alarm": 2,
    "ringtone": 3,
    "media": 4,
    "call": 5,
    "voip": 6,
}


def control_volume(action="get", sound_type=None, volume=None):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-media-sound-manager.so.0",
             "libcapi-media-sound-manager.so.1"]
        )

        # int sound_manager_get_volume(sound_type_e type, int *volume)
        lib.sound_manager_get_volume.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.sound_manager_get_volume.restype = ctypes.c_int

        # int sound_manager_set_volume(sound_type_e type, int volume)
        lib.sound_manager_set_volume.argtypes = [ctypes.c_int, ctypes.c_int]
        lib.sound_manager_set_volume.restype = ctypes.c_int

        # int sound_manager_get_max_volume(sound_type_e type, int *max)
        lib.sound_manager_get_max_volume.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_int)
        ]
        lib.sound_manager_get_max_volume.restype = ctypes.c_int

        if action == "get":
            volumes = {}
            for name, type_val in SOUND_TYPES.items():
                vol = ctypes.c_int()
                max_vol = ctypes.c_int()
                if lib.sound_manager_get_volume(type_val, ctypes.byref(vol)) == 0:
                    lib.sound_manager_get_max_volume(type_val, ctypes.byref(max_vol))
                    volumes[name] = {
                        "current": vol.value,
                        "max": max_vol.value,
                    }
            return {"action": "get", "volumes": volumes}

        elif action == "set":
            if sound_type is None or volume is None:
                return {"error": "sound_type and volume are required for 'set'"}

            type_val = SOUND_TYPES.get(sound_type)
            if type_val is None:
                return {"error": f"Unknown sound type: {sound_type}"}

            max_vol = ctypes.c_int()
            lib.sound_manager_get_max_volume(type_val, ctypes.byref(max_vol))
            clamped = max(0, min(volume, max_vol.value))

            tizen_capi_utils.check_return(
                lib.sound_manager_set_volume(type_val, clamped),
                f"Failed to set {sound_type} volume"
            )

            return {
                "status": "success",
                "action": "set",
                "sound_type": sound_type,
                "volume_set": clamped,
                "max_volume": max_vol.value,
            }

        return {"error": f"Unknown action: {action}"}

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    act = args.get("action", "get")
    st = args.get("sound_type")
    vol = args.get("volume")
    if vol is not None:
        vol = int(vol)
    print(json.dumps(control_volume(act, st, vol)))
