#ifndef __CONTAINER_ENGINE_H__
#define __CONTAINER_ENGINE_H__

#include <string>
#include <memory>

class ContainerEngine {
public:
    ContainerEngine();
    ~ContainerEngine();

    // Initialize the container backend (crun or runc)
    bool Initialize();

    // Setup container rootfs, generate config.json, and execute a skill command
    // capturing the JSON output synchronously via popen
    std::string ExecuteSkill(const std::string& skill_name, const std::string& arg_str);

private:
    bool m_initialized;
    std::string m_runtime_bin;
};

#endif // __CONTAINER_ENGINE_H__
