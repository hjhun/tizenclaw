#include "container_engine.h"
#include <dlog.h>
#include <cstdlib>
#include <cstdio>
#include <string>
#include <array>
#include <memory>
#include <fstream>
#include <stdexcept>

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw_Container"

ContainerEngine::ContainerEngine() : m_initialized(false), m_runtime_bin("crun") {
}

ContainerEngine::~ContainerEngine() {
}

bool ContainerEngine::Initialize() {
    if (m_initialized) return true;

    dlog_print(DLOG_INFO, LOG_TAG, "ContainerEngine Initializing...");
    
    // Check if crun exists, fallback to runc
    if (std::system("crun --version > /dev/null 2>&1") == 0) {
        m_runtime_bin = "crun";
    } else if (std::system("runc --version > /dev/null 2>&1") == 0) {
        m_runtime_bin = "runc";
    } else {
        dlog_print(DLOG_ERROR, LOG_TAG, "Neither crun nor runc binary found. Container execution will fail.");
        return false;
    }

    dlog_print(DLOG_INFO, LOG_TAG, "Using OCI runtime: %s", m_runtime_bin.c_str());
    m_initialized = true;
    return true;
}

std::string ContainerEngine::ExecuteSkill(const std::string& skill_name, const std::string& arg_str) {
    if (!m_initialized) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Cannot start container. Engine not initialized.");
        return "{}";
    }

    std::string bundle_dir = "/opt/usr/share/tizenclaw/bundles/" + skill_name;
    std::string rootfs_path = "/opt/usr/share/tizenclaw/rootfs.tar.gz";

    // 1. Prepare bundle directory and extract RootFS
    std::string prepare_cmd = "mkdir -p " + bundle_dir + "/rootfs && "
                              "if [ ! -f " + bundle_dir + "/.extracted ]; then "
                              "tar -xzf " + rootfs_path + " -C " + bundle_dir + "/rootfs && "
                              "touch " + bundle_dir + "/.extracted; fi";
    
    int ext_ret = std::system(prepare_cmd.c_str());
    if (ext_ret != 0) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to prepare bundle/extract rootfs! Return code: %d", ext_ret);
        return "{}";
    }

    // 2. Generate config.json mapping the skill directory
    // We bind /opt/usr/share/tizenclaw/skills inside the container at /skills
    std::string config_file = bundle_dir + "/config.json";
    
    // Escape single quotes in arg_str for the JSON string
    std::string escaped_arg_str = arg_str;
    size_t pos = 0;
    while ((pos = escaped_arg_str.find("\"", pos)) != std::string::npos) {
        escaped_arg_str.replace(pos, 1, "\\\"");
        pos += 2;
    }

    std::string config_json = R"({
        "ociVersion": "1.0.2",
        "process": {
            "terminal": false,
            "user": {"uid": 0, "gid": 0},
            "args": ["python3", "/skills/)" + skill_name + R"(/)" + skill_name + R"(.py"],
            "env": [
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                "CLAW_ARGS=)" + escaped_arg_str + R"("
            ],
            "cwd": "/",
            "capabilities": {
                "bounding": ["CAP_CHOWN", "CAP_DAC_OVERRIDE", "CAP_FOWNER", "CAP_FSETID", "CAP_KILL", "CAP_SETGID", "CAP_SETUID", "CAP_SETPCAP", "CAP_NET_BIND_SERVICE", "CAP_NET_RAW", "CAP_SYS_CHROOT", "CAP_MKNOD", "CAP_AUDIT_WRITE", "CAP_SETFCAP"]
            }
        },
        "root": {
            "path": "rootfs",
            "readonly": true
        },
        "mounts": [
            {
                "destination": "/skills",
                "type": "bind",
                "source": "/opt/usr/share/tizenclaw/skills",
                "options": ["rbind", "rro"]
            }
        ]
    })";

    std::ofstream out_conf(config_file);
    if (!out_conf.is_open()) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to write config.json");
        return "{}";
    }
    out_conf << config_json;
    out_conf.close();

    // 3. Execute the skill container synchronously and capture output via popen
    std::string run_cmd = "cd " + bundle_dir + " && " + m_runtime_bin + " run tizenclaw_" + skill_name;
    dlog_print(DLOG_INFO, LOG_TAG, "Executing: %s", run_cmd.c_str());

    std::string output;
    std::array<char, 256> buffer;
    
    // popen opens a pipe to read stdout of the command
    std::unique_ptr<FILE, decltype(&pclose)> pipe(popen(run_cmd.c_str(), "r"), pclose);
    if (!pipe) {
        dlog_print(DLOG_ERROR, LOG_TAG, "popen() failed to run container!");
        return "{}";
    }
    
    while (fgets(buffer.data(), buffer.size(), pipe.get()) != nullptr) {
        output += buffer.data();
    }

    // Attempt to automatically delete bundle afterwards (non-blocking cleanup)
    std::system((m_runtime_bin + " delete -f tizenclaw_" + skill_name + " > /dev/null 2>&1").c_str());

    return output;
}
