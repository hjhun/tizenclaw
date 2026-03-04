#ifndef __CONTAINER_ENGINE_H__
#define __CONTAINER_ENGINE_H__

#include <string>
#include <memory>

class ContainerEngine {
public:
    ContainerEngine();
    ~ContainerEngine();

    // Initialize the runc backend
    bool Initialize();

    // Create and start a new container
    // Uses the given name and a base RootFS path
    bool StartContainer(const std::string& container_name, const std::string& rootfs_path);

    // Stop and destroy a container
    bool StopContainer(const std::string& container_name);

private:
    bool m_initialized;
};

#endif // __CONTAINER_ENGINE_H__
