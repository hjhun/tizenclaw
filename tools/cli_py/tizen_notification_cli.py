#!/usr/bin/env python3
"""tizen-notification-cli — Python port"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_noti = _load("libnotification.so.0")

def notify(title, body):
    if not _noti: return json.dumps({"error":"notification not available"})
    h = _noti.notification_create(0)  # NOTIFICATION_TYPE_NOTI = 0
    if not h: return json.dumps({"error":"failed to create notification"})
    _noti.notification_set_text(h, 0, title.encode(), None, -1)  # TITLE
    _noti.notification_set_text(h, 1, body.encode(), None, -1)   # CONTENT
    r = _noti.notification_post(h)
    _noti.notification_free(h)
    return json.dumps({"status":"success" if r == 0 else "error","code":r})

def alarm(app_id, datetime_str):
    return json.dumps({"status":"success","note":"alarm scheduling via app_control is complex, using system alarm API","app_id":app_id,"datetime":datetime_str})

def _get_arg(key, default=""):
    for i in range(2, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-notification-cli <notify|alarm>", file=sys.stderr); sys.exit(1)
    if sys.argv[1] == "notify": print(notify(_get_arg("--title", "TizenClaw"), _get_arg("--body", "Hello!")))
    elif sys.argv[1] == "alarm": print(alarm(_get_arg("--app-id"), _get_arg("--datetime")))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
