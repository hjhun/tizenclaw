#!/usr/bin/env python3
"""
TizenClaw Skill: Terminate App
Uses Tizen CAPI app_manager to terminate a running application.
"""
import ctypes
import json
import os
import sys

# Add common directory to path
sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def terminate_app(app_id):
    try:
        lib = tizen_capi_utils.load_library([
            "libcapi-appfw-app-manager.so.0",
            "libcapi-appfw-app-manager.so.1",
        ])

        # bool app_manager_is_running(const char *app_id)
        lib.app_manager_is_running.argtypes = [ctypes.c_char_p]
        lib.app_manager_is_running.restype = ctypes.c_bool

        # int app_manager_get_app_context(
        #     const char *app_id, app_context_h *handle)
        lib.app_manager_get_app_context.argtypes = [
            ctypes.c_char_p,
            ctypes.POINTER(ctypes.c_void_p),
        ]
        lib.app_manager_get_app_context.restype = ctypes.c_int

        # int app_manager_terminate_app(app_context_h handle)
        lib.app_manager_terminate_app.argtypes = [
            ctypes.c_void_p,
        ]
        lib.app_manager_terminate_app.restype = ctypes.c_int

        # int app_context_destroy(app_context_h handle)
        lib.app_context_destroy.argtypes = [ctypes.c_void_p]
        lib.app_context_destroy.restype = ctypes.c_int

        b_app_id = app_id.encode("utf-8")

        # Check if app is running
        is_running = lib.app_manager_is_running(b_app_id)
        if not is_running:
            return {
                "status": "not_running",
                "app_id": app_id,
                "message": f"App {app_id} is not currently running",
            }

        # Get app context
        ctx = ctypes.c_void_p()
        ret = lib.app_manager_get_app_context(
            b_app_id, ctypes.byref(ctx)
        )
        tizen_capi_utils.check_return(
            ret, "app_manager_get_app_context failed"
        )

        # Terminate the app
        ret = lib.app_manager_terminate_app(ctx)
        lib.app_context_destroy(ctx)
        tizen_capi_utils.check_return(
            ret, "app_manager_terminate_app failed"
        )

        return {
            "status": "success",
            "app_id": app_id,
            "message": f"App {app_id} has been terminated",
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    claw_args = os.environ.get("CLAW_ARGS")
    if claw_args:
        try:
            parsed = json.loads(claw_args)
            app_id = parsed.get("app_id", "")
            if app_id:
                result = terminate_app(app_id)
                print(json.dumps(result))
                sys.exit(0)
        except Exception as e:
            print(json.dumps({"error": f"Failed to parse CLAW_ARGS: {e}"}))
            sys.exit(1)

    if len(sys.argv) < 2:
        print(json.dumps({"error": f"Usage: {sys.argv[0]} <app_id>"}))
        sys.exit(1)

    target_app_id = sys.argv[1]
    print(json.dumps(terminate_app(target_app_id)))
