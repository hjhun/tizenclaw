/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include <vconf.h>
#include <glib.h>
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>
#include <json.hpp>
#include <unistd.h>
#include <signal.h>

using json = nlohmann::json;

namespace {

void PrintUsage() {
    std::cerr << "Usage: tizen-vconf-cli <command> [args]\n"
              << "Commands:\n"
              << "  get <key>           Get vconf value\n"
              << "  set <key> <value>   Set vconf value (type auto-detected if possible)\n"
              << "  watch <key>         Monitor vconf value changes\n";
}

json KeyNodeToJson(keynode_t* node) {
    if (!node) return nullptr;

    const char* key = vconf_keynode_get_name(node);
    int type = vconf_keynode_get_type(node);
    json j;
    j["key"] = key ? key : "unknown";

    switch (type) {
        case VCONF_TYPE_INT: {
            j["type"] = "int";
            j["value"] = vconf_keynode_get_int(node);
            break;
        }
        case VCONF_TYPE_BOOL: {
            j["type"] = "bool";
            j["value"] = (bool)vconf_keynode_get_bool(node);
            break;
        }
        case VCONF_TYPE_DOUBLE: {
            j["type"] = "double";
            j["value"] = vconf_keynode_get_dbl(node);
            break;
        }
        case VCONF_TYPE_STRING: {
            const char* val = vconf_keynode_get_str(node);
            j["type"] = "string";
            j["value"] = val ? std::string(val) : "";
            break;
        }
        default:
            j["type"] = "unknown";
            j["value"] = nullptr;
            break;
    }
    return j;
}

int DetectVconfType(const char* key) {
    int int_val;
    if (vconf_get_int(key, &int_val) == VCONF_OK) return VCONF_TYPE_INT;
    int bool_val;
    if (vconf_get_bool(key, &bool_val) == VCONF_OK) return VCONF_TYPE_BOOL;
    double dbl_val;
    if (vconf_get_dbl(key, &dbl_val) == VCONF_OK) return VCONF_TYPE_DOUBLE;
    char* str_val = vconf_get_str(key);
    if (str_val) { free(str_val); return VCONF_TYPE_STRING; }
    return -1;
}

json VConfValueToJson(const char* key) {
    json j;
    j["key"] = key;

    int int_val;
    if (vconf_get_int(key, &int_val) == VCONF_OK) {
        j["type"] = "int";
        j["value"] = int_val;
        return j;
    }
    int bool_val;
    if (vconf_get_bool(key, &bool_val) == VCONF_OK) {
        j["type"] = "bool";
        j["value"] = (bool)bool_val;
        return j;
    }
    double dbl_val;
    if (vconf_get_dbl(key, &dbl_val) == VCONF_OK) {
        j["type"] = "double";
        j["value"] = dbl_val;
        return j;
    }
    char* str_val = vconf_get_str(key);
    if (str_val) {
        j["type"] = "string";
        j["value"] = std::string(str_val);
        free(str_val);
        return j;
    }
    return nullptr;
}

void OnKeyChanged(keynode_t* node, void* user_data) {
    json j = KeyNodeToJson(node);
    j["event"] = "changed";
    std::cout << j.dump() << std::endl;
}

GMainLoop* g_loop = nullptr;
void SignalHandler(int) {
    if (g_loop) g_main_loop_quit(g_loop);
}

} // namespace

int main(int argc, char* argv[]) {
    if (argc < 3) {
        PrintUsage();
        return 1;
    }

    std::string cmd = argv[1];
    std::string key = argv[2];

    if (cmd == "get") {
        json j = VConfValueToJson(key.c_str());
        if (j.is_null()) {
            std::cerr << "Key not found or error" << std::endl;
            return 1;
        }
        std::cout << j.dump() << std::endl;
    } else if (cmd == "set") {
        if (argc < 4) {
            std::cerr << "Value required for set" << std::endl;
            return 1;
        }
        std::string val_str = argv[3];
        
        // Try to detect type from existing key
        int type = DetectVconfType(key.c_str());
        if (type < 0) type = VCONF_TYPE_STRING;

        int ret = -1;
        if (type == VCONF_TYPE_INT) {
            ret = vconf_set_int(key.c_str(), std::stoi(val_str));
        } else if (type == VCONF_TYPE_BOOL) {
            bool b = (val_str == "true" || val_str == "1");
            ret = vconf_set_bool(key.c_str(), b);
        } else if (type == VCONF_TYPE_DOUBLE) {
            ret = vconf_set_dbl(key.c_str(), std::stod(val_str));
        } else {
            ret = vconf_set_str(key.c_str(), val_str.c_str());
        }

        if (ret == VCONF_OK) {
            std::cout << "{\"status\":\"ok\"}" << std::endl;
        } else {
            std::cerr << "Failed to set value: " << ret << std::endl;
            return 1;
        }
    } else if (cmd == "watch") {
        signal(SIGINT, SignalHandler);
        signal(SIGTERM, SignalHandler);

        if (vconf_notify_key_changed(key.c_str(), OnKeyChanged, nullptr) != VCONF_OK) {
            std::cerr << "Failed to register notification" << std::endl;
            return 1;
        }

        // Initial state
        json init_j = VConfValueToJson(key.c_str());
        if (!init_j.is_null()) {
            init_j["event"] = "initial";
            std::cout << init_j.dump() << std::endl;
        }

        g_loop = g_main_loop_new(nullptr, FALSE);
        g_main_loop_run(g_loop);
        g_main_loop_unref(g_loop);
        g_loop = nullptr;
        
        vconf_ignore_key_changed(key.c_str(), OnKeyChanged);
    } else {
        PrintUsage();
        return 1;
    }

    return 0;
}
