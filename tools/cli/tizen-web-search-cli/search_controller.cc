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

#include "search_controller.hh"

#include "http_client.hh"

#include <curl/curl.h>

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace tizenclaw {
namespace cli {

namespace {

constexpr const char* kConfigPath =
    "/opt/usr/share/tizenclaw/config"
    "/web_search_config.json";

constexpr const char* kSupportedEngines[] = {
    "naver", "google", "brave",
    "gemini", "grok", "kimi", "perplexity"};

// --- Minimal JSON helpers (no external lib) ---

std::string JsonStr(const std::string& raw,
                    const std::string& key) {
  std::string pat = "\"" + key + "\"";
  auto pos = raw.find(pat);
  if (pos == std::string::npos)
    return "";

  pos = raw.find(':', pos + pat.size());
  if (pos == std::string::npos)
    return "";

  pos = raw.find('"', pos + 1);
  if (pos == std::string::npos)
    return "";

  auto end = pos + 1;
  while (end < raw.size()) {
    if (raw[end] == '\\') {
      end += 2;
      continue;
    }
    if (raw[end] == '"')
      break;
    ++end;
  }

  return raw.substr(pos + 1, end - pos - 1);
}

std::string UrlEncode(const std::string& s) {
  CURL* curl = curl_easy_init();
  if (!curl)
    return s;

  char* encoded = curl_easy_escape(
      curl, s.c_str(),
      static_cast<int>(s.size()));
  std::string result = encoded ? encoded : s;
  if (encoded)
    curl_free(encoded);

  curl_easy_cleanup(curl);
  return result;
}

std::string EscapeJson(const std::string& s) {
  std::string r;
  r.reserve(s.size());
  for (char c : s) {
    switch (c) {
      case '"':  r += "\\\""; break;
      case '\\': r += "\\\\"; break;
      case '\n': r += "\\n"; break;
      case '\r': r += "\\r"; break;
      case '\t': r += "\\t"; break;
      default:   r += c; break;
    }
  }
  return r;
}

std::string StripHtml(const std::string& s) {
  std::string r;
  r.reserve(s.size());
  bool in_tag = false;
  for (char c : s) {
    if (c == '<') {
      in_tag = true;
    } else if (c == '>') {
      in_tag = false;
    } else if (!in_tag) {
      r += c;
    }
  }
  return r;
}

std::string ReadFile(const std::string& path) {
  std::ifstream f(path);
  if (!f.is_open())
    return "";

  std::ostringstream ss;
  ss << f.rdbuf();
  return ss.str();
}

// --- Config loading ---

struct EngineConfig {
  std::string api_key;
  std::string client_id;
  std::string client_secret;
  std::string search_engine_id;
  std::string model;
  std::string base_url;
};

struct Config {
  std::string default_engine = "naver";
  std::map<std::string, EngineConfig> engines;
};

Config LoadConfig() {
  Config cfg;
  std::string raw = ReadFile(kConfigPath);
  if (raw.empty())
    return cfg;

  cfg.default_engine =
      JsonStr(raw, "default_engine");
  if (cfg.default_engine.empty())
    cfg.default_engine = "naver";

  // Naver
  EngineConfig naver;
  naver.client_id = JsonStr(raw, "client_id");
  naver.client_secret =
      JsonStr(raw, "client_secret");
  cfg.engines["naver"] = naver;

  // Google
  EngineConfig google;
  google.api_key = JsonStr(raw, "api_key");
  google.search_engine_id =
      JsonStr(raw, "search_engine_id");
  cfg.engines["google"] = google;

  // Brave/Gemini/Grok/Kimi/Perplexity
  for (const auto& name : {"brave", "gemini",
                           "grok", "kimi",
                           "perplexity"}) {
    EngineConfig ec;
    // Find engine-specific section
    std::string sec =
        "\"" + std::string(name) + "\"";
    auto pos = raw.find(sec);
    if (pos != std::string::npos) {
      auto brace = raw.find('{', pos);
      if (brace != std::string::npos) {
        auto end = raw.find('}', brace);
        if (end != std::string::npos) {
          std::string sub =
              raw.substr(brace, end - brace + 1);
          ec.api_key = JsonStr(sub, "api_key");
          ec.model = JsonStr(sub, "model");
          ec.base_url = JsonStr(sub, "base_url");
        }
      }
    }
    cfg.engines[name] = ec;
  }

  return cfg;
}

// --- Search engines ---

std::string SearchNaver(const std::string& query,
                        const Config& cfg) {
  auto it = cfg.engines.find("naver");
  if (it == cfg.engines.end() ||
      it->second.client_id.empty()) {
    return "{\"error\": "
           "\"Naver credentials not configured\"}";
  }

  std::string url =
      "https://openapi.naver.com/v1/search/"
      "webkr.json?query=" +
      UrlEncode(query) + "&display=5";

  HttpClient http;
  auto resp = http.Get(url,
      "X-Naver-Client-Id: " +
      it->second.client_id + "\n"
      "X-Naver-Client-Secret: " +
      it->second.client_secret);

  if (!resp.error.empty()) {
    return "{\"error\": \"Naver API: " +
           EscapeJson(resp.error) + "\"}";
  }

  // Parse items from response
  std::string r =
      "{\"engine\": \"naver\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"results\": [";

  std::string body = resp.body;
  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = body.find("\"title\"", pos);
    if (pos == std::string::npos)
      break;

    auto item_start = body.rfind('{', pos);
    auto item_end = body.find('}', pos);
    if (item_start == std::string::npos ||
        item_end == std::string::npos)
      break;

    std::string item =
        body.substr(item_start,
                    item_end - item_start + 1);
    std::string title =
        StripHtml(JsonStr(item, "title"));
    std::string snippet =
        StripHtml(JsonStr(item, "description"));
    std::string link = JsonStr(item, "link");

    if (count > 0)
      r += ", ";

    r += "{\"title\": \"" + EscapeJson(title) +
         "\", \"snippet\": \"" +
         EscapeJson(snippet) +
         "\", \"url\": \"" + EscapeJson(link) +
         "\"}";
    count++;
    pos = item_end + 1;
  }

  r += "]}";
  return r;
}

std::string SearchGoogle(const std::string& query,
                         const Config& cfg) {
  auto it = cfg.engines.find("google");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Google credentials not configured\"}";
  }

  std::string url =
      "https://www.googleapis.com/customsearch/"
      "v1?q=" + UrlEncode(query) +
      "&key=" + it->second.api_key +
      "&cx=" + it->second.search_engine_id +
      "&num=5";

  HttpClient http;
  auto resp = http.Get(url);

  if (!resp.error.empty()) {
    return "{\"error\": \"Google API: " +
           EscapeJson(resp.error) + "\"}";
  }

  std::string r =
      "{\"engine\": \"google\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"results\": [";

  std::string body = resp.body;
  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = body.find("\"title\"", pos);
    if (pos == std::string::npos)
      break;

    auto item_start = body.rfind('{', pos);
    auto item_end = body.find('}', pos);
    if (item_start == std::string::npos ||
        item_end == std::string::npos)
      break;

    std::string item =
        body.substr(item_start,
                    item_end - item_start + 1);
    std::string title = JsonStr(item, "title");
    std::string snippet = JsonStr(item, "snippet");
    std::string link = JsonStr(item, "link");

    if (count > 0)
      r += ", ";

    r += "{\"title\": \"" + EscapeJson(title) +
         "\", \"snippet\": \"" +
         EscapeJson(snippet) +
         "\", \"url\": \"" + EscapeJson(link) +
         "\"}";
    count++;
    pos = item_end + 1;
  }

  r += "]}";
  return r;
}

std::string SearchBrave(const std::string& query,
                        const Config& cfg) {
  auto it = cfg.engines.find("brave");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Brave API key not configured\"}";
  }

  std::string url =
      "https://api.search.brave.com/res/v1/"
      "web/search?q=" + UrlEncode(query) +
      "&count=5";

  HttpClient http;
  auto resp = http.Get(url,
      "Accept: application/json\n"
      "X-Subscription-Token: " +
      it->second.api_key);

  if (!resp.error.empty()) {
    return "{\"error\": \"Brave API: " +
           EscapeJson(resp.error) + "\"}";
  }

  std::string r =
      "{\"engine\": \"brave\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"results\": [";

  std::string body = resp.body;
  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = body.find("\"title\"", pos);
    if (pos == std::string::npos)
      break;

    auto item_start = body.rfind('{', pos);
    auto item_end = body.find('}', pos);
    if (item_start == std::string::npos ||
        item_end == std::string::npos)
      break;

    std::string item =
        body.substr(item_start,
                    item_end - item_start + 1);
    std::string title = JsonStr(item, "title");
    std::string snippet =
        JsonStr(item, "description");
    std::string link = JsonStr(item, "url");

    if (count > 0)
      r += ", ";

    r += "{\"title\": \"" + EscapeJson(title) +
         "\", \"snippet\": \"" +
         EscapeJson(snippet) +
         "\", \"url\": \"" + EscapeJson(link) +
         "\"}";
    count++;
    pos = item_end + 1;
  }

  r += "]}";
  return r;
}

std::string SearchGemini(const std::string& query,
                         const Config& cfg) {
  auto it = cfg.engines.find("gemini");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Gemini API key not configured\"}";
  }

  std::string model = it->second.model;
  if (model.empty())
    model = "gemini-2.5-flash";

  std::string url =
      "https://generativelanguage.googleapis.com"
      "/v1beta/models/" + model +
      ":generateContent?key=" +
      it->second.api_key;

  std::string body =
      "{\"contents\": [{\"parts\": [{\"text\": \"" +
      EscapeJson(query) +
      "\"}]}], \"tools\": [{\"google_search\": {}}]}";

  HttpClient http;
  auto resp = http.Post(url, body);

  if (!resp.error.empty()) {
    return "{\"error\": \"Gemini API: " +
           EscapeJson(resp.error) + "\"}";
  }

  // Extract text from first candidate
  std::string text;
  auto text_pos = resp.body.find("\"text\"");
  if (text_pos != std::string::npos)
    text = JsonStr(resp.body.substr(text_pos - 1),
                   "text");

  if (text.empty())
    text = "No response";

  // Extract grounding URIs
  std::string r =
      "{\"engine\": \"gemini\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"content\": \"" + EscapeJson(text) +
      "\", \"results\": [";

  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = resp.body.find("\"uri\"", pos);
    if (pos == std::string::npos)
      break;

    std::string uri = JsonStr(
        resp.body.substr(pos - 1), "uri");
    if (!uri.empty()) {
      if (count > 0)
        r += ", ";

      std::string title = JsonStr(
          resp.body.substr(pos - 50 > 0
              ? pos - 50 : 0), "title");
      r += "{\"title\": \"" +
           EscapeJson(title) +
           "\", \"snippet\": \"\", "
           "\"url\": \"" + EscapeJson(uri) +
           "\"}";
      count++;
    }

    pos += 5;
  }

  r += "]}";
  return r;
}

std::string SearchGrok(const std::string& query,
                       const Config& cfg) {
  auto it = cfg.engines.find("grok");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Grok API key not configured\"}";
  }

  std::string model = it->second.model;
  if (model.empty())
    model = "grok-4-1-fast";

  std::string body =
      "{\"model\": \"" + model + "\", "
      "\"input\": [{\"role\": \"user\", "
      "\"content\": \"" + EscapeJson(query) +
      "\"}], "
      "\"tools\": [{\"type\": \"web_search\"}]}";

  HttpClient http;
  auto resp = http.Post(
      "https://api.x.ai/v1/responses", body,
      "Authorization: Bearer " +
      it->second.api_key);

  if (!resp.error.empty()) {
    return "{\"error\": \"Grok API: " +
           EscapeJson(resp.error) + "\"}";
  }

  std::string text;
  auto text_pos = resp.body.find("\"text\"");
  if (text_pos != std::string::npos)
    text = JsonStr(resp.body.substr(text_pos - 1),
                   "text");

  if (text.empty())
    text = "No response";

  // Extract URL citations
  std::string r =
      "{\"engine\": \"grok\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"content\": \"" + EscapeJson(text) +
      "\", \"results\": [";

  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = resp.body.find("\"url_citation\"", pos);
    if (pos == std::string::npos)
      break;

    auto url_pos = resp.body.find("\"url\"",
                                  pos + 14);
    if (url_pos != std::string::npos) {
      std::string url = JsonStr(
          resp.body.substr(url_pos - 1), "url");
      if (!url.empty()) {
        if (count > 0)
          r += ", ";

        r += "{\"title\": \"\", "
             "\"snippet\": \"\", "
             "\"url\": \"" + EscapeJson(url) +
             "\"}";
        count++;
      }
    }

    pos += 14;
  }

  r += "]}";
  return r;
}

std::string SearchPerplexity(
    const std::string& query,
    const Config& cfg) {
  auto it = cfg.engines.find("perplexity");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Perplexity API key not configured\"}";
  }

  std::string base = it->second.base_url;
  if (base.empty())
    base = "https://api.perplexity.ai";

  std::string model = it->second.model;
  if (model.empty())
    model = "sonar-pro";

  std::string body =
      "{\"model\": \"" + model + "\", "
      "\"messages\": [{\"role\": \"user\", "
      "\"content\": \"" + EscapeJson(query) +
      "\"}]}";

  HttpClient http;
  auto resp = http.Post(
      base + "/chat/completions", body,
      "Authorization: Bearer " +
      it->second.api_key);

  if (!resp.error.empty()) {
    return "{\"error\": \"Perplexity API: " +
           EscapeJson(resp.error) + "\"}";
  }

  std::string content;
  auto content_pos =
      resp.body.find("\"content\"");
  if (content_pos != std::string::npos) {
    content = JsonStr(
        resp.body.substr(content_pos - 1),
        "content");
  }

  if (content.empty())
    content = "No response";

  // Extract citations
  std::string r =
      "{\"engine\": \"perplexity\", "
      "\"query\": \"" + EscapeJson(query) +
      "\", \"content\": \"" +
      EscapeJson(content) +
      "\", \"results\": [";

  int count = 0;
  size_t pos = 0;
  while (count < 5) {
    pos = resp.body.find("\"url_citation\"", pos);
    if (pos == std::string::npos)
      break;

    auto url_pos = resp.body.find("\"url\"",
                                  pos + 14);
    if (url_pos != std::string::npos) {
      std::string url = JsonStr(
          resp.body.substr(url_pos - 1), "url");
      if (!url.empty()) {
        if (count > 0)
          r += ", ";

        r += "{\"title\": \"\", "
             "\"snippet\": \"\", "
             "\"url\": \"" + EscapeJson(url) +
             "\"}";
        count++;
      }
    }

    pos += 14;
  }

  r += "]}";
  return r;
}

// Kimi (simplified — single-round)
std::string SearchKimi(const std::string& query,
                       const Config& cfg) {
  auto it = cfg.engines.find("kimi");
  if (it == cfg.engines.end() ||
      it->second.api_key.empty()) {
    return "{\"error\": "
           "\"Kimi API key not configured\"}";
  }

  std::string base = it->second.base_url;
  if (base.empty())
    base = "https://api.moonshot.ai/v1";

  std::string model = it->second.model;
  if (model.empty())
    model = "moonshot-v1-128k";

  std::string body =
      "{\"model\": \"" + model + "\", "
      "\"messages\": [{\"role\": \"user\", "
      "\"content\": \"" + EscapeJson(query) +
      "\"}], "
      "\"tools\": [{\"type\": "
      "\"builtin_function\", "
      "\"function\": "
      "{\"name\": \"$web_search\"}}]}";

  HttpClient http;
  auto resp = http.Post(
      base + "/chat/completions", body,
      "Authorization: Bearer " +
      it->second.api_key);

  if (!resp.error.empty()) {
    return "{\"error\": \"Kimi API: " +
           EscapeJson(resp.error) + "\"}";
  }

  std::string content;
  auto content_pos =
      resp.body.find("\"content\"");
  if (content_pos != std::string::npos) {
    content = JsonStr(
        resp.body.substr(content_pos - 1),
        "content");
  }

  if (content.empty())
    content = "No response";

  return "{\"engine\": \"kimi\", "
         "\"query\": \"" + EscapeJson(query) +
         "\", \"content\": \"" +
         EscapeJson(content) +
         "\", \"results\": []}";
}

using SearchFunc = std::string (*)(
    const std::string&, const Config&);

const std::map<std::string, SearchFunc> kEngines = {
    {"naver",      SearchNaver},
    {"google",     SearchGoogle},
    {"brave",      SearchBrave},
    {"gemini",     SearchGemini},
    {"grok",       SearchGrok},
    {"kimi",       SearchKimi},
    {"perplexity", SearchPerplexity},
};

}  // namespace

std::string SearchController::Search(
    const std::string& query,
    const std::string& engine) const {
  Config cfg = LoadConfig();

  std::string eng = engine;
  if (eng.empty())
    eng = cfg.default_engine;

  auto it = kEngines.find(eng);
  if (it == kEngines.end()) {
    std::string supported;
    for (const auto& name : kSupportedEngines) {
      if (!supported.empty())
        supported += ", ";
      supported += name;
    }

    return "{\"error\": \"Unknown engine: " +
           eng + ". Supported: " + supported +
           "\"}";
  }

  return it->second(query, cfg);
}

}  // namespace cli
}  // namespace tizenclaw
