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

json VConfValueToJson(const char* key) {
    keynode_t* node = vconf_get_keynode(key);
    if (!node) return nullptr;

    int type = vconf_keynode_get_type(node);
    json j;
    j["key"] = key;

    switch (type) {
        case VCONF_TYPE_INT: {
            int val = 0;
            vconf_get_int(key, &val);
            j["type"] = "int";
            j["value"] = val;
            break;
        }
        case VCONF_TYPE_BOOL: {
            int val = 0;
            vconf_get_bool(key, &val);
            j["type"] = "bool";
            j["value"] = (bool)val;
            break;
        }
        case VCONF_TYPE_DOUBLE: {
            double val = 0;
            vconf_get_double(key, &val);
            j["type"] = "double";
            j["value"] = val;
            break;
        }
        case VCONF_TYPE_STRING: {
            char* val = vconf_get_str(key);
            j["type"] = "string";
            j["value"] = val ? std::string(val) : "";
            if (val) free(val);
            break;
        }
        default:
            j["type"] = "unknown";
            j["value"] = nullptr;
            break;
    }
    vconf_keynode_destroy(node);
    return j;
}

void OnKeyChanged(const char* key, void* user_data) {
    json j = VConfValueToJson(key);
    j["event"] = "changed";
    std::cout << j.dump() << std::endl;
}

volatile bool g_keep_running = true;
void SignalHandler(int) {
    g_keep_running = false;
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
        keynode_t* node = vconf_get_keynode(key.c_str());
        int type = node ? vconf_keynode_get_type(node) : VCONF_TYPE_STRING;
        if (node) vconf_keynode_destroy(node);

        int ret = -1;
        if (type == VCONF_TYPE_INT) {
            ret = vconf_set_int(key.c_str(), std::stoi(val_str));
        } else if (type == VCONF_TYPE_BOOL) {
            bool b = (val_str == "true" || val_str == "1");
            ret = vconf_set_bool(key.c_str(), b);
        } else if (type == VCONF_TYPE_DOUBLE) {
            ret = vconf_set_double(key.c_str(), std::stod(val_str));
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
        OnKeyChanged(key.c_str(), nullptr);

        while (g_keep_running) {
            pause(); // Wait for signals or callbacks
        }
        
        vconf_ignore_key_changed(key.c_str(), OnKeyChanged);
    } else {
        PrintUsage();
        return 1;
    }

    return 0;
}
