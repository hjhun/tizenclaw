#!/usr/bin/env python3
"""tizen-network-info-cli — Python port"""
import ctypes, json, sys

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_conn = _load("libcapi-network-connection.so.0")
_wifi = _load("libcapi-network-wifi-manager.so.0")
_bt = _load("libcapi-network-bluetooth.so.0")

def network_info():
    r = {"status":"success"}
    if _conn:
        h = ctypes.c_void_p()
        if _conn.connection_create(ctypes.byref(h)) == 0:
            t = ctypes.c_int()
            _conn.connection_get_type(h, ctypes.byref(t))
            types = {0:"disconnected",1:"wifi",2:"cellular",3:"ethernet",4:"bluetooth",5:"net_proxy"}
            r["type"] = types.get(t.value, "unknown")
            ip = ctypes.c_char_p()
            if _conn.connection_get_ip_address(h, 0, ctypes.byref(ip)) == 0 and ip.value:
                r["ipv4"] = ip.value.decode()
            _conn.connection_destroy(h)
    return json.dumps(r)

def wifi_info():
    r = {"status":"success"}
    if _wifi:
        h = ctypes.c_void_p()
        if _wifi.wifi_manager_initialize(ctypes.byref(h)) == 0:
            act = ctypes.c_bool()
            _wifi.wifi_manager_is_activated(h, ctypes.byref(act))
            r["activated"] = act.value
            ap = ctypes.c_void_p()
            if _wifi.wifi_manager_get_connected_ap(h, ctypes.byref(ap)) == 0 and ap.value:
                ssid = ctypes.c_char_p()
                _wifi.wifi_manager_ap_get_essid(ap, ctypes.byref(ssid))
                if ssid.value: r["ssid"] = ssid.value.decode()
                rssi = ctypes.c_int()
                _wifi.wifi_manager_ap_get_rssi(ap, ctypes.byref(rssi))
                r["rssi"] = rssi.value
            _wifi.wifi_manager_deinitialize(h)
    return json.dumps(r)

def bluetooth_info():
    r = {"status":"success"}
    if _bt:
        if _bt.bt_initialize() == 0:
            st = ctypes.c_int()
            _bt.bt_adapter_get_state(ctypes.byref(st))
            r["enabled"] = st.value != 0
            name = ctypes.c_char_p()
            if _bt.bt_adapter_get_name(ctypes.byref(name)) == 0 and name.value:
                r["name"] = name.value.decode()
            addr = ctypes.c_char_p()
            if _bt.bt_adapter_get_address(ctypes.byref(addr)) == 0 and addr.value:
                r["address"] = addr.value.decode()
            _bt.bt_deinitialize()
    return json.dumps(r)

def data_usage():
    r = {"status":"success"}
    try:
        with open("/proc/net/dev") as f:
            lines = f.readlines()[2:]
            for line in lines:
                parts = line.split()
                iface = parts[0].rstrip(":")
                if iface in ("lo",): continue
                r.setdefault("interfaces", []).append({
                    "name": iface, "rx_bytes": int(parts[1]), "tx_bytes": int(parts[9])
                })
    except: pass
    return json.dumps(r)

COMMANDS = {"network":network_info,"wifi":wifi_info,"wifi-scan":lambda:json.dumps({"status":"success","networks":[],"note":"scan requires callback"}),"bluetooth":bluetooth_info,"bt-scan":lambda:json.dumps({"status":"success","devices":[],"note":"scan requires callback"}),"data-usage":data_usage}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(f"Usage: {sys.argv[0]} <{'|'.join(COMMANDS.keys())}>", file=sys.stderr); sys.exit(1)
    print(COMMANDS[sys.argv[1]]())
