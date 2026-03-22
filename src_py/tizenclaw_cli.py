#!/usr/bin/env python3
import sys
import json
import socket
import struct
import argparse

SOCKET_PATH = "\0tizenclaw.sock"

class SocketClient:
    def __init__(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(10.0)

    def connect(self):
        try:
            self.sock.connect(SOCKET_PATH)
            return True
        except Exception as e:
            print(f"Failed to connect to daemon: {e}", file=sys.stderr)
            return False

    def send_json_rpc(self, method: str, params: dict = None) -> dict:
        req = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params or {},
            "id": 1
        }
        try:
            req_bytes = json.dumps(req).encode('utf-8')
            self.sock.sendall(struct.pack("!I", len(req_bytes)) + req_bytes)
            
            # Read length prefixed response
            len_bytes = self.sock.recv(4)
            if len(len_bytes) != 4:
                return {}
            msg_len = struct.unpack("!I", len_bytes)[0]
            
            data = b""
            while len(data) < msg_len:
                chunk = self.sock.recv(min(4096, msg_len - len(data)))
                if not chunk:
                    break
                data += chunk
                
            return json.loads(data.decode('utf-8'))
        except Exception as e:
            print(f"IPC Error: {e}", file=sys.stderr)
            return {}
            
    def close(self):
        self.sock.close()

def main():
    parser = argparse.ArgumentParser(description="tizenclaw-cli Python Port")
    parser.add_argument("-s", "--session", default="cli_test", help="Session ID")
    parser.add_argument("--list-agents", action="store_true", help="List all running agents")
    parser.add_argument("prompt", nargs="*", help="The prompt to send")
    
    args = parser.parse_args()

    client = SocketClient()
    if not client.connect():
        sys.exit(1)

    if args.list_agents:
        resp = client.send_json_rpc("list_agents")
        print(json.dumps(resp.get("result", []), indent=2))
    elif args.prompt:
        prompt_text = " ".join(args.prompt)
        resp = client.send_json_rpc("prompt", {
            "session_id": args.session,
            "text": prompt_text,
            "stream": False
        })
        print(resp.get("result", {}).get("text", "Error: No text returned"))
    else:
        parser.print_help()

    client.close()

if __name__ == "__main__":
    main()
