#!/usr/bin/env python3
"""tizen-file-manager-cli — Python port (pure stdlib)"""
import json, os, shutil, sys, stat, time
import urllib.request

def _get_arg(key, default=""):
    for i in range(2, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

def read_file(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f: return json.dumps({"status":"success","path":path,"content":f.read()})
    except Exception as e: return json.dumps({"error":str(e)})

def write_file(path, content):
    try:
        with open(path, "w", encoding="utf-8") as f: f.write(content)
        return json.dumps({"status":"success","path":path,"bytes_written":len(content)})
    except Exception as e: return json.dumps({"error":str(e)})

def append_file(path, content):
    try:
        with open(path, "a", encoding="utf-8") as f: f.write(content)
        return json.dumps({"status":"success","path":path})
    except Exception as e: return json.dumps({"error":str(e)})

def remove(path):
    try:
        if os.path.isdir(path): shutil.rmtree(path)
        else: os.remove(path)
        return json.dumps({"status":"success","removed":path})
    except Exception as e: return json.dumps({"error":str(e)})

def mkdir(path):
    try:
        os.makedirs(path, exist_ok=True)
        return json.dumps({"status":"success","path":path})
    except Exception as e: return json.dumps({"error":str(e)})

def list_dir(path):
    try:
        entries = []
        for e in os.listdir(path):
            fp = os.path.join(path, e)
            s = os.stat(fp)
            entries.append({"name":e,"is_dir":os.path.isdir(fp),"size":s.st_size,"modified":time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(s.st_mtime))})
        return json.dumps({"status":"success","path":path,"entries":entries})
    except Exception as e: return json.dumps({"error":str(e)})

def file_stat(path):
    try:
        s = os.stat(path)
        return json.dumps({"status":"success","path":path,"size":s.st_size,"mode":oct(s.st_mode),
            "uid":s.st_uid,"gid":s.st_gid,"is_dir":stat.S_ISDIR(s.st_mode),
            "modified":time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(s.st_mtime)),
            "created":time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(s.st_ctime))})
    except Exception as e: return json.dumps({"error":str(e)})

def copy(src, dst):
    try:
        if os.path.isdir(src): shutil.copytree(src, dst)
        else: shutil.copy2(src, dst)
        return json.dumps({"status":"success","src":src,"dst":dst})
    except Exception as e: return json.dumps({"error":str(e)})

def move(src, dst):
    try:
        shutil.move(src, dst)
        return json.dumps({"status":"success","src":src,"dst":dst})
    except Exception as e: return json.dumps({"error":str(e)})

def download(url, dest):
    try:
        urllib.request.urlretrieve(url, dest)
        return json.dumps({"status":"success","url":url,"dest":dest,"size":os.path.getsize(dest)})
    except Exception as e: return json.dumps({"error":str(e)})

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-file-manager-cli <read|write|append|remove|mkdir|list|stat|copy|move|download>", file=sys.stderr); sys.exit(1)
    cmd = sys.argv[1]
    path = _get_arg("--path")
    if cmd == "read": print(read_file(path))
    elif cmd == "write": print(write_file(path, _get_arg("--content")))
    elif cmd == "append": print(append_file(path, _get_arg("--content")))
    elif cmd == "remove": print(remove(path))
    elif cmd == "mkdir": print(mkdir(path))
    elif cmd == "list": print(list_dir(path))
    elif cmd == "stat": print(file_stat(path))
    elif cmd == "copy": print(copy(_get_arg("--src"), _get_arg("--dst")))
    elif cmd == "move": print(move(_get_arg("--src"), _get_arg("--dst")))
    elif cmd == "download": print(download(_get_arg("--url"), _get_arg("--dest")))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
