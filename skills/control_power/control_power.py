#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# power_lock_e
POWER_LOCK_CPU = 0
POWER_LOCK_DISPLAY = 1
POWER_LOCK_DISPLAY_DIM = 2


def control_power(action, resource):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-device.so.0", "libcapi-system-device.so.1"]
        )

        # int device_power_request_lock(power_lock_e type, int timeout_ms)
        lib.device_power_request_lock.argtypes = [ctypes.c_int, ctypes.c_int]
        lib.device_power_request_lock.restype = ctypes.c_int

        # int device_power_release_lock(power_lock_e type)
        lib.device_power_release_lock.argtypes = [ctypes.c_int]
        lib.device_power_release_lock.restype = ctypes.c_int

        lock_type = POWER_LOCK_CPU if resource == "cpu" else POWER_LOCK_DISPLAY

        if action == "lock":
            # Lock for 0 = indefinite until release
            tizen_capi_utils.check_return(
                lib.device_power_request_lock(lock_type, 0),
                f"Failed to request {resource} lock"
            )
            return {
                "status": "success",
                "action": "lock",
                "resource": resource,
                "message": f"{resource} lock acquired",
            }
        else:
            tizen_capi_utils.check_return(
                lib.device_power_release_lock(lock_type),
                f"Failed to release {resource} lock"
            )
            return {
                "status": "success",
                "action": "unlock",
                "resource": resource,
                "message": f"{resource} lock released",
            }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    act = args.get("action", "lock")
    res = args.get("resource", "display")
    print(json.dumps(control_power(act, res)))
