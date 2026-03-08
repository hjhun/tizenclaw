#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# notification_type_e
NOTIFICATION_TYPE_NOTI = 0
NOTIFICATION_TYPE_ONGOING = 2

# notification_text_type_e
NOTIFICATION_TEXT_TYPE_TITLE = 0
NOTIFICATION_TEXT_TYPE_CONTENT = 1


def send_notification(title, body):
    try:
        lib = tizen_capi_utils.load_library(
            ["libnotification.so.0", "libnotification.so.1"]
        )

        # notification_h notification_create(notification_type_e type)
        lib.notification_create.argtypes = [ctypes.c_int]
        lib.notification_create.restype = ctypes.c_void_p

        # int notification_set_text(notification_h noti, notification_text_type_e type,
        #   const char *text, const char *key, int group_id, int priv_id)
        lib.notification_set_text.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.c_char_p,
            ctypes.c_char_p, ctypes.c_int, ctypes.c_int
        ]
        lib.notification_set_text.restype = ctypes.c_int

        # int notification_post(notification_h noti)
        lib.notification_post.argtypes = [ctypes.c_void_p]
        lib.notification_post.restype = ctypes.c_int

        # int notification_free(notification_h noti)
        lib.notification_free.argtypes = [ctypes.c_void_p]
        lib.notification_free.restype = ctypes.c_int

        noti = lib.notification_create(NOTIFICATION_TYPE_NOTI)
        if not noti:
            return {"error": "Failed to create notification"}

        # Set title
        ret = lib.notification_set_text(
            noti, NOTIFICATION_TEXT_TYPE_TITLE,
            title.encode("utf-8"), None, -1, -1
        )
        if ret != 0:
            lib.notification_free(noti)
            return {"error": f"Failed to set title (code: {ret})"}

        # Set content
        ret = lib.notification_set_text(
            noti, NOTIFICATION_TEXT_TYPE_CONTENT,
            body.encode("utf-8"), None, -1, -1
        )
        if ret != 0:
            lib.notification_free(noti)
            return {"error": f"Failed to set content (code: {ret})"}

        ret = lib.notification_post(noti)
        lib.notification_free(noti)

        if ret != 0:
            return {"error": f"Failed to post notification (code: {ret})"}

        return {
            "status": "success",
            "title": title,
            "body": body,
            "message": "Notification posted successfully",
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    t = args.get("title", "TizenClaw")
    b = args.get("body", "Hello from TizenClaw!")
    print(json.dumps(send_notification(t, b)))
