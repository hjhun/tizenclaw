#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# feedback_pattern_e values
FEEDBACK_PATTERNS = {
    "TAP": 0, "SIP": 1,
    "KEY0": 6, "KEY1": 7, "KEY2": 8, "KEY3": 9, "KEY4": 10,
    "KEY5": 11, "KEY6": 12, "KEY7": 13, "KEY8": 14, "KEY9": 15,
    "HOLD": 16, "HW_TAP": 17, "HW_HOLD": 18,
    "MESSAGE": 19, "EMAIL": 20, "WAKEUP": 21,
    "SCHEDULE": 22, "TIMER": 23, "GENERAL": 24,
    "POWERON": 25, "POWEROFF": 26,
    "CHARGERCONN": 27, "CHARGING_ERROR": 28,
    "FULLCHARGED": 29, "LOWBATT": 30,
    "LOCK": 31, "UNLOCK": 32,
    "VIBRATION_ON": 33, "SILENT_OFF": 34,
    "BT_CONNECTED": 35, "BT_DISCONNECTED": 36,
}

# feedback_type_e
FEEDBACK_TYPE_NONE = 0
FEEDBACK_TYPE_SOUND = 1
FEEDBACK_TYPE_VIBRATION = 2


def play_feedback(pattern_name="TAP"):
    try:
        lib = tizen_capi_utils.load_library(
            ["libfeedback.so.0", "libfeedback.so.1"]
        )

        # int feedback_initialize(void)
        lib.feedback_initialize.argtypes = []
        lib.feedback_initialize.restype = ctypes.c_int

        # int feedback_play(feedback_pattern_e pattern)
        lib.feedback_play.argtypes = [ctypes.c_int]
        lib.feedback_play.restype = ctypes.c_int

        # int feedback_deinitialize(void)
        lib.feedback_deinitialize.argtypes = []
        lib.feedback_deinitialize.restype = ctypes.c_int

        pattern_val = FEEDBACK_PATTERNS.get(pattern_name.upper())
        if pattern_val is None:
            return {"error": f"Unknown pattern: {pattern_name}"}

        tizen_capi_utils.check_return(
            lib.feedback_initialize(),
            "Failed to initialize feedback"
        )

        ret = lib.feedback_play(pattern_val)
        lib.feedback_deinitialize()

        if ret != 0:
            return {"error": f"Failed to play feedback (code: {ret})"}

        return {
            "status": "success",
            "pattern": pattern_name.upper(),
            "message": f"Played feedback pattern: {pattern_name.upper()}",
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    pat = args.get("pattern", "TAP")
    print(json.dumps(play_feedback(pat)))
