#include <dlog.h>
#include <curl/curl.h>
#include <fstream>
#include <iostream>

#include "agent_core.h"

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_AgentCore"

AgentCore::AgentCore() : m_container(new ContainerEngine()), m_initialized(false) {
    // Constructor
}

AgentCore::~AgentCore() {
    Shutdown();
}

bool AgentCore::Initialize() {
    if (m_initialized) return true;

    dlog_print(DLOG_INFO, LOG_TAG, "AgentCore Initializing...");
    
    if (!m_container->Initialize()) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to initialize LXC Container Engine");
        return false;
    }

    std::ifstream key_file("/usr/apps/org.tizen.tizenclaw/data/gemini_api_key.txt");
    if (key_file.is_open()) {
        std::getline(key_file, m_gemini_api_key);
        dlog_print(DLOG_INFO, LOG_TAG, "Loaded Gemini API Key (Length: %zu)", m_gemini_api_key.length());
        key_file.close();
    } else {
        dlog_print(DLOG_ERROR, LOG_TAG, "Gemini API key file not found: /usr/apps/org.tizen.tizenclaw/data/gemini_api_key.txt");
    }

    curl_global_init(CURL_GLOBAL_DEFAULT);

    m_initialized = true;
    return true;
}

void AgentCore::Shutdown() {
    if (!m_initialized) return;

    dlog_print(DLOG_INFO, LOG_TAG, "AgentCore Shutting down...");
    
    m_container.reset();
    curl_global_cleanup();
    
    m_initialized = false;
}

void AgentCore::ProcessPrompt(const std::string& prompt) {
    if (!m_initialized) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Cannot process prompt. AgentCore not initialized.");
        return;
    }

    dlog_print(DLOG_INFO, LOG_TAG, "AgentCore received prompt: %s", prompt.c_str());

    std::string gemini_response = QueryGemini(prompt);
    
    try {
        auto json_res = nlohmann::json::parse(gemini_response);
        if (json_res.contains("candidates") && !json_res["candidates"].empty()) {
            auto parts = json_res["candidates"][0]["content"]["parts"];
            for (auto& part : parts) {
                if (part.contains("functionCall")) {
                    std::string skill_name = part["functionCall"]["name"];
                    auto args = part["functionCall"]["args"];
                    dlog_print(DLOG_INFO, LOG_TAG, "Gemini requested function: %s", skill_name.c_str());
                    ExecuteSkill(skill_name, args);
                } else if (part.contains("text")) {
                    std::string text_reply = part["text"];
                    dlog_print(DLOG_INFO, LOG_TAG, "Gemini Text Reply: %s", text_reply.c_str());
                }
            }
        } else {
            dlog_print(DLOG_ERROR, LOG_TAG, "Gemini API returned an error or empty candidates.");
        }
    } catch (const std::exception& e) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to parse Gemini JSON: %s", e.what());
    }
}

static size_t WriteCallback(void *contents, size_t size, size_t nmemb, void *userp) {
    ((std::string*)userp)->append((char*)contents, size * nmemb);
    return size * nmemb;
}

std::string AgentCore::QueryGemini(const std::string& prompt_text) {
    if (m_gemini_api_key.empty()) {
        dlog_print(DLOG_ERROR, LOG_TAG, "API Key is empty! Please create /usr/apps/org.tizen.tizenclaw/data/gemini_api_key.txt");
        return "{}";
    }
    
    nlohmann::json payload = {
        {"contents", {{
            {"parts", {{{"text", prompt_text}}}}
        }}},
        {"tools", {{
            {"functionDeclarations", {
                {
                    {"name", "launch_app"},
                    {"description", "Launch a Tizen app using app ID"},
                    {"parameters", {
                        {"type", "object"},
                        {"properties", {
                            {"app_id", {{"type", "string"}, {"description", "The Tizen application ID, e.g., org.tizen.browser"}}}
                        }},
                        {"required", {"app_id"}}
                    }}
                },
                {
                    {"name", "vibrate_device"},
                    {"description", "Trigger haptic vibration feedback"},
                    {"parameters", {
                        {"type", "object"},
                        {"properties", {
                            {"duration_ms", {{"type", "integer"}, {"description", "Vibration duration in milliseconds"}}}
                        }}
                    }}
                },
                {
                    {"name", "schedule_alarm"},
                    {"description", "Schedule an alarm or reminder"},
                    {"parameters", {
                        {"type", "object"},
                        {"properties", {
                            {"delay_sec", {{"type", "integer"}, {"description", "Delay in seconds (must be >= 600)"}}},
                            {"prompt_text", {{"type", "string"}, {"description", "Prompt to send when alarm fires"}}}
                        }},
                        {"required", {"delay_sec", "prompt_text"}}
                    }}
                }
            }}
        }}}
    };
    
    std::string url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key=" + m_gemini_api_key;
    std::string response_string;
    
    CURL *curl = curl_easy_init();
    if (curl) {
        curl_easy_setopt(curl, CURLOPT_URL, url.c_str());
        struct curl_slist *headers = NULL;
        headers = curl_slist_append(headers, "Content-Type: application/json");
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);

        std::string json_str = payload.dump();
        curl_easy_setopt(curl, CURLOPT_POSTFIELDS, json_str.c_str());
        
        curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteCallback);
        curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_string);
        curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 0L); // Bypass for emulator testing

        CURLcode res = curl_easy_perform(curl);
        if (res != CURLE_OK) {
            dlog_print(DLOG_ERROR, LOG_TAG, "curl_easy_perform() failed: %s", curl_easy_strerror(res));
        }
        curl_slist_free_all(headers);
        curl_easy_cleanup(curl);
    }
    return response_string;
}

bool AgentCore::ExecuteSkill(const std::string& skill_name, const nlohmann::json& args) {
    dlog_print(DLOG_INFO, LOG_TAG, "Executing skill logic: %s", skill_name.c_str());
    
    // Launching the predefined container environment for Skills execution
    m_container->StartContainer("tizenclaw_skill_vm", "/usr/apps/org.tizen.tizenclaw/data/rootfs.tar.gz");
    
    std::string arg_str = "";
    if (skill_name == "launch_app") {
        arg_str = args.value("app_id", "");
    } else if (skill_name == "schedule_alarm") {
        int delay = args.value("delay_sec", 600);
        std::string text = args.value("prompt_text", "reminder");
        arg_str = std::to_string(delay) + " '" + text + "'";
    } else if (skill_name == "vibrate_device") {
        int duration = args.value("duration_ms", 1000);
        arg_str = std::to_string(duration);
    }
    
    std::string skill_file = "/usr/apps/org.tizen.tizenclaw/data/skills/" + skill_name + "/" + skill_name + ".py";
    // Setup execution using the system Python interpreter (or rootfs one)
    std::string cmd = "python3 " + skill_file + " " + arg_str;
    
    dlog_print(DLOG_INFO, LOG_TAG, "Running Skill CMD: %s", cmd.c_str());
    int res = std::system(cmd.c_str());
    
    if (res != 0) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Skill execution failed with code %d", res);
        return false;
    }
    return true;
}
