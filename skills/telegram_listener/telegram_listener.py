#!/usr/bin/env python3
"""
TizenClaw Telegram Listener
Polls a Telegram Bot for messages and translates them into Tizen AppControl
launch requests directed to the TizenClaw service (org.tizen.tizenclaw).
"""

import urllib.request
import urllib.parse
import json
import ctypes
import sys
import time
import os

def send_prompt_to_tizenclaw(prompt_text):
    try:
        libappcontrol = ctypes.CDLL("libcapi-appfw-app-control.so.0")
    except OSError as e:
        print(f"Error loading libcapi-appfw-app-control: {e}")
        return False

    app_control_h = ctypes.c_void_p()
    
    app_control_create = libappcontrol.app_control_create
    app_control_create.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
    
    app_control_set_app_id = libappcontrol.app_control_set_app_id
    app_control_set_app_id.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    
    app_control_add_extra_data = libappcontrol.app_control_add_extra_data
    app_control_add_extra_data.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
    
    app_control_send_launch_request = libappcontrol.app_control_send_launch_request
    app_control_send_launch_request.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p]
    
    app_control_destroy = libappcontrol.app_control_destroy
    app_control_destroy.argtypes = [ctypes.c_void_p]
    
    libappcontrol.app_control_create(ctypes.byref(app_control_h))
    libappcontrol.app_control_set_app_id(app_control_h, b"org.tizen.tizenclaw")
    libappcontrol.app_control_add_extra_data(app_control_h, b"prompt", prompt_text.encode('utf-8'))
    
    ret = libappcontrol.app_control_send_launch_request(app_control_h, None, None)
    
    libappcontrol.app_control_destroy(app_control_h)
    return ret == 0

def poll_telegram_bot(token):
    offset = 0
    url = f"https://api.telegram.org/bot{token}"
    print(f"Starting Telegram polling for TizenClaw...")
    
    while True:
        try:
            req_url = f"{url}/getUpdates?offset={offset}&timeout=30"
            req = urllib.request.Request(req_url)
            with urllib.request.urlopen(req, timeout=40) as response:
                data = json.loads(response.read().decode())
                
                if data.get("ok"):
                    for result in data["result"]:
                        offset = result["update_id"] + 1
                        message = result.get("message", {})
                        text = message.get("text")
                        
                        if text:
                            print(f"Received Telegram Message: '{text}' -> Forwarding to AgentCore")
                            success = send_prompt_to_tizenclaw(text)
                            if not success:
                                print(f"Failed to forward message.")
        except urllib.error.URLError as e:
            print(f"Network error: {e}")
            time.sleep(5)
        except Exception as e:
            print(f"Exception during polling: {e}")
            time.sleep(5)

if __name__ == "__main__":
    bot_token = os.environ.get("TELEGRAM_BOT_TOKEN")
    if not bot_token:
        print("Please set TELEGRAM_BOT_TOKEN environment variable.")
        sys.exit(1)
        
    poll_telegram_bot(bot_token)
