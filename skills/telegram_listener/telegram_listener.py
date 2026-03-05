#!/usr/bin/env python3
"""
TizenClaw Telegram Listener
Polls a Telegram Bot for messages, sends them to the
TizenClaw daemon via IPC socket, and replies to Telegram
with the daemon's response.
"""

import urllib.request
import urllib.parse
import json
import socket
import sys
import time
import os


def send_prompt_to_tizenclaw(prompt_text, chat_id):
    """Send prompt via IPC and receive response."""
    try:
        sock = socket.socket(
            socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(60)
        sock.connect("\0tizenclaw.ipc")

        # Send JSON request
        request = json.dumps({
            "type": "prompt",
            "session_id": f"telegram_{chat_id}",
            "text": prompt_text
        }).encode('utf-8')
        sock.sendall(request)

        # Signal end of write so daemon knows
        # we're done sending
        sock.shutdown(socket.SHUT_WR)

        # Read response
        response_data = b""
        while True:
            chunk = sock.recv(4096)
            if not chunk:
                break
            response_data += chunk

        sock.close()

        if response_data:
            resp = json.loads(
                response_data.decode('utf-8'))
            return resp
        return {"status": "error",
                "text": "Empty response from daemon"}

    except socket.timeout:
        return {"status": "error",
                "text": "Request timed out"}
    except OSError as e:
        print(f"IPC socket error: {e}")
        return {"status": "error",
                "text": f"Connection failed: {e}"}
    except json.JSONDecodeError as e:
        print(f"Response parse error: {e}")
        return {"status": "error",
                "text": "Invalid response format"}


def send_telegram_message(token, chat_id, text):
    """Send a message back to a Telegram chat."""
    url = (f"https://api.telegram.org/bot{token}"
           f"/sendMessage")

    # Telegram has 4096 char limit per message
    max_len = 4000
    if len(text) > max_len:
        text = text[:max_len] + "\n...(truncated)"

    data = json.dumps({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "Markdown"
    }).encode('utf-8')

    req = urllib.request.Request(
        url, data=data,
        headers={"Content-Type":
                 "application/json"})
    try:
        with urllib.request.urlopen(
                req, timeout=10) as resp:
            return resp.status == 200
    except Exception as e:
        print(f"Failed to send Telegram msg: {e}")
        # Retry without Markdown parse mode
        try:
            data = json.dumps({
                "chat_id": chat_id,
                "text": text
            }).encode('utf-8')
            req = urllib.request.Request(
                url, data=data,
                headers={"Content-Type":
                         "application/json"})
            with urllib.request.urlopen(
                    req, timeout=10) as resp:
                return resp.status == 200
        except Exception as e2:
            print(f"Retry also failed: {e2}")
            return False


def poll_telegram_bot(token, allowed_chat_ids=None):
    """Main polling loop for Telegram updates."""
    offset = 0
    url = f"https://api.telegram.org/bot{token}"
    print("Starting Telegram polling for "
          "TizenClaw...")
    if allowed_chat_ids:
        print(f"Allowed chat IDs: "
              f"{allowed_chat_ids}")
    else:
        print("No chat ID restrictions "
              "(all users allowed)")

    while True:
        try:
            req_url = (f"{url}/getUpdates"
                       f"?offset={offset}"
                       f"&timeout=30")
            req = urllib.request.Request(req_url)
            with urllib.request.urlopen(
                    req, timeout=40) as response:
                data = json.loads(
                    response.read().decode())

                if data.get("ok"):
                    for result in data["result"]:
                        offset = (
                            result["update_id"] + 1)
                        message = result.get(
                            "message", {})
                        text = message.get("text")
                        chat_id = message.get(
                            "chat", {}).get("id")

                        if text and chat_id:
                            # Check allowlist
                            if (allowed_chat_ids and
                                    chat_id not in
                                    allowed_chat_ids):
                                print(
                                    f"Blocked [{chat_id}]"
                                    f": not in allowlist")
                                continue

                            print(
                                f"Telegram [{chat_id}]"
                                f": '{text}' -> "
                                f"Forwarding")

                            resp = (
                                send_prompt_to_tizenclaw(
                                    text, chat_id))

                            reply = resp.get(
                                "text",
                                "No response")
                            status = resp.get(
                                "status", "error")

                            if status == "error":
                                reply = (
                                    f"⚠️ {reply}")

                            send_telegram_message(
                                token, chat_id,
                                reply)

        except urllib.error.URLError as e:
            print(f"Network error: {e}")
            time.sleep(5)
        except Exception as e:
            print(f"Polling exception: {e}")
            time.sleep(5)


if __name__ == "__main__":
    bot_token = os.environ.get(
        "TELEGRAM_BOT_TOKEN")
    if not bot_token:
        print("Please set TELEGRAM_BOT_TOKEN "
              "environment variable.")
        sys.exit(1)

    # Load allowed_chat_ids from config
    _allowed_ids = None
    _config_path = (
        "/opt/usr/share/tizenclaw/"
        "telegram_config.json")
    try:
        with open(_config_path) as _f:
            _cfg = json.load(_f)
            _ids = _cfg.get(
                "allowed_chat_ids", [])
            if _ids:
                _allowed_ids = set(
                    int(x) for x in _ids)
    except (FileNotFoundError, json.JSONDecodeError,
            ValueError) as _e:
        print(f"Config load note: {_e}")

    poll_telegram_bot(bot_token, _allowed_ids)
