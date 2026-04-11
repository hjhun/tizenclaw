use crate::core::registration_store::RegisteredPaths;
use crate::generic::infra::container_engine::ContainerEngine;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntrySummary {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathStatSummary {
    pub is_dir: bool,
    pub is_file: bool,
    pub size: u64,
    pub readonly: bool,
}

pub fn summarize(
    paths: &libtizenclaw_core::framework::paths::PlatformPaths,
    registrations: &RegisteredPaths,
) -> Value {
    json!({
        "runtimes": {
            "bash": command_summary("bash"),
            "sh": command_summary("sh"),
            "python3": command_summary("python3"),
            "python": command_summary("python"),
            "node": command_summary("node"),
        },
        "utilities": {
            "cat": command_summary("cat"),
            "ls": command_summary("ls"),
            "find": command_summary("find"),
            "stat": command_summary("stat"),
            "mkdir": command_summary("mkdir"),
            "rm": command_summary("rm"),
            "cp": command_summary("cp"),
            "mv": command_summary("mv"),
        },
        "direct_execution": {
            "available": true,
            "mode": "tool_executor_or_direct_process",
            "requires_executable_bit": true
        },
        "tool_roots": {
            "managed_tools_dir": paths.tools_dir,
            "managed_skills_dir": paths.skills_dir,
            "skill_hubs_dir": paths.skill_hubs_dir,
            "registered_tool_paths": registrations.tool_paths,
            "registered_skill_paths": registrations.skill_paths,
        },
        "embedded": embedded_summary(&paths.embedded_tools_dir),
    })
}

pub async fn read_file_via_system(path: &Path) -> Result<String, String> {
    let output = execute_system_command("cat", &[path.as_os_str()], None).await?;
    if !output["success"].as_bool().unwrap_or(false) {
        return Err(command_failure("cat", &output));
    }
    Ok(output["stdout"].as_str().unwrap_or("").to_string())
}

pub async fn list_dir_via_system(path: &Path) -> Result<Vec<DirEntrySummary>, String> {
    let args = [
        OsString::from(path.as_os_str()),
        OsString::from("-mindepth"),
        OsString::from("1"),
        OsString::from("-maxdepth"),
        OsString::from("1"),
        OsString::from("-printf"),
        OsString::from("%P\t%y\t%s\n"),
    ];
    let arg_refs = [
        args[0].as_os_str(),
        args[1].as_os_str(),
        args[2].as_os_str(),
        args[3].as_os_str(),
        args[4].as_os_str(),
        args[5].as_os_str(),
        args[6].as_os_str(),
    ];
    let output = execute_system_command("find", &arg_refs, None).await?;
    if !output["success"].as_bool().unwrap_or(false) {
        return Err(command_failure("find", &output));
    }

    let mut entries = Vec::new();
    for line in output["stdout"]
        .as_str()
        .unwrap_or("")
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let mut parts = line.splitn(3, '\t');
        let name = parts.next().unwrap_or("").to_string();
        let kind = parts.next().unwrap_or("");
        let size = parts
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        if name.is_empty() {
            continue;
        }
        entries.push(DirEntrySummary {
            path: path.join(&name).to_string_lossy().to_string(),
            name,
            is_dir: matches!(kind, "d"),
            size,
        });
    }

    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

pub async fn stat_path_via_system(path: &Path) -> Result<PathStatSummary, String> {
    let args = [
        OsString::from("-c"),
        OsString::from("%F\t%s\t%a"),
        OsString::from(path.as_os_str()),
    ];
    let arg_refs = [args[0].as_os_str(), args[1].as_os_str(), args[2].as_os_str()];
    let output = execute_system_command("stat", &arg_refs, None).await?;
    if !output["success"].as_bool().unwrap_or(false) {
        return Err(command_failure("stat", &output));
    }

    let line = output["stdout"]
        .as_str()
        .unwrap_or("")
        .lines()
        .next()
        .ok_or_else(|| "stat returned no output".to_string())?;
    let mut parts = line.splitn(3, '\t');
    let kind = parts.next().unwrap_or("");
    let size = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let permissions = parts.next().unwrap_or("");
    let perm_mode = u32::from_str_radix(permissions, 8).unwrap_or(0);
    Ok(PathStatSummary {
        is_dir: kind == "directory",
        is_file: kind.contains("file"),
        size,
        readonly: perm_mode & 0o222 == 0,
    })
}

pub async fn mkdir_via_system(path: &Path) -> Result<(), String> {
    let args = [OsString::from("-p"), OsString::from(path.as_os_str())];
    let arg_refs = [args[0].as_os_str(), args[1].as_os_str()];
    let output = execute_system_command("mkdir", &arg_refs, None).await?;
    if output["success"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        Err(command_failure("mkdir", &output))
    }
}

pub async fn remove_via_system(path: &Path, is_dir: bool) -> Result<(), String> {
    let args = if is_dir {
        vec![
            OsString::from("-rf"),
            OsString::from("--"),
            OsString::from(path.as_os_str()),
        ]
    } else {
        vec![
            OsString::from("-f"),
            OsString::from("--"),
            OsString::from(path.as_os_str()),
        ]
    };
    let arg_refs = args.iter().map(|arg| arg.as_os_str()).collect::<Vec<_>>();
    let output = execute_system_command("rm", &arg_refs, None).await?;
    if output["success"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        Err(command_failure("rm", &output))
    }
}

pub async fn copy_via_system(src: &Path, dst: &Path, recursive: bool) -> Result<u64, String> {
    let args = if recursive {
        vec![
            OsString::from("-R"),
            OsString::from("--"),
            OsString::from(src.as_os_str()),
            OsString::from(dst.as_os_str()),
        ]
    } else {
        vec![
            OsString::from("--"),
            OsString::from(src.as_os_str()),
            OsString::from(dst.as_os_str()),
        ]
    };
    let arg_refs = args.iter().map(|arg| arg.as_os_str()).collect::<Vec<_>>();
    let output = execute_system_command("cp", &arg_refs, None).await?;
    if !output["success"].as_bool().unwrap_or(false) {
        return Err(command_failure("cp", &output));
    }
    Ok(std::fs::metadata(dst).map(|metadata| metadata.len()).unwrap_or(0))
}

pub async fn move_via_system(src: &Path, dst: &Path) -> Result<(), String> {
    let args = [
        OsString::from("--"),
        OsString::from(src.as_os_str()),
        OsString::from(dst.as_os_str()),
    ];
    let arg_refs = [args[0].as_os_str(), args[1].as_os_str(), args[2].as_os_str()];
    let output = execute_system_command("mv", &arg_refs, None).await?;
    if output["success"].as_bool().unwrap_or(false) {
        Ok(())
    } else {
        Err(command_failure("mv", &output))
    }
}

fn embedded_summary(root: &Path) -> Value {
    let descriptor_names = embedded_descriptor_names(root);
    let descriptor_count = descriptor_names.len();
    let recommendation = if descriptor_count == 0 {
        "No embedded descriptors were found. Keep reusable workflows in textual skills and hard-coded capabilities in Rust."
    } else {
        "Embedded descriptors are documentation-only metadata for built-in capabilities. Migrate reusable workflows to textual skills and keep only hard-coded runtime capabilities embedded."
    };

    json!({
        "root_dir": root,
        "descriptor_count": descriptor_count,
        "descriptor_names": descriptor_names,
        "status": "documentation_only",
        "recommendation": recommendation,
    })
}

fn embedded_descriptor_names(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut names = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let file_name = path.file_name()?.to_str()?.to_string();
            if !file_name.ends_with(".md")
                || file_name == "index.md"
                || file_name == "tools.md"
                || file_name.starts_with('.')
            {
                return None;
            }
            Some(file_name.trim_end_matches(".md").to_string())
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn command_summary(name: &str) -> Value {
    let resolved = resolve_command(name);
    json!({
        "available": resolved.is_some(),
        "requested": name,
        "path": resolved.map(|path| path.to_string_lossy().to_string()),
    })
}

fn resolve_command(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let full = dir.join(name);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

async fn execute_system_command(
    binary: &str,
    args: &[&std::ffi::OsStr],
    cwd: Option<&Path>,
) -> Result<Value, String> {
    let resolved = resolve_command(binary).ok_or_else(|| format!("Required command '{}' was not found on PATH", binary))?;
    let binary_str = resolved.to_string_lossy().to_string();
    let owned_args = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let arg_refs = owned_args.iter().map(|arg| arg.as_str()).collect::<Vec<_>>();
    let cwd_str = cwd.map(|path| path.to_string_lossy().to_string());
    let engine = ContainerEngine::new();
    engine
        .execute_oneshot(&binary_str, &arg_refs, cwd_str.as_deref())
        .await
}

fn command_failure(command: &str, output: &Value) -> String {
    let stderr = output
        .get("stderr")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let exit_code = output
        .get("exit_code")
        .and_then(|value| value.as_i64())
        .unwrap_or(-1);
    if stderr.is_empty() {
        format!("{} failed with exit code {}", command, exit_code)
    } else {
        format!("{} failed with exit code {}: {}", command, exit_code, stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DirEntrySummary, PathStatSummary, embedded_summary, embedded_descriptor_names, summarize,
    };
    use crate::core::registration_store::RegisteredPaths;
    use libtizenclaw_core::framework::paths::PlatformPaths;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn embedded_descriptor_scan_ignores_index_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("index.md"), "# Index").unwrap();
        std::fs::write(dir.path().join("tools.md"), "# Tools").unwrap();
        std::fs::write(dir.path().join("read_file.md"), "# Read").unwrap();

        let names = embedded_descriptor_names(dir.path());
        assert_eq!(names, vec!["read_file"]);
    }

    #[test]
    fn embedded_summary_reports_documentation_only_status() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("create_task.md"), "# Task").unwrap();

        let summary = embedded_summary(dir.path());
        assert_eq!(summary["descriptor_count"], json!(1));
        assert_eq!(summary["status"], json!("documentation_only"));
    }

    #[test]
    fn summarize_includes_registered_roots_and_runtime_shape() {
        let temp = tempdir().unwrap();
        let base = temp.path().join("runtime");
        let paths = PlatformPaths::from_base(base.clone());
        paths.ensure_dirs();

        let mut registrations = RegisteredPaths::default();
        registrations.tool_paths.push("/tmp/tools-extra".to_string());
        registrations.skill_paths.push("/tmp/skills-extra".to_string());

        let summary = summarize(&paths, &registrations);
        assert!(summary["runtimes"]["bash"]["available"].is_boolean());
        assert_eq!(
            summary["tool_roots"]["registered_tool_paths"][0],
            json!("/tmp/tools-extra")
        );
        assert_eq!(
            summary["tool_roots"]["registered_skill_paths"][0],
            json!("/tmp/skills-extra")
        );
        assert!(summary["embedded"]["descriptor_count"].is_number());
    }

    #[test]
    fn dir_entry_summary_is_plain_data() {
        let entry = DirEntrySummary {
            name: "demo.txt".to_string(),
            path: "/tmp/demo.txt".to_string(),
            is_dir: false,
            size: 12,
        };
        assert_eq!(entry.name, "demo.txt");
        assert_eq!(entry.size, 12);
    }

    #[test]
    fn path_stat_summary_is_plain_data() {
        let stat = PathStatSummary {
            is_dir: false,
            is_file: true,
            size: 9,
            readonly: false,
        };
        assert!(stat.is_file);
        assert_eq!(stat.size, 9);
    }
}
