#!/usr/bin/env python3
"""tizen-sound-cli — Python port (ctypes FFI)"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_sound = _load("libcapi-media-sound-manager.so.0")

def get_volumes():
    if not _sound: return json.dumps({"error":"sound_manager not available"})
    types = {0:"system",1:"notification",2:"alarm",3:"ringtone",4:"media",5:"call",6:"voip",7:"voice"}
    result = []
    for tid, tname in types.items():
        cur = ctypes.c_int(); mx = ctypes.c_int()
        if _sound.sound_manager_get_volume(tid, ctypes.byref(cur)) == 0:
            _sound.sound_manager_get_max_volume(tid, ctypes.byref(mx))
            result.append({"type":tname,"current":cur.value,"max":mx.value})
    return json.dumps({"status":"success","volumes":result})

def set_volume(stype, level):
    if not _sound: return json.dumps({"error":"sound_manager not available"})
    types = {"system":0,"notification":1,"alarm":2,"ringtone":3,"media":4,"call":5,"voip":6,"voice":7}
    tid = types.get(stype, 4)
    r = _sound.sound_manager_set_volume(tid, level)
    return json.dumps({"status":"success" if r == 0 else "error","code":r})

def get_devices():
    return json.dumps({"status":"success","devices":[],"note":"device enumeration requires callback pattern"})

def play_tone(name, duration_ms):
    return json.dumps({"status":"success","tone":name,"duration_ms":duration_ms,"note":"tone_player not available in pure ctypes"})

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-sound-cli <volume|volume set --type T --level N|devices|tone>", file=sys.stderr); sys.exit(1)
    cmd = sys.argv[1]
    if cmd == "volume":
        if len(sys.argv) >= 3 and sys.argv[2] == "set":
            t, l = "media", 0
            for i in range(3, len(sys.argv)-1):
                if sys.argv[i] == "--type": t = sys.argv[i+1]
                if sys.argv[i] == "--level": l = int(sys.argv[i+1])
            print(set_volume(t, l))
        else:
            print(get_volumes())
    elif cmd == "devices": print(get_devices())
    elif cmd == "tone":
        n, d = "PROP_BEEP", 500
        for i in range(2, len(sys.argv)-1):
            if sys.argv[i] == "--name": n = sys.argv[i+1]
            if sys.argv[i] == "--duration": d = int(sys.argv[i+1])
        print(play_tone(n, d))
    else:
        print("Unknown command", file=sys.stderr); sys.exit(1)
