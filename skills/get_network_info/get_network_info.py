#!/usr/bin/env python3
import ctypes
import json
import os
import sys

sys.path.append(os.path.join(os.path.dirname(__file__), "..", "common"))
import tizen_capi_utils


def get_network_info():
    try:
        lib = tizen_capi_utils.load_library(
            ["libcapi-network-connection.so.1",
             "libcapi-network-connection.so.0"]
        )

        # int connection_create(connection_h *connection)
        lib.connection_create.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
        lib.connection_create.restype = ctypes.c_int

        # int connection_get_type(connection_h conn, connection_type_e *type)
        lib.connection_get_type.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_int)
        ]
        lib.connection_get_type.restype = ctypes.c_int

        # int connection_get_ip_address(connection_h conn,
        #   connection_address_family_e family, char **ip)
        lib.connection_get_ip_address.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.connection_get_ip_address.restype = ctypes.c_int

        # int connection_get_proxy(connection_h conn,
        #   connection_address_family_e family, char **proxy)
        lib.connection_get_proxy.argtypes = [
            ctypes.c_void_p, ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)
        ]
        lib.connection_get_proxy.restype = ctypes.c_int

        # int connection_destroy(connection_h connection)
        lib.connection_destroy.argtypes = [ctypes.c_void_p]
        lib.connection_destroy.restype = ctypes.c_int

        handle = ctypes.c_void_p()
        tizen_capi_utils.check_return(
            lib.connection_create(ctypes.byref(handle)),
            "Failed to create connection handle"
        )

        # Get connection type
        conn_type = ctypes.c_int()
        lib.connection_get_type(handle, ctypes.byref(conn_type))
        type_map = {
            0: "disconnected", 1: "wifi", 2: "cellular",
            3: "ethernet", 4: "bt", 5: "net_proxy",
        }

        # Get IP address (IPv4 = 0)
        ip_ptr = ctypes.c_char_p()
        lib.connection_get_ip_address(handle, 0, ctypes.byref(ip_ptr))
        ip = ip_ptr.value.decode("utf-8") if ip_ptr.value else ""

        # Get proxy
        proxy_ptr = ctypes.c_char_p()
        lib.connection_get_proxy(handle, 0, ctypes.byref(proxy_ptr))
        proxy = proxy_ptr.value.decode("utf-8") if proxy_ptr.value else ""

        lib.connection_destroy(handle)

        return {
            "connection_type": type_map.get(conn_type.value, "unknown"),
            "is_connected": conn_type.value != 0,
            "ip_address": ip,
            "proxy": proxy,
        }

    except Exception as e:
        return {"error": str(e)}


if __name__ == "__main__":
    print(json.dumps(get_network_info()))
