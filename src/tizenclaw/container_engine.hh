#ifndef __CONTAINER_ENGINE_H__
#define __CONTAINER_ENGINE_H__

#include <string>
#include <memory>

namespace tizenclaw {

class ContainerEngine {
public:
    ContainerEngine();
    ~ContainerEngine();

    // Initialize the container backend (crun or runc)
    bool Initialize();

    // Execute a skill: tries UDS socket first, then
    // crun exec fallback, then host-direct fallback.
    std::string ExecuteSkill(
        const std::string& skill_name,
        const std::string& arg_str);

    // Execute arbitrary Python code via the skill
    // executor's execute_code command.
    std::string ExecuteCode(
        const std::string& code);

private:
    // Execute skill via Unix Domain Socket to the
    // skill_executor running in the secure container.
    std::string ExecuteSkillViaSocket(
        const std::string& skill_name,
        const std::string& arg_str);

    // Legacy: exec into running OCI container
    std::string ExecuteSkillViaCrun(
        const std::string& skill_name,
        const std::string& arg_str);

    bool EnsureSkillsContainerRunning();
    bool PrepareSkillsBundle();
    bool IsContainerRunning() const;
    bool StartSkillsContainer();
    void StopSkillsContainer();
    bool WriteSkillsConfig() const;
    std::string BuildPaths(
        const std::string& leaf) const;
    std::string EscapeShellArg(
        const std::string& input) const;
    std::string CrunCmd(
        const std::string& subcmd) const;

    // Extract last JSON-like line from raw output
    static std::string ExtractJsonResult(
        const std::string& raw);

    bool m_initialized;
    std::string m_runtime_bin;
    std::string m_app_data_dir;
    std::string m_skills_dir;
    std::string m_bundle_dir;
    std::string m_rootfs_tar;
    std::string m_container_id;
    std::string m_crun_root;

    static constexpr const char* kSkillSocketPath =
        "/tmp/tizenclaw_skill.sock";
};

} // namespace tizenclaw

#endif // __CONTAINER_ENGINE_H__
