#!/usr/bin/env python3
"""tizen-sensor-cli — Python port"""
import ctypes, json, sys, time

def _load(n):
    try: return ctypes.CDLL(n)
    except OSError: return None

_sensor = _load("libcapi-system-sensor.so.0")

SENSOR_TYPES = {
    "accelerometer": 1, "gravity": 2, "gyroscope": 4,
    "light": 7, "proximity": 8, "pressure": 5,
    "magnetic": 6, "orientation": 15,
    "ultraviolet": 9, "temperature": 10,
}

class SensorEvent(ctypes.Structure):
    _fields_ = [
        ("accuracy", ctypes.c_int),
        ("timestamp", ctypes.c_ulonglong),
        ("value_count", ctypes.c_int),
        ("values", ctypes.c_float * 16),
    ]

def read_sensor(sensor_type):
    if not _sensor: return json.dumps({"error":"sensor lib not available"})
    tid = SENSOR_TYPES.get(sensor_type, 1)
    sensor = ctypes.c_void_p()
    r = _sensor.sensor_get_default_sensor(tid, ctypes.byref(sensor))
    if r != 0: return json.dumps({"error":f"sensor {sensor_type} not available","code":r})
    listener = ctypes.c_void_p()
    r = _sensor.sensor_create_listener(sensor, ctypes.byref(listener))
    if r != 0: return json.dumps({"error":"create_listener failed","code":r})
    _sensor.sensor_listener_start(listener)
    time.sleep(0.1)
    ev = SensorEvent()
    r = _sensor.sensor_listener_read_data(listener, ctypes.byref(ev))
    _sensor.sensor_listener_stop(listener)
    _sensor.sensor_destroy_listener(listener)
    if r != 0: return json.dumps({"error":"read_data failed","code":r})
    vals = [ev.values[i] for i in range(min(ev.value_count, 4))]
    return json.dumps({"status":"success","type":sensor_type,"values":vals,"accuracy":ev.accuracy,"timestamp":ev.timestamp})

if __name__ == "__main__":
    t = "accelerometer"
    for i in range(1, len(sys.argv)-1):
        if sys.argv[i] == "--type": t = sys.argv[i+1]
    if len(sys.argv) < 3:
        print("Usage: tizen-sensor-cli --type <accelerometer|gravity|...>", file=sys.stderr); sys.exit(1)
    print(read_sensor(t))
