#!/usr/bin/env python3
"""tizen-media-cli — Python port"""
import ctypes, json, sys, os, mimetypes as _mt

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_content = _load("libcapi-content-media-content.so.0")
_meta = _load("libcapi-media-metadata-extractor.so.0")
_mime = _load("libcapi-content-mime-type.so.0")

def _get_arg(key, default=""):
    for i in range(2, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

def list_media(media_type, max_count):
    """List media files using filesystem scan as fallback"""
    media_dirs = ["/opt/usr/media", "/opt/media"]
    exts_map = {"image":[".jpg",".jpeg",".png",".gif",".bmp"],
                "video":[".mp4",".avi",".mkv",".webm"],
                "audio":[".mp3",".wav",".ogg",".flac",".aac"],
                "all":None}
    allowed = exts_map.get(media_type, None)
    files = []
    for mdir in media_dirs:
        if not os.path.isdir(mdir): continue
        for root, dirs, fnames in os.walk(mdir):
            for fn in fnames:
                if allowed and not any(fn.lower().endswith(e) for e in allowed): continue
                fp = os.path.join(root, fn)
                try:
                    st = os.stat(fp)
                    files.append({"path":fp,"name":fn,"size":st.st_size})
                except: pass
                if len(files) >= max_count: break
            if len(files) >= max_count: break
    return json.dumps({"status":"success","type":media_type,"count":len(files),"files":files})

def get_metadata(path):
    if not os.path.isfile(path): return json.dumps({"error":"file not found"})
    r = {"status":"success","path":path,"size":os.path.getsize(path)}
    if _meta:
        h = ctypes.c_void_p()
        if _meta.metadata_extractor_create(ctypes.byref(h)) == 0:
            if _meta.metadata_extractor_set_path(h, path.encode()) == 0:
                for attr_id, name in [(0,"duration"),(1,"video_codec"),(2,"audio_codec"),(5,"artist"),(6,"title"),(7,"album"),(10,"genre"),(18,"width"),(19,"height")]:
                    val = ctypes.c_char_p()
                    if _meta.metadata_extractor_get_metadata(h, attr_id, ctypes.byref(val)) == 0 and val.value:
                        r[name] = val.value.decode()
            _meta.metadata_extractor_destroy(h)
    return json.dumps(r)

def get_mime_type(path):
    if _mime:
        mime = ctypes.c_char_p()
        if _mime.mime_type_get_mime_type(os.path.splitext(path)[1].lstrip(".").encode(), ctypes.byref(mime)) == 0 and mime.value:
            return json.dumps({"status":"success","path":path,"mime":mime.value.decode()})
    # Fallback to Python mimetypes
    m, _ = _mt.guess_type(path)
    return json.dumps({"status":"success","path":path,"mime":m or "application/octet-stream"})

def get_extensions(mime_type):
    exts = _mt.guess_all_extensions(mime_type)
    return json.dumps({"status":"success","mime":mime_type,"extensions":exts})

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: tizen-media-cli <content|metadata|mime|mime-ext>", file=sys.stderr); sys.exit(1)
    cmd = sys.argv[1]
    if cmd == "content":
        t = _get_arg("--type", "all"); m = int(_get_arg("--max", "20"))
        print(list_media(t, m))
    elif cmd == "metadata": print(get_metadata(_get_arg("--path")))
    elif cmd == "mime": print(get_mime_type(_get_arg("--path")))
    elif cmd == "mime-ext": print(get_extensions(_get_arg("--mime")))
    else: print("Unknown command", file=sys.stderr); sys.exit(1)
