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

#include "response_printer.hh"

#include <iostream>
#include <string>

#include <nlohmann/json.hpp>

namespace tizenclaw {
namespace cli {

void ResponsePrinter::PrintAgentList(
    const std::string& body) {
  try {
    auto j = nlohmann::json::parse(body);
    auto res = j.value("result",
                       nlohmann::json::object());

    // Configured roles
    if (res.contains("configured_roles")) {
      auto& roles = res["configured_roles"];
      std::cout << "=== Configured Roles ("
                << roles.size() << ") ===\n";
      for (auto& r : roles) {
        std::cout << "  - "
                  << r.value("name", "?")
                  << "  tools: ["
                  << r.value("allowed_tools",
                             nlohmann::json::array())
                         .dump()
                  << "]\n";
      }
    }

    // Dynamic agents
    if (res.contains("dynamic_agents") &&
        !res["dynamic_agents"].empty()) {
      auto& da = res["dynamic_agents"];
      std::cout << "\n=== Dynamic Agents ("
                << da.size() << ") ===\n";
      for (auto& a : da) {
        std::cout << "  - "
                  << a.value("name", "?") << "\n";
      }
    }

    // Active delegations
    if (res.contains("active_delegations")) {
      auto& del = res["active_delegations"];
      if (del.contains("active") &&
          !del["active"].empty()) {
        std::cout << "\n=== Active Delegations ("
                  << del["active"].size()
                  << ") ===\n";
        for (auto& d : del["active"]) {
          std::cout << "  - ["
                    << d.value("role", "?")
                    << "] " << d.value("task", "")
                    << " ("
                    << d.value("elapsed_sec", 0)
                    << "s)\n";
        }
      }
    }

    // Event bus sources
    if (res.contains("event_bus_sources") &&
        !res["event_bus_sources"].empty()) {
      auto& src = res["event_bus_sources"];
      std::cout << "\n=== Event Bus Sources ("
                << src.size() << ") ===\n";
      for (auto& s : src) {
        std::cout << "  - "
                  << s.value("name", "?")
                  << " (" << s.value("plugin_id", "")
                  << ")\n";
      }
    }

    // Autonomous trigger
    if (res.contains("autonomous_trigger")) {
      auto& at = res["autonomous_trigger"];
      std::cout << "\n=== Autonomous Trigger ==="
                << "\n  enabled: "
                << (at.value("enabled", false)
                        ? "yes" : "no")
                << "\n";
    }
  } catch (...) {
    // Fallback: raw JSON
    std::cout << body << "\n";
  }
}

void ResponsePrinter::PrintPerceptionStatus(
    const std::string& body) {
  try {
    auto j = nlohmann::json::parse(body);
    auto res = j.value("result",
                       nlohmann::json::object());

    // Engine status
    if (res.contains("engine")) {
      auto& e = res["engine"];
      std::cout << "=== Perception Engine ==="
                << "\n  Running: "
                << (e.value("running", false)
                        ? "yes" : "no")
                << "\n  Analysis interval: "
                << e.value(
                       "analysis_interval_sec", 0)
                << "s"
                << "\n  Events recorded: "
                << e.value("event_count", 0)
                << "\n";
    }

    // Situation assessment
    if (res.contains("situation")) {
      auto& sit = res["situation"];
      std::string level =
          sit.value("level", "unknown");
      std::string emoji = "✅";
      if (level == "advisory") emoji = "ℹ️ ";
      else if (level == "warning") emoji = "⚠️ ";
      else if (level == "critical") emoji = "🔴";

      std::cout << "\n=== Situation Assessment ==="
                << "\n  " << emoji << " Level: "
                << level
                << "\n  Risk Score: ";

      // Risk bar
      double risk =
          sit.value("risk_score", 0.0);
      int pct = static_cast<int>(risk * 100);
      int filled = pct / 5;
      std::cout << "[";
      for (int i = 0; i < 20; i++) {
        std::cout << (i < filled ? "█" : "░");
      }
      std::cout << "] " << pct << "%\n";

      std::cout << "  Summary: "
                << sit.value("summary", "")
                << "\n";

      if (sit.contains("factors") &&
          !sit["factors"].empty()) {
        std::cout << "\n  Risk Factors:\n";
        for (auto& f : sit["factors"]) {
          std::cout << "    • " << f << "\n";
        }
      }
      if (sit.contains("suggestions") &&
          !sit["suggestions"].empty()) {
        std::cout << "\n  Suggestions:\n";
        for (auto& sg : sit["suggestions"]) {
          std::cout << "    💡 " << sg << "\n";
        }
      }
    }

    // Device profile
    if (res.contains("profile")) {
      auto& p = res["profile"];
      std::cout << "\n=== Device Profile ==="
                << "\n  🔋 Battery: "
                << p.value("battery_level", -1)
                << "% ("
                << p.value("battery_health",
                           "unknown")
                << ")";
      if (p.value("charging", false)) {
        std::cout << " ⚡";
      }
      double drain = p.value(
          "battery_drain_rate", 0.0);
      if (drain > 0) {
        std::cout << "\n  📉 Drain rate: "
                  << drain << " %/min";
      }
      std::cout << "\n  🧠 Memory: "
                << p.value("memory_trend",
                           "unknown")
                << " ("
                << p.value(
                       "memory_warning_count", 0)
                << " warnings)"
                << "\n  🌐 Network: "
                << p.value("network_status",
                           "unknown")
                << " ("
                << p.value(
                       "network_drop_count", 0)
                << " drops)";

      auto fg = p.value("foreground_app", "");
      if (!fg.empty()) {
        std::cout << "\n  📱 Foreground: " << fg;
      }
      if (p.contains("top_apps") &&
          !p["top_apps"].empty()) {
        std::cout << "\n  📊 Top apps: ";
        bool first = true;
        for (auto& a : p["top_apps"]) {
          if (!first) std::cout << ", ";
          std::cout << a;
          first = false;
        }
      }
      std::cout << "\n";
    }

    // Anomalies
    if (res.contains("anomalies") &&
        !res["anomalies"].empty()) {
      std::cout << "\n=== Anomalies ===";
      for (auto& a : res["anomalies"]) {
        std::cout << "\n  ⚡ ["
                  << a.value("severity", "?")
                  << "] "
                  << a.value("type", "unknown")
                  << ": "
                  << a.value("detail", "");
      }
      std::cout << "\n";
    }

  } catch (...) {
    // Fallback: raw JSON
    std::cout << body << "\n";
  }
}

}  // namespace cli
}  // namespace tizenclaw

// Extend outside namespace block to avoid issues
namespace tizenclaw {
namespace cli {

void ResponsePrinter::PrintMcpToolList(
    const std::string& body) {
  try {
    auto j = nlohmann::json::parse(body);
    auto res = j.value("result",
                       nlohmann::json::object());
    bool enabled = res.value("enabled", false);
    int count = res.value("tool_count", 0);

    std::cout << "=== MCP Tools ===\n"
              << "  Enabled: "
              << (enabled ? "yes" : "no")
              << "\n  Connected Tools: " << count
              << "\n\n";

    if (res.contains("tools") &&
        res["tools"].is_array()) {
      for (const auto& t : res["tools"]) {
        std::string name =
            t.value("name", "?");
        std::string desc =
            t.value("description", "");

        std::cout << "  " << name << "\n"
                  << "    Desc: " << desc << "\n\n";
      }
    }
    if (count == 0) {
      std::cout
          << "  (no MCP tools currently connected)\n"
          << "  Use --connect-mcp <config_path> "
          << "to connect\n\n"
          << "  💡 [Reference] Popular Official MCP Servers:\n"
             "    - npx -y @modelcontextprotocol/server-github\n"
             "    - npx -y @modelcontextprotocol/server-postgres\n"
             "    - npx -y @modelcontextprotocol/server-slack\n"
             "    - npx -y @modelcontextprotocol/server-google-drive\n"
             "    - python3 -m mcp_server_sqlite\n"
             "    - python3 -m mcp_server_weather\n"
             "    - python3 -m mcp_server_fetch\n"
          << "  Register them in mcp_servers.json!\n";
    }
  } catch (...) {
    std::cout << body << "\n";
  }
}

void ResponsePrinter::PrintToolList(
    const std::string& body) {
  try {
    auto j = nlohmann::json::parse(body);
    auto res = j.value("result",
                       nlohmann::json::object());
    bool enabled = res.value("enabled", false);
    int count = res.value("tool_count", 0);

    std::cout << "=== System CLI Tools ===\n"
              << "  Enabled: "
              << (enabled ? "yes" : "no")
              << "\n  Registered: " << count
              << "\n\n";

    if (res.contains("tools") &&
        res["tools"].is_array()) {
      for (const auto& t : res["tools"]) {
        std::string name =
            t.value("name", "?");
        std::string path =
            t.value("path", "?");
        std::string desc =
            t.value("description", "");
        int timeout =
            t.value("timeout_seconds", 10);
        std::string se =
            t.value("side_effect", "none");
        bool doc = t.value("has_doc", false);

        std::cout << "  " << name << "\n"
                  << "    Path: " << path << "\n"
                  << "    Desc: " << desc << "\n"
                  << "    Timeout: " << timeout
                  << "s  Side-effect: " << se
                  << "  Doc: "
                  << (doc ? "yes" : "no")
                  << "\n\n";
      }
    }
    if (count == 0) {
      std::cout
          << "  (no tools registered)\n"
          << "  Use --register-tool <path> "
          << "to add tools\n";
    }
  } catch (...) {
    std::cout << body << "\n";
  }
}

void ResponsePrinter::PrintToolResult(
    const std::string& body) {
  try {
    auto j = nlohmann::json::parse(body);
    if (j.contains("result")) {
      auto res = j["result"];
      std::cout << res.value("message", "OK")
                << ": "
                << res.value("tool", "")
                << "\n";
    } else if (j.contains("error")) {
      auto err = j["error"];
      std::cerr << "Error: "
                << err.value("message",
                             "Unknown error")
                << "\n";
    } else {
      std::cout << body << "\n";
    }
  } catch (...) {
    std::cout << body << "\n";
  }
}

}  // namespace cli
}  // namespace tizenclaw
