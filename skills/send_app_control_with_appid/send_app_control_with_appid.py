#!/usr/bin/env python3
"""
TizenClaw Skill: send_app_control_with_appid
Launch a specific Tizen application using its explicit app_id.
Use this when the exact app_id is known.
"""
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

OPERATION_DEFAULT = "http://tizen.org/appcontrol/operation/default"


def send_app_control_with_appid():
    try:
        args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
        app_id = args.get("app_id", "")
        extra_data = args.get("extra_data", {})

        if not app_id:
            return {"error": "app_id is required"}

        ac_lib = tizen_capi_utils.load_library(
            ["libcapi-appfw-app-control.so.0", "libcapi-appfw-app-control.so"]
        )

        # Function signatures
        ac_lib.app_control_create.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
        ac_lib.app_control_create.restype = ctypes.c_int
        ac_lib.app_control_destroy.argtypes = [ctypes.c_void_p]
        ac_lib.app_control_destroy.restype = ctypes.c_int
        ac_lib.app_control_set_operation.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        ac_lib.app_control_set_operation.restype = ctypes.c_int
        ac_lib.app_control_set_app_id.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        ac_lib.app_control_set_app_id.restype = ctypes.c_int
        ac_lib.app_control_add_extra_data.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        ac_lib.app_control_add_extra_data.restype = ctypes.c_int
        ac_lib.app_control_send_launch_request.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
        ac_lib.app_control_send_launch_request.restype = ctypes.c_int

        # Create app_control handle
        handle = ctypes.c_void_p()
        ret = ac_lib.app_control_create(ctypes.byref(handle))
        if ret != 0:
            return {"error": f"app_control_create failed: {ret}"}

        # Set default operation
        ac_lib.app_control_set_operation(handle, OPERATION_DEFAULT.encode("utf-8"))

        # Set explicit app_id
        ret = ac_lib.app_control_set_app_id(handle, app_id.encode("utf-8"))
        if ret != 0:
            ac_lib.app_control_destroy(handle)
            return {"error": f"app_control_set_app_id failed: {ret}"}

        # Add extra data if provided
        if extra_data and isinstance(extra_data, dict):
            for k, v in extra_data.items():
                ac_lib.app_control_add_extra_data(
                    handle, k.encode("utf-8"), str(v).encode("utf-8")
                )

        # Send launch request
        ret = ac_lib.app_control_send_launch_request(handle, None, None)
        ac_lib.app_control_destroy(handle)

        if ret != 0:
            error_map = {
                -22: "INVALID_PARAMETER",
                -38: "APP_NOT_FOUND",
                -12: "OUT_OF_MEMORY",
                -13: "PERMISSION_DENIED",
            }
            return {
                "error": f"Launch failed: {error_map.get(ret, ret)}",
                "code": ret,
                "app_id": app_id,
            }

        return {"result": "launched", "app_id": app_id}

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(send_app_control_with_appid()))
