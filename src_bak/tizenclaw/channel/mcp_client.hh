#ifndef MCP_CLIENT_HH
#define MCP_CLIENT_HH

#include <json.hpp>
#include <string>
#include <vector>
#include <memory>
#include <mutex>
#include <atomic>

namespace tizenclaw {

class McpClient {
 public:
  struct ToolInfo {
    std::string name;
    std::string description;
    nlohmann::json input_schema;
  };

  // Constructor takes a server name, command, and arguments
  McpClient(const std::string& server_name, const std::string& command,
            const std::vector<std::string>& args, int timeout_ms = 10000);
  ~McpClient();

  // Start the MCP server process and perform initialize handshake
  bool Connect();

  // Disconnect and kill the process
  void Disconnect();

  // Retrieve the list of tools from this MCP server
  std::vector<ToolInfo> GetTools();

  // Call a tool on this MCP server
  nlohmann::json CallTool(const std::string& tool_name,
                          const nlohmann::json& arguments);

  const std::string& GetServerName() const { return server_name_; }
  bool IsConnected() const { return is_connected_; }

  // Update last_used
  void UpdateLastUsed();
  long long GetLastUsedMs() const;

  // Timeout setting
  int GetTimeoutMs() const { return timeout_ms_; }

 private:
  std::string server_name_;
  std::string command_;
  std::vector<std::string> args_;

  pid_t pid_ = -1;
  int pipe_stdin_[2] = {-1, -1};
  int pipe_stdout_[2] = {-1, -1};
  
  std::atomic<bool> is_connected_{false};
  std::mutex io_mutex_;

  std::atomic<int> next_req_id_{1};

  // Internal helper to send string to server via stdin pipe
  bool SendRpcMessage(const nlohmann::json& message);

  // Internal helper to read next '\n' delimited JSON line from stdout pipe
  nlohmann::json ReadRpcMessage(int timeout_ms = 5000);

  // Send a JSON-RPC request and wait for the response matching req_id
  nlohmann::json SendRequestSync(const std::string& method,
                                 const nlohmann::json& params,
                                 int timeout_ms);

  // Persistent read buffer for stdio pipe
  std::string read_buffer_;
  
  int timeout_ms_;
  std::atomic<long long> last_used_ms_{0};
};

}  // namespace tizenclaw

#endif  // MCP_CLIENT_HH
