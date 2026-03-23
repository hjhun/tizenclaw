#!/usr/bin/env python3
"""tizen-device-info-cli — Python port (ctypes FFI to Tizen C-API)"""
import ctypes, ctypes.util, json, sys, os

def _load(name):
    for n in [name, name.replace('.so.0', '.so')]:
        try: return ctypes.CDLL(n)
        except OSError: pass
    return None

_device = _load("libcapi-system-device.so.0")
_sysinfo = _load("libcapi-system-info.so.0")
_runtime = _load("libcapi-system-runtime-info.so.0")
_storage = _load("libstorage.so.0")
_settings = _load("libcapi-system-system-settings.so.0")

# ─── helpers ───
def _si_string(key):
    p = ctypes.c_char_p()
    if _sysinfo and _sysinfo.system_info_get_platform_string(key.encode(), ctypes.byref(p)) == 0 and p.value:
        v = p.value.decode(); ctypes.CDLL("libc.so.6").free(p); return v
    return ""

def _si_int(key):
    v = ctypes.c_int()
    if _sysinfo and _sysinfo.system_info_get_platform_int(key.encode(), ctypes.byref(v)) == 0: return v.value
    return 0

def _si_bool(key):
    v = ctypes.c_bool()
    if _sysinfo and _sysinfo.system_info_get_platform_bool(key.encode(), ctypes.byref(v)) == 0: return v.value
    return False

# ─── subcommands ───
def battery():
    if not _device: return json.dumps({"error": "libcapi-system-device not available"})
    pct = ctypes.c_int(); _device.device_battery_get_percent(ctypes.byref(pct))
    ch = ctypes.c_bool(); _device.device_battery_is_charging(ctypes.byref(ch))
    lv = ctypes.c_int(); _device.device_battery_get_level_status(ctypes.byref(lv))
    levels = {0:"empty",1:"critical",2:"low",3:"high",4:"full"}
    return json.dumps({"status":"success","percent":pct.value,"is_charging":ch.value,"level_status":levels.get(lv.value,"unknown")})

def system_info():
    return json.dumps({
        "model_name": _si_string("http://tizen.org/system/model_name"),
        "platform_name": _si_string("http://tizen.org/system/platform.name"),
        "platform_version": _si_string("http://tizen.org/feature/platform.version"),
        "build_string": _si_string("http://tizen.org/system/build.string"),
        "build_type": _si_string("http://tizen.org/system/build.type"),
        "manufacturer": _si_string("http://tizen.org/system/manufacturer"),
        "cpu_arch": _si_string("http://tizen.org/feature/platform.core.cpu.arch"),
        "screen_width": _si_int("http://tizen.org/feature/screen.width"),
        "screen_height": _si_int("http://tizen.org/feature/screen.height"),
        "screen_dpi": _si_int("http://tizen.org/feature/screen.dpi"),
        "features": {
            "bluetooth": _si_bool("http://tizen.org/feature/network.bluetooth"),
            "wifi": _si_bool("http://tizen.org/feature/network.wifi"),
            "gps": _si_bool("http://tizen.org/feature/location.gps"),
            "camera": _si_bool("http://tizen.org/feature/camera"),
            "nfc": _si_bool("http://tizen.org/feature/network.nfc"),
            "accelerometer": _si_bool("http://tizen.org/feature/sensor.accelerometer"),
            "barometer": _si_bool("http://tizen.org/feature/sensor.barometer"),
            "gyroscope": _si_bool("http://tizen.org/feature/sensor.gyroscope"),
        }
    })

def runtime_info():
    if not _runtime: return json.dumps({"error": "runtime_info not available"})
    # Use /proc for cross-platform compatibility
    mem = {}
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                parts = line.split()
                if parts[0] in ("MemTotal:", "MemAvailable:", "MemFree:"):
                    mem[parts[0].rstrip(":")] = int(parts[1])
    except: pass
    cpu_load = 0.0
    try:
        with open("/proc/loadavg") as f:
            cpu_load = float(f.read().split()[0])
    except: pass
    return json.dumps({"status":"success","MemTotal_kB":mem.get("MemTotal",0),"MemAvailable_kB":mem.get("MemAvailable",0),"MemFree_kB":mem.get("MemFree",0),"cpu_load_1m":cpu_load})

def storage_info():
    result = []
    try:
        st = os.statvfs("/opt/usr")
        total = st.f_frsize * st.f_blocks // 1024
        avail = st.f_frsize * st.f_bavail // 1024
        result.append({"id":"internal","total_kB":total,"available_kB":avail})
    except: pass
    try:
        st = os.statvfs("/opt/media")
        total = st.f_frsize * st.f_blocks // 1024
        avail = st.f_frsize * st.f_bavail // 1024
        result.append({"id":"media","total_kB":total,"available_kB":avail})
    except: pass
    return json.dumps({"status":"success","storages":result})

def thermal_info():
    zones = []
    try:
        base = "/sys/class/thermal"
        for d in os.listdir(base):
            if d.startswith("thermal_zone"):
                try:
                    temp = int(open(f"{base}/{d}/temp").read().strip()) / 1000.0
                    ttype = open(f"{base}/{d}/type").read().strip()
                    zones.append({"zone":d,"type":ttype,"temp_c":temp})
                except: pass
    except: pass
    return json.dumps({"status":"success","zones":zones})

def display_info():
    result = {"status":"success"}
    if _device:
        b = ctypes.c_int()
        if _device.device_display_get_brightness(0, ctypes.byref(b)) == 0:
            result["brightness"] = b.value
        mb = ctypes.c_int()
        if _device.device_display_get_max_brightness(0, ctypes.byref(mb)) == 0:
            result["max_brightness"] = mb.value
    return json.dumps(result)

def settings():
    keys = {0:"incoming_call_ringtone",1:"wallpaper_home_screen",2:"wallpaper_lock_screen",
            3:"font_size",4:"font_type",5:"motion_activation",8:"email_alert_ringtone",
            9:"usb_debugging_enabled",21:"locale_country",22:"locale_language",
            25:"locale_timezone",26:"time_changed",30:"developer_option_state"}
    result = {}
    if _settings:
        for k, name in keys.items():
            try:
                if name in ("font_size","motion_activation","usb_debugging_enabled","developer_option_state"):
                    v = ctypes.c_int()
                    if _settings.system_settings_get_value_int(k, ctypes.byref(v)) == 0: result[name] = v.value
                else:
                    v = ctypes.c_char_p()
                    if _settings.system_settings_get_value_string(k, ctypes.byref(v)) == 0 and v.value:
                        result[name] = v.value.decode()
            except: pass
    return json.dumps({"status":"success","settings":result})

COMMANDS = {"battery":battery,"system-info":system_info,"runtime":runtime_info,"storage":storage_info,"thermal":thermal_info,"display":display_info,"settings":settings}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(f"Usage: {sys.argv[0]} <{'|'.join(COMMANDS.keys())}>", file=sys.stderr); sys.exit(1)
    print(COMMANDS[sys.argv[1]]())
