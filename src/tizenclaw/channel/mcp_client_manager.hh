#ifndef MCP_CLIENT_MANAGER_HH
#define MCP_CLIENT_MANAGER_HH

#include "mcp_client.hh"
#include "../llm/llm_backend.hh" // For LlmToolDecl
#include <map>
#include <string>
#include <vector>
#include <memory>
#include <mutex>

namespace tizenclaw {

class McpClientManager {
 public:
  McpClientManager();
  ~McpClientManager();

  // Load configured MCP servers from JSON file and attempt to connect
  bool LoadConfigAndConnect(const std::string& config_path);

  // Retrieve all tools from all currently connected MCP servers
  // The prefix used will be mcp__serverName__toolName
  std::vector<LlmToolDecl> GetToolDeclarations();

  // Execute an MCP tool (name formatted as mcp__serverName__toolName)
  std::string ExecuteTool(const std::string& full_tool_name,
                          const nlohmann::json& args);

  // Check if a tool belongs to an MCP client based on prefix
  static bool IsMcpTool(const std::string& full_tool_name);

 private:
  std::map<std::string, std::shared_ptr<McpClient>> clients_;
  std::mutex clients_mutex_;

  // Parses prefix "mcp__" to extract server name and actual tool name
  bool ParseToolName(const std::string& full_tool_name,
                     std::string& out_server_name,
                     std::string& out_actual_tool_name);
};

}  // namespace tizenclaw

#endif  // MCP_CLIENT_MANAGER_HH
