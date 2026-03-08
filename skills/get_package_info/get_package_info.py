#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def get_package_info(package_id):
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-appfw-package-manager.so.0",
             "libcapi-appfw-package-manager.so.1"]
        )

        # int package_info_create(const char *pkg_id, package_info_h *info)
        lib.package_info_create.argtypes = [
            ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)
        ]
        lib.package_info_create.restype = ctypes.c_int

        # int package_info_get_label(package_info_h info, char **label)
        lib.package_info_get_label.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.package_info_get_label.restype = ctypes.c_int

        # int package_info_get_version(package_info_h info, char **version)
        lib.package_info_get_version.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.package_info_get_version.restype = ctypes.c_int

        # int package_info_get_type(package_info_h info, char **type)
        lib.package_info_get_type.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.package_info_get_type.restype = ctypes.c_int

        # int package_info_get_installed_storage(package_info_h info,
        #   package_info_installed_storage_type_e *storage)
        lib.package_info_get_installed_storage.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)
        ]
        lib.package_info_get_installed_storage.restype = ctypes.c_int

        # int package_info_is_system_package(package_info_h info, bool *system)
        lib.package_info_is_system_package.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_bool)
        ]
        lib.package_info_is_system_package.restype = ctypes.c_int

        # int package_info_is_removable_package(package_info_h info, bool *removable)
        lib.package_info_is_removable_package.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_bool)
        ]
        lib.package_info_is_removable_package.restype = ctypes.c_int

        # int package_info_is_preload_package(package_info_h info, bool *preload)
        lib.package_info_is_preload_package.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_bool)
        ]
        lib.package_info_is_preload_package.restype = ctypes.c_int

        # int package_info_destroy(package_info_h info)
        lib.package_info_destroy.argtypes = [ctypes.c_void_p]
        lib.package_info_destroy.restype = ctypes.c_int

        info = ctypes.c_void_p()
        ret = lib.package_info_create(
            package_id.encode("utf-8"), ctypes.byref(info)
        )
        if ret != 0:
            return {"error": f"Package '{package_id}' not found (code: {ret})"}

        def get_str(func):
            val = ctypes.c_char_p()
            if func(info, ctypes.byref(val)) == 0 and val.value:
                return val.value.decode("utf-8")
            return ""

        def get_bool(func):
            val = ctypes.c_bool()
            if func(info, ctypes.byref(val)) == 0:
                return val.value
            return None

        storage = ctypes.c_int()
        lib.package_info_get_installed_storage(info, ctypes.byref(storage))
        storage_map = {0: "internal", 1: "external"}

        result = {
            "package_id": package_id,
            "label": get_str(lib.package_info_get_label),
            "version": get_str(lib.package_info_get_version),
            "type": get_str(lib.package_info_get_type),
            "installed_storage": storage_map.get(storage.value, "unknown"),
            "is_system": get_bool(lib.package_info_is_system_package),
            "is_removable": get_bool(lib.package_info_is_removable_package),
            "is_preload": get_bool(lib.package_info_is_preload_package),
        }

        lib.package_info_destroy(info)
        return result

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    args = json.loads(os.environ.get("CLAW_ARGS", "{}"))
    pkg = args.get("package_id", "org.tizen.setting")
    print(json.dumps(get_package_info(pkg)))
