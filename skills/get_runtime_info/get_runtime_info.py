#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


class RuntimeCpuUsage(ctypes.Structure):
    _fields_ = [
        ("user", ctypes.c_double),
        ("system", ctypes.c_double),
        ("nice", ctypes.c_double),
        ("iowait", ctypes.c_double),
    ]


class RuntimeMemoryInfo(ctypes.Structure):
    _fields_ = [
        ("total", ctypes.c_int),
        ("used", ctypes.c_int),
        ("free", ctypes.c_int),
        ("cache", ctypes.c_int),
        ("swap", ctypes.c_int),
    ]


def get_runtime_info():
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-system-runtime-info.so.0", "libcapi-system-runtime-info.so.1"]
        )

        # int runtime_info_get_system_memory_info(runtime_memory_info_s *info)
        lib.runtime_info_get_system_memory_info.argtypes = [
            ctypes.POINTER(RuntimeMemoryInfo)
        ]
        lib.runtime_info_get_system_memory_info.restype = ctypes.c_int

        # int runtime_info_get_cpu_usage(runtime_cpu_usage_s *usage)
        lib.runtime_info_get_cpu_usage.argtypes = [
            ctypes.POINTER(RuntimeCpuUsage)
        ]
        lib.runtime_info_get_cpu_usage.restype = ctypes.c_int

        mem = RuntimeMemoryInfo()
        cpu = RuntimeCpuUsage()

        result = {}

        ret = lib.runtime_info_get_system_memory_info(ctypes.byref(mem))
        if ret == 0:
            result["memory"] = {
                "total_kb": mem.total,
                "used_kb": mem.used,
                "free_kb": mem.free,
                "cache_kb": mem.cache,
                "swap_kb": mem.swap,
                "usage_percent": round(
                    (mem.used / mem.total * 100) if mem.total > 0 else 0, 1
                ),
            }
        else:
            result["memory"] = {"error": f"Failed to get memory info (code: {ret})"}

        ret = lib.runtime_info_get_cpu_usage(ctypes.byref(cpu))
        if ret == 0:
            result["cpu"] = {
                "user_percent": round(cpu.user, 1),
                "system_percent": round(cpu.system, 1),
                "nice_percent": round(cpu.nice, 1),
                "iowait_percent": round(cpu.iowait, 1),
                "total_usage_percent": round(cpu.user + cpu.system, 1),
            }
        else:
            result["cpu"] = {"error": f"Failed to get CPU info (code: {ret})"}

        return result

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_runtime_info()))
