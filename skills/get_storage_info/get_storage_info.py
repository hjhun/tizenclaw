#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


# Callback type: bool (*)(int storage_id, storage_type_e type,
#   storage_state_e state, const char *path, void *user_data)
STORAGE_CB = ctypes.CFUNCTYPE(
    ctypes.c_bool, ctypes.c_int, ctypes.c_int, ctypes.c_int,
    ctypes.c_char_p, ctypes.POINTER(ctypes.py_object)
)


def get_storage_info():
    try:
        lib = tizen_capi_utils.load_library(
            ["libstorage.so.0.1", "libstorage.so.0"]
        )

        # int storage_get_internal_memory_size(struct statvfs *buf)
        # Simpler approach: use statvfs directly via ctypes
        # Or use storage_foreach_device_supported + storage_get_total/available_space

        # int storage_get_total_space(int storage_id, unsigned long long *bytes)
        lib.storage_get_total_space.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_ulonglong)
        ]
        lib.storage_get_total_space.restype = ctypes.c_int

        # int storage_get_available_space(int storage_id, unsigned long long *bytes)
        lib.storage_get_available_space.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_ulonglong)
        ]
        lib.storage_get_available_space.restype = ctypes.c_int

        # int storage_get_root_directory(int storage_id, char **path)
        lib.storage_get_root_directory.argtypes = [
            ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.storage_get_root_directory.restype = ctypes.c_int

        # Collect storage devices via callback
        storages = []

        def _cb(storage_id, stype, state, path, user_data):
            type_map = {0: "internal", 1: "external", 2: "extended_internal"}
            state_map = {
                0: "unmountable", 1: "removed", 2: "mounted",
                3: "mounted_read_only",
            }
            storages.append({
                "id": storage_id,
                "type": type_map.get(stype, "unknown"),
                "state": state_map.get(state, "unknown"),
                "path": path.decode("utf-8") if path else "",
            })
            return True

        cb = STORAGE_CB(_cb)

        # int storage_foreach_device_supported(storage_device_supported_cb cb, void *ud)
        lib.storage_foreach_device_supported.argtypes = [STORAGE_CB, ctypes.c_void_p]
        lib.storage_foreach_device_supported.restype = ctypes.c_int

        lib.storage_foreach_device_supported(cb, None)

        # Get space info for each storage
        for s in storages:
            total = ctypes.c_ulonglong()
            avail = ctypes.c_ulonglong()
            if lib.storage_get_total_space(s["id"], ctypes.byref(total)) == 0:
                s["total_bytes"] = total.value
                s["total_mb"] = round(total.value / (1024 * 1024), 1)
            if lib.storage_get_available_space(s["id"], ctypes.byref(avail)) == 0:
                s["available_bytes"] = avail.value
                s["available_mb"] = round(avail.value / (1024 * 1024), 1)
                if total.value > 0:
                    s["used_percent"] = round(
                        (1 - avail.value / total.value) * 100, 1
                    )

        return {"storages": storages}

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_storage_info()))
