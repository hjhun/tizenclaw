#!/usr/bin/env python3
"""
TizenClaw Skill: Get Device Info
Queries Tizen Device C-API to get device information like battery charge percentage.
"""
import ctypes
import sys
import json

def get_battery_info():
    try:
        # Load the Tizen system-device library
        libdevice = ctypes.CDLL("libcapi-system-device.so.0")
    except OSError as e:
        return {"error": f"Error loading libcapi-system-device: {e}"}
        
    # int device_battery_get_percent(int *status);
    device_battery_get_percent = libdevice.device_battery_get_percent
    device_battery_get_percent.argtypes = [ctypes.POINTER(ctypes.c_int)]
    device_battery_get_percent.restype = ctypes.c_int
    
    battery_level = ctypes.c_int(0)
    ret = device_battery_get_percent(ctypes.byref(battery_level))
    
    if ret == 0:
        return {"battery_percent": battery_level.value}
    else:
        return {"error": f"Failed to get battery info, code: {ret}"}

if __name__ == "__main__":
    result = get_battery_info()
    print(json.dumps(result))
