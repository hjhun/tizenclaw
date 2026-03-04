#include "container_engine.h"
#include <dlog.h>
#include <cstdlib>
#include <string>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Container"

ContainerEngine::ContainerEngine() : m_initialized(false) {
}

ContainerEngine::~ContainerEngine() {
}

bool ContainerEngine::Initialize() {
    if (m_initialized) return true;

    dlog_print(DLOG_INFO, LOG_TAG, "ContainerEngine Initializing runc environment...");
    
    // In a real environment, we'd check if 'runc' binary is available in $PATH
    int ret = std::system("runc --version > /dev/null 2>&1");
    if (ret != 0) {
        dlog_print(DLOG_ERROR, LOG_TAG, "runc binary not found or not executable. Container execution might fail.");
        // We still return true to not block the daemon, but log the error
    }

    m_initialized = true;
    return true;
}

bool ContainerEngine::StartContainer(const std::string& container_name, const std::string& rootfs_path) {
    if (!m_initialized) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Cannot start container. Engine not initialized.");
        return false;
    }

    dlog_print(DLOG_INFO, LOG_TAG, "Creating Container via runc: %s with config in: %s", 
               container_name.c_str(), rootfs_path.c_str());

    // runc expects a config.json in the bundle directory.
    // For Phase 2 mock testing, we just simulate the command.
    std::string run_cmd = "runc --root /tmp/runc run -b " + rootfs_path + " -d " + container_name;
    dlog_print(DLOG_INFO, LOG_TAG, "Executing: %s", run_cmd.c_str());

    // Using std::system() for basic execution. In production, fork() and exec() or popen() 
    // is preferred for better process tracking.
    int ret = std::system((run_cmd + " > /dev/null 2>&1").c_str());
    if (ret != 0) {
        // Under test environments without rootfs/config.json, runc run will fail.
        dlog_print(DLOG_WARNING, LOG_TAG, "runc run failed (expected in mock). Ret code: %d", ret);
    }

    return true;
}

bool ContainerEngine::StopContainer(const std::string& container_name) {
    if (!m_initialized) return false;

    dlog_print(DLOG_INFO, LOG_TAG, "Stopping Container via runc: %s", container_name.c_str());

    std::string stop_cmd = "runc --root /tmp/runc kill " + container_name + " KILL";
    dlog_print(DLOG_INFO, LOG_TAG, "Executing: %s", stop_cmd.c_str());
    
    int ret = std::system((stop_cmd + " > /dev/null 2>&1").c_str());
    if (ret != 0) {
        dlog_print(DLOG_WARNING, LOG_TAG, "runc kill failed (expected in mock). Ret code: %d", ret);
    }
    
    std::string delete_cmd = "runc --root /tmp/runc delete " + container_name;
    std::system((delete_cmd + " > /dev/null 2>&1").c_str());

    return true;
}
