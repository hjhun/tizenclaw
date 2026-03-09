#!/usr/bin/env python3
"""
TizenClaw Skill: send_app_control_without_appid
Launch or query applications using implicit intent (operation, URI, MIME type).
Use this when you do NOT know the specific app_id — the system finds the
best matching app automatically.
"""
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# Callback: bool (*app_control_app_matched_cb)(app_control_h, const char *appid, void *user_data)
APP_MATCHED_CB = ctypes.CFUNCTYPE(
    ctypes.c_bool, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p
)


def send_app_control_without_appid():
    try:
        args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
        operation = args.get("operation", "")
        uri = args.get("uri", "")
        mime = args.get("mime", "")
        action = args.get("action", "launch")
        extra_data = args.get("extra_data", {})

        if not operation:
            return {"error": "operation is required"}

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
        ac_lib.app_control_set_uri.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        ac_lib.app_control_set_uri.restype = ctypes.c_int
        ac_lib.app_control_set_mime.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        ac_lib.app_control_set_mime.restype = ctypes.c_int
        ac_lib.app_control_add_extra_data.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
        ac_lib.app_control_add_extra_data.restype = ctypes.c_int
        ac_lib.app_control_send_launch_request.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
        ac_lib.app_control_send_launch_request.restype = ctypes.c_int
        ac_lib.app_control_foreach_app_matched.argtypes = [ctypes.c_void_p, APP_MATCHED_CB, ctypes.c_void_p]
        ac_lib.app_control_foreach_app_matched.restype = ctypes.c_int

        # Create app_control handle
        handle = ctypes.c_void_p()
        ret = ac_lib.app_control_create(ctypes.byref(handle))
        if ret != 0:
            return {"error": f"app_control_create failed: {ret}"}

        # Set operation
        ac_lib.app_control_set_operation(handle, operation.encode("utf-8"))

        # Set URI if provided
        if uri:
            ac_lib.app_control_set_uri(handle, uri.encode("utf-8"))

        # Set MIME if provided
        if mime:
            ac_lib.app_control_set_mime(handle, mime.encode("utf-8"))

        # Add extra data if provided
        if extra_data and isinstance(extra_data, dict):
            for k, v in extra_data.items():
                ac_lib.app_control_add_extra_data(
                    handle, k.encode("utf-8"), str(v).encode("utf-8")
                )

        if action == "query":
            # Query matching apps
            matched_apps = []

            def on_app_matched(app_control, appid, user_data):
                if appid:
                    matched_apps.append(appid.decode("utf-8"))
                return True

            cb = APP_MATCHED_CB(on_app_matched)
            ret = ac_lib.app_control_foreach_app_matched(handle, cb, None)
            ac_lib.app_control_destroy(handle)

            if ret != 0:
                return {"error": f"app_control_foreach_app_matched failed: {ret}"}

            return {
                "matched_apps": matched_apps,
                "count": len(matched_apps),
                "operation": operation,
                "uri": uri or None,
                "mime": mime or None,
            }
        else:
            # Launch via implicit intent
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
                    "operation": operation,
                }

            return {
                "result": "launched",
                "operation": operation,
                "uri": uri or None,
                "mime": mime or None,
            }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(send_app_control_without_appid()))
