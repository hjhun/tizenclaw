#!/usr/bin/env python3
import ctypes
import json
import os
import sys
import time

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils

# sensor_type_e values
SENSOR_TYPES = {
    "accelerometer": 1,
    "gravity": 2,
    "linear_acceleration": 3,
    "magnetic": 4,
    "rotation_vector": 5,
    "orientation": 6,
    "gyroscope": 7,
    "light": 8,
    "proximity": 9,
    "pressure": 10,
}


class SensorEvent(ctypes.Structure):
    _fields_ = [
        ("accuracy", ctypes.c_int),
        ("timestamp", ctypes.c_ulonglong),
        ("value_count", ctypes.c_int),
        ("values", ctypes.c_float * 16),
    ]


def get_sensor_data(sensor_type_name):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-sensor.so.0", "libcapi-system-sensor.so.1"]
        )

        sensor_type_val = SENSOR_TYPES.get(sensor_type_name)
        if sensor_type_val is None:
            return {"error": f"Unknown sensor type: {sensor_type_name}"}

        # int sensor_get_default_sensor(sensor_type_e type, sensor_h *sensor)
        lib.sensor_get_default_sensor.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_void_p)
        ]
        lib.sensor_get_default_sensor.restype = ctypes.c_int

        # int sensor_create_listener(sensor_h sensor, sensor_listener_h *listener)
        lib.sensor_create_listener.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p)
        ]
        lib.sensor_create_listener.restype = ctypes.c_int

        # int sensor_listener_start(sensor_listener_h listener)
        lib.sensor_listener_start.argtypes = [ctypes.c_void_p]
        lib.sensor_listener_start.restype = ctypes.c_int

        # int sensor_listener_read_data(sensor_listener_h listener,
        #   sensor_event_s *event)
        lib.sensor_listener_read_data.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(SensorEvent)
        ]
        lib.sensor_listener_read_data.restype = ctypes.c_int

        # int sensor_listener_stop(sensor_listener_h listener)
        lib.sensor_listener_stop.argtypes = [ctypes.c_void_p]
        lib.sensor_listener_stop.restype = ctypes.c_int

        # int sensor_destroy_listener(sensor_listener_h listener)
        lib.sensor_destroy_listener.argtypes = [ctypes.c_void_p]
        lib.sensor_destroy_listener.restype = ctypes.c_int

        # Check if sensor is available
        sensor = ctypes.c_void_p()
        ret = lib.sensor_get_default_sensor(sensor_type_val, ctypes.byref(sensor))
        if ret != 0:
            return {
                "error": f"Sensor '{sensor_type_name}' not available on this device",
                "sensor_type": sensor_type_name,
            }

        listener = ctypes.c_void_p()
        tizen_capi_utils.check_return(
            lib.sensor_create_listener(sensor, ctypes.byref(listener)),
            "Failed to create sensor listener"
        )

        tizen_capi_utils.check_return(
            lib.sensor_listener_start(listener),
            "Failed to start sensor listener"
        )

        # Brief delay for sensor to produce data
        time.sleep(0.2)

        event = SensorEvent()
        ret = lib.sensor_listener_read_data(listener, ctypes.byref(event))

        lib.sensor_listener_stop(listener)
        lib.sensor_destroy_listener(listener)

        if ret != 0:
            return {"error": f"Failed to read sensor data (code: {ret})"}

        # Build value labels based on sensor type
        values = [event.values[i] for i in range(min(event.value_count, 16))]
        labeled = {}

        if sensor_type_name in ("accelerometer", "gravity", "linear_acceleration",
                                 "magnetic", "gyroscope"):
            keys = ["x", "y", "z"]
        elif sensor_type_name in ("orientation", "rotation_vector"):
            keys = ["x", "y", "z", "w"]
        elif sensor_type_name == "light":
            keys = ["lux"]
        elif sensor_type_name == "proximity":
            keys = ["distance"]
        elif sensor_type_name == "pressure":
            keys = ["hpa"]
        else:
            keys = [f"v{i}" for i in range(len(values))]

        for i, v in enumerate(values):
            key = keys[i] if i < len(keys) else f"v{i}"
            labeled[key] = round(v, 4)

        accuracy_map = {0: "undefined", 1: "unreliable", 2: "low", 3: "medium", 4: "high"}

        return {
            "sensor_type": sensor_type_name,
            "values": labeled,
            "accuracy": accuracy_map.get(event.accuracy, "unknown"),
            "timestamp": event.timestamp,
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    st = args.get("sensor_type", "accelerometer")
    print(json.dumps(get_sensor_data(st)))
