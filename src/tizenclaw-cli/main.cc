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
/**
 * tizenclaw-cli: CLI tool for testing TizenClaw
 * daemon utilizing the libtizenclaw CAPI.
 *
 * Usage:
 *   tizenclaw-cli "What is the battery level?"
 *   tizenclaw-cli -s my_session "Run a skill"
 *   tizenclaw-cli --stream "Tell me about Tizen"
 *   tizenclaw-cli   (interactive mode)
 */

#include <iostream>
#include <string>

#include <nlohmann/json.hpp>

#include "interactive_shell.hh"
#include "request_handler.hh"
#include "response_printer.hh"
#include "socket_client.hh"
#include "tool_prober.hh"

namespace {

void PrintUsage() {
  std::cerr << "tizenclaw-cli — TizenClaw IPC test"
            << "\n\n"
            << "Usage:\n"
            << "  tizenclaw-cli [options] [prompt]"
            << "\n\n"
            << "Options:\n"
            << "  -s <id>       Session ID "
            << "(default: cli_test)\n"
            << "  --stream      Enable streaming\n"
            << "  --send-to <channel> <text>\n"
            << "                Send outbound "
            << "message via channel\n"
            << "  --list-agents List all running "
            << "agents\n"
            << "  --perception  Show perception "
            << "engine status\n"
             << "  --run-cli <tool> <args...>\n"
             << "                Run a CLI tool directly via tool executor\n"
             << "  --start-cli <tool> [args]\n"
             << "                Start a CLI session\n"
             << "  --send-cli <session_id> <input>\n"
             << "                Send input to a CLI session\n"
             << "  --read-cli <session_id>\n"
             << "                Read output from a CLI session\n"
             << "  --close-cli <session_id>\n"
             << "                Close a CLI session\n"
             << "  --list-cli    List active CLI sessions\n"
             << "  --register-tool <path>\n"
             << "                Register a system "
             << "CLI tool by probing its help\n"
             << "  --unregister-tool <name>\n"
             << "                Unregister a "
             << "system CLI tool\n"
             << "  --list-tools  List registered "
             << "system CLI tools\n"
             << "  --list-mcp    List active MCP "
             << "tools\n"
             << "  --connect-mcp <path>\n"
             << "                Connect to MCP "
             << "servers defined in JSON config\n"
             << "  -h, --help    Show this help\n\n"
             << "If no prompt given, interactive "
             << "mode.\n";
}

}  // namespace

int main(int argc, char* argv[]) {
  std::string session_id = "cli_test";
  bool stream = false;
  std::string prompt;

  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg == "-h" || arg == "--help") {
      PrintUsage();
      return 0;
    } else if (arg == "--send-to" &&
               i + 2 < argc) {
      std::string channel = argv[++i];
      std::string text;
      for (int j = ++i; j < argc; ++j) {
        if (!text.empty()) text += " ";
        text += argv[j];
      }
      tizenclaw::cli::SocketClient client;
      return client.SendToChannel(channel, text);
    } else if (arg == "--run-cli" &&
               i + 1 < argc) {
      std::string tool = argv[++i];
      nlohmann::json params;
      params["tool_name"] = tool;
      std::string args_str;
      for (int j = ++i; j < argc; ++j) {
        if (!args_str.empty()) args_str += " ";
        args_str += argv[j];
        ++i;
      }
      params["arguments"] = args_str;
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("execute_cli", params.dump());
      if (!resp.empty()) {
        std::cout << resp << "\n";
        return 0;
      }
      return 1;
    } else if (arg == "--start-cli" && i + 1 < argc) {
      std::string tool = argv[++i];
      nlohmann::json params;
      params["tool_name"] = tool;
      if (i + 1 < argc) {
          params["arguments"] = argv[++i];
      }
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("execute_cli_session", params.dump());
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--send-cli" && i + 2 < argc) {
      std::string sid = argv[++i];
      std::string input = argv[++i];
      nlohmann::json params;
      params["session_id"] = sid;
      params["input"] = input;
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("cli_session_send", params.dump());
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--read-cli" && i + 1 < argc) {
      std::string sid = argv[++i];
      nlohmann::json params;
      params["session_id"] = sid;
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("cli_session_read", params.dump());
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--close-cli" && i + 1 < argc) {
      std::string sid = argv[++i];
      nlohmann::json params;
      params["session_id"] = sid;
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("cli_session_close", params.dump());
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--list-cli") {
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendToExecutor("cli_session_list", "{}");
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--list-agents") {
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "list_agents");
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintAgentList(resp);
      return 0;
    } else if (arg == "--perception") {
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "get_perception_status");
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintPerceptionStatus(resp);
      return 0;
    } else if (arg == "--list-tools") {
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "list_system_cli");
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintToolList(resp);
      return 0;
    } else if (arg == "--list-mcp") {
      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "list_mcp_tools");
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintMcpToolList(resp);
      return 0;
    } else if (arg == "--connect-mcp" &&
               i + 1 < argc) {
      std::string config_path = argv[++i];
      nlohmann::json params;
      params["config_path"] = config_path;

      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "connect_mcp_servers",
          params.dump());
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      std::cout << resp << "\n";
      return 0;
    } else if (arg == "--register-tool" &&
               i + 1 < argc) {
      std::string path = argv[++i];
      auto probe =
          tizenclaw::cli::ToolProber::Probe(
              path);
      if (!probe.success) {
        std::cerr << "Probe failed: "
                  << probe.error << "\n";
        return 1;
      }

      // Build JSON params with escaping via
      // nlohmann::json to avoid manual escaping
      nlohmann::json params;
      params["name"] = probe.name;
      params["path"] = path;
      params["description"] = probe.description;
      params["side_effect"] = "reversible";
      params["timeout_seconds"] = 10;
      // Send raw help output for LLM-based doc
      // generation, with prober doc as fallback
      params["help_output"] = probe.help_output;
      params["tool_doc"] = probe.tool_doc;

      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "register_system_cli",
          params.dump());
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintToolResult(resp);
      return 0;
    } else if (arg == "--unregister-tool" &&
               i + 1 < argc) {
      std::string name = argv[++i];
      nlohmann::json params;
      params["name"] = name;

      tizenclaw::cli::SocketClient client;
      std::string resp = client.SendJsonRpc(
          "unregister_system_cli",
          params.dump());
      if (resp.empty()) {
        std::cerr << "Failed to read response\n";
        return 1;
      }
      tizenclaw::cli::ResponsePrinter
          ::PrintToolResult(resp);
      return 0;
    } else if (arg == "-s" && i + 1 < argc) {
      session_id = argv[++i];
    } else if (arg == "--stream") {
      stream = true;
    } else {
      for (int j = i; j < argc; ++j) {
        if (!prompt.empty()) prompt += " ";
        prompt += argv[j];
      }
      break;
    }
  }

  tizenclaw::cli::RequestHandler handler;
  if (!handler.Create()) return 1;

  // Single-shot mode
  if (!prompt.empty()) {
    std::string resp = handler.SendRequest(
        session_id, prompt, stream);
    if (!stream && !resp.empty()) {
      std::cout << resp << "\n";
    }
    return resp.empty() ? 1 : 0;
  }

  // Interactive mode
  tizenclaw::cli::InteractiveShell shell(handler);
  shell.Run(session_id, stream);
  return 0;
}
