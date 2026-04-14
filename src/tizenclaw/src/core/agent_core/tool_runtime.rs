async fn file_manager_tool(tc_args: &Value, session_workdir: &Path) -> Value {
    let force_rust_fallback = tc_args
        .get("backend_preference")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().eq_ignore_ascii_case("rust_fallback"))
        .unwrap_or(false);
    let operation = tc_args
        .get("operation")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    match operation.as_str() {
        "read" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let pattern = tc_args.get("pattern").and_then(|value| value.as_str());
            let max_chars = tc_args
                .get("max_chars")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
                .filter(|value| *value > 0)
                .unwrap_or(DEFAULT_FILE_READ_MAX_CHARS);
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if path.is_dir() {
                if !force_rust_fallback {
                    match runtime_capabilities::list_dir_via_system(&path).await {
                        Ok(entries) => {
                            let entries = entries
                                .into_iter()
                                .map(|entry| {
                                    json!({
                                        "name": entry.name,
                                        "path": entry.path,
                                        "is_dir": entry.is_dir,
                                        "size": entry.size,
                                    })
                                })
                                .collect::<Vec<_>>();
                            return build_directory_read_payload(
                                &path,
                                entries,
                                "linux_utility",
                                pattern,
                                max_chars,
                            );
                        }
                        Err(system_error) => {
                            log::debug!(
                                "[file_manager] directory read fallback for '{}': {}",
                                path.display(),
                                system_error
                            );
                        }
                    }
                }
                return match std::fs::read_dir(&path) {
                    Ok(entries) => {
                        let entries = entries
                            .filter_map(|entry| entry.ok())
                            .map(|entry| {
                                let entry_path = entry.path();
                                let metadata = entry.metadata().ok();
                                json!({
                                    "name": entry.file_name().to_string_lossy(),
                                    "path": entry_path.to_string_lossy(),
                                    "is_dir": metadata.as_ref().map(|value| value.is_dir()).unwrap_or(false),
                                    "size": metadata.as_ref().map(|value| value.len()).unwrap_or(0),
                                })
                            })
                            .collect::<Vec<_>>();
                        build_directory_read_payload(
                            &path,
                            entries,
                            "rust_fallback",
                            pattern,
                            max_chars,
                        )
                    }
                    Err(error) => {
                        json!({"error": format!("Failed to read directory '{}': {}", path.display(), error)})
                    }
                };
            }
            if let Some(tool_name) = specialized_read_tool_for_path(&path) {
                let redirected = match tool_name {
                    "extract_document_text" => {
                        feature_tools::extract_document_text(path_str, None, None, session_workdir)
                            .await
                    }
                    "inspect_tabular_data" => {
                        feature_tools::inspect_tabular_data(path_str, 5, session_workdir).await
                    }
                    _ => json!({
                        "error": format!("Unsupported specialized read tool '{}'", tool_name)
                    }),
                };
                if redirected.get("error").is_some() {
                    return redirected;
                }
                let mut redirected = redirected;
                if let Some(object) = redirected.as_object_mut() {
                    object.insert(
                        "redirected_from".into(),
                        json!("file_manager.read"),
                    );
                    object.insert("recommended_tool".into(), json!(tool_name));
                }
                return redirected;
            }
            if file_looks_binary(&path) {
                return json!({
                    "error": format!(
                        "Binary file detected at '{}'. Use a specialized extraction tool instead of file_manager read.",
                        path.display()
                    ),
                    "path": path.to_string_lossy(),
                    "recommended_tool": "extract_document_text",
                });
            }
            if !force_rust_fallback {
                match runtime_capabilities::read_file_via_system(&path).await {
                    Ok(content) => {
                        let mut payload = build_text_read_payload(&content, pattern, max_chars);
                        let object = payload.as_object_mut().expect("read payload object");
                        object.insert("success".into(), json!(true));
                        object.insert("operation".into(), json!(operation));
                        object.insert("path".into(), json!(path.to_string_lossy()));
                        object.insert("backend".into(), json!("linux_utility"));
                        return json!({
                            "success": object.get("success").cloned().unwrap(),
                            "operation": object.get("operation").cloned().unwrap(),
                            "path": object.get("path").cloned().unwrap(),
                            "content": object.get("content").cloned().unwrap(),
                            "truncated": object.get("truncated").cloned().unwrap(),
                            "total_chars": object.get("total_chars").cloned().unwrap(),
                            "pattern": object.get("pattern").cloned().unwrap(),
                            "match_count": object.get("match_count").cloned().unwrap(),
                            "backend": object.get("backend").cloned().unwrap(),
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] read fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let mut payload = build_text_read_payload(&content, pattern, max_chars);
                    let object = payload.as_object_mut().expect("read payload object");
                    object.insert("success".into(), json!(true));
                    object.insert("operation".into(), json!(operation));
                    object.insert("path".into(), json!(path.to_string_lossy()));
                    object.insert("backend".into(), json!("rust_fallback"));
                    payload
                }
                Err(error) => {
                    json!({"error": format!("Failed to read file '{}': {}", path.display(), error)})
                }
            }
        }
        "write" | "append" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let content = tc_args
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if let Some(parent) = path.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create directory '{}': {}", parent.display(), error)});
                }
            }
            let write_result = if operation == "append" {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut file| std::io::Write::write_all(&mut file, content.as_bytes()))
            } else {
                std::fs::write(&path, content.as_bytes())
            };
            match write_result {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "bytes_written": content.len()
                }),
                Err(error) => {
                    json!({"error": format!("Failed to {} file '{}': {}", operation, path.display(), error)})
                }
            }
        }
        "remove" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !path.exists() {
                return json!({"error": format!("Failed to remove '{}': path does not exist", path.display())});
            }
            let is_dir = path.is_dir();
            if !force_rust_fallback {
                match runtime_capabilities::remove_via_system(&path, is_dir).await {
                    Ok(()) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] remove fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            let result = if is_dir {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match result {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to remove '{}': {}", path.display(), error)})
                }
            }
        }
        "mkdir" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::mkdir_via_system(&path).await {
                    Ok(()) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] mkdir fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::create_dir_all(&path) {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to create directory '{}': {}", path.display(), error)})
                }
            }
        }
        "list" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::list_dir_via_system(&path).await {
                    Ok(entries) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "entries": entries.into_iter().map(|entry| json!({
                                "name": entry.name,
                                "path": entry.path,
                                "is_dir": entry.is_dir,
                                "size": entry.size,
                            })).collect::<Vec<_>>(),
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] list fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let entries = entries
                        .filter_map(|entry| entry.ok())
                        .map(|entry| {
                            let entry_path = entry.path();
                            let metadata = entry.metadata().ok();
                            json!({
                                "name": entry.file_name().to_string_lossy(),
                                "path": entry_path.to_string_lossy(),
                                "is_dir": metadata.as_ref().map(|value| value.is_dir()).unwrap_or(false),
                                "size": metadata.as_ref().map(|value| value.len()).unwrap_or(0)
                            })
                        })
                        .collect::<Vec<_>>();
                    json!({
                        "success": true,
                        "operation": operation,
                        "path": path.to_string_lossy(),
                        "entries": entries,
                        "backend": "rust_fallback",
                    })
                }
                Err(error) => {
                    json!({"error": format!("Failed to list directory '{}': {}", path.display(), error)})
                }
            }
        }
        "stat" => {
            let path_str = tc_args
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let path = match resolve_workspace_path(session_workdir, path_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if !force_rust_fallback {
                match runtime_capabilities::stat_path_via_system(&path).await {
                    Ok(metadata) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "path": path.to_string_lossy(),
                            "is_dir": metadata.is_dir,
                            "is_file": metadata.is_file,
                            "size": metadata.size,
                            "readonly": metadata.readonly,
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] stat fallback for '{}': {}",
                            path.display(),
                            system_error
                        );
                    }
                }
            }
            match std::fs::metadata(&path) {
                Ok(metadata) => json!({
                    "success": true,
                    "operation": operation,
                    "path": path.to_string_lossy(),
                    "is_dir": metadata.is_dir(),
                    "is_file": metadata.is_file(),
                    "size": metadata.len(),
                    "readonly": metadata.permissions().readonly(),
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to stat '{}': {}", path.display(), error)})
                }
            }
        }
        "copy" | "move" => {
            let src_str = tc_args
                .get("src")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let dst_str = tc_args
                .get("dst")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let src = match resolve_workspace_path(session_workdir, src_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": format!("src: {}", error)}),
            };
            let dst = match resolve_workspace_path(session_workdir, dst_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": format!("dst: {}", error)}),
            };
            if let Some(parent) = dst.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create destination directory '{}': {}", parent.display(), error)});
                }
            }
            let recursive = src.is_dir();
            if !force_rust_fallback {
                let system_result = if operation == "move" {
                    runtime_capabilities::move_via_system(&src, &dst)
                        .await
                        .map(|_| 0u64)
                } else {
                    runtime_capabilities::copy_via_system(&src, &dst, recursive).await
                };
                match system_result {
                    Ok(bytes) => {
                        return json!({
                            "success": true,
                            "operation": operation,
                            "src": src.to_string_lossy(),
                            "dst": dst.to_string_lossy(),
                            "bytes_copied": bytes,
                            "backend": "linux_utility",
                        });
                    }
                    Err(system_error) => {
                        log::debug!(
                            "[file_manager] {} fallback '{}' -> '{}': {}",
                            operation,
                            src.display(),
                            dst.display(),
                            system_error
                        );
                    }
                }
            }
            let result = if operation == "move" {
                std::fs::rename(&src, &dst).map(|_| 0u64)
            } else {
                std::fs::copy(&src, &dst)
            };
            match result {
                Ok(bytes) => json!({
                    "success": true,
                    "operation": operation,
                    "src": src.to_string_lossy(),
                    "dst": dst.to_string_lossy(),
                    "bytes_copied": bytes,
                    "backend": "rust_fallback",
                }),
                Err(error) => {
                    json!({"error": format!("Failed to {} '{}' -> '{}': {}", operation, src.display(), dst.display(), error)})
                }
            }
        }
        "download" => {
            let url = tc_args
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim();
            let dest_str = tc_args
                .get("dest")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if url.is_empty() {
                return json!({"error": "Missing required url"});
            }
            if let Some((tool_name, params)) = parse_tool_uri(url) {
                return match tool_name.as_str() {
                    "extract_document_text" => {
                        let path = params.get("path").map(String::as_str).unwrap_or("");
                        let output_path = params.get("output_path").map(String::as_str);
                        let max_chars = params
                            .get("max_chars")
                            .and_then(|value| value.parse::<usize>().ok());
                        feature_tools::extract_document_text(
                            path,
                            output_path,
                            max_chars,
                            session_workdir,
                        )
                        .await
                    }
                    "inspect_tabular_data" => {
                        let path = params.get("path").map(String::as_str).unwrap_or("");
                        let preview_rows = params
                            .get("preview_rows")
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(5);
                        feature_tools::inspect_tabular_data(path, preview_rows, session_workdir)
                            .await
                    }
                    _ => json!({
                        "error": format!(
                            "Unsupported tool URI '{}'. Supported tool:// targets: extract_document_text, inspect_tabular_data",
                            tool_name
                        )
                    }),
                };
            }
            let dest = match resolve_workspace_path(session_workdir, dest_str) {
                Ok(path) => path,
                Err(error) => return json!({"error": error}),
            };
            if let Some(parent) = dest.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return json!({"error": format!("Failed to create destination directory '{}': {}", parent.display(), error)});
                }
            }
            let response = crate::infra::http_client::http_get(url, &[], 1, 60).await;
            if !response.success {
                return json!({"error": format!("Failed to download '{}': {}", url, response.error)});
            }
            match std::fs::write(&dest, response.body.as_bytes()) {
                Ok(()) => json!({
                    "success": true,
                    "operation": operation,
                    "url": url,
                    "dest": dest.to_string_lossy(),
                    "bytes_written": response.body.len()
                }),
                Err(error) => {
                    json!({"error": format!("Failed to save download to '{}': {}", dest.display(), error)})
                }
            }
        }
        _ => json!({"error": format!("Unsupported file_manager operation '{}'", operation)}),
    }
}

fn extract_option_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn canonical_tool_trace(tc: &backend::LlmToolCall) -> Value {
    let default = json!({
        "type": "toolCall",
        "tool_call_id": tc.id,
        "name": tc.name,
        "params": tc.args,
        "arguments": tc.args,
        "actual_tool_name": tc.name
    });

    if tc.name == "file_manager" {
        let operation = tc
            .args
            .get("operation")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        return match operation.as_str() {
            "read" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "read_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "file_path": path,
                        "files": [path],
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path,
                        "file_path": path,
                        "files": [path]
                    }
                })
            }
            "write" | "append" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let content = tc
                    .args
                    .get("content")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "write_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "file_path": path,
                        "content": content,
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path,
                        "file_path": path,
                        "content": content
                    }
                })
            }
            "list" => {
                let path = tc
                    .args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or(".");
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "list_files",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path,
                        "operation": operation,
                        "actual_tool_name": tc.name
                    },
                    "arguments": {
                        "path": path
                    }
                })
            }
            _ => default.clone(),
        };
    }

    if tc.name == "run_generated_code" {
        let runtime = tc
            .args
            .get("runtime")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let code = tc
            .args
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();

        if runtime == "bash" && !code.is_empty() {
            let parsed = parse_shell_like_args(code);
            if let Some(command) = parsed.first().map(String::as_str) {
                match command {
                    "cat" => {
                        if let Some(path) = parsed.get(1) {
                            return json!({
                                "type": "toolCall",
                                "tool_call_id": tc.id,
                                "name": "read_file",
                                "actual_tool_name": tc.name,
                                "params": {
                                    "path": path.as_str(),
                                    "file_path": path.as_str(),
                                    "files": [path.as_str()],
                                    "runtime": runtime,
                                    "actual_tool_name": tc.name,
                                    "raw_code": code
                                },
                                "arguments": {
                                    "path": path.as_str(),
                                    "file_path": path.as_str(),
                                    "files": [path.as_str()]
                                }
                            });
                        }
                    }
                    "ls" => {
                        let path = parsed
                            .iter()
                            .skip(1)
                            .find(|value| !value.starts_with('-'))
                            .map(String::as_str)
                            .unwrap_or(".");
                        return json!({
                            "type": "toolCall",
                            "tool_call_id": tc.id,
                            "name": "list_files",
                            "actual_tool_name": tc.name,
                            "params": {
                                "path": path,
                                "runtime": runtime,
                                "actual_tool_name": tc.name,
                                "raw_code": code
                            },
                            "arguments": {
                                "path": path
                            }
                        });
                    }
                    "find" => {
                        let path = parsed.get(1).map(String::as_str).unwrap_or(".");
                        return json!({
                            "type": "toolCall",
                            "tool_call_id": tc.id,
                            "name": "list_files",
                            "actual_tool_name": tc.name,
                            "params": {
                                "path": path,
                                "runtime": runtime,
                                "actual_tool_name": tc.name,
                                "raw_code": code
                            },
                            "arguments": {
                                "path": path
                            }
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    let Some(args_str) = tc.args.get("args").and_then(|value| value.as_str()) else {
        return default;
    };

    if !tc.name.contains("file-manager") {
        return default;
    }

    let parsed = parse_shell_like_args(args_str);
    let Some(subcommand) = parsed.first().map(String::as_str) else {
        return default;
    };

    match subcommand {
        "read" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "read_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "files": [path.as_str()],
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "files": [path.as_str()]
                    }
                })
            } else {
                default
            }
        }
        "write" | "append" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                let content = extract_option_value(&parsed, "--content").unwrap_or_default();
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "write_file",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "content": content.as_str(),
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str(),
                        "file_path": path.as_str(),
                        "content": content.as_str()
                    }
                })
            } else {
                default
            }
        }
        "list" => {
            if let Some(path) = extract_option_value(&parsed, "--path") {
                json!({
                    "type": "toolCall",
                    "tool_call_id": tc.id,
                    "name": "list_files",
                    "actual_tool_name": tc.name,
                    "params": {
                        "path": path.as_str(),
                        "subcommand": subcommand,
                        "actual_tool_name": tc.name,
                        "raw_args": args_str
                    },
                    "arguments": {
                        "path": path.as_str()
                    }
                })
            } else {
                default
            }
        }
        _ => default,
    }
}

fn generated_code_runtime_spec(runtime: &str) -> Option<(&'static str, &'static str)> {
    match runtime.trim().to_ascii_lowercase().as_str() {
        "python" | "python3" => Some(("python3", ".py")),
        "node" => Some(("node", ".js")),
        "bash" => Some(("bash", ".sh")),
        _ => None,
    }
}

fn sanitize_generated_code_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut previous_was_dash = false;

    for ch in name.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            slug.push(lowered);
            previous_was_dash = false;
        } else if !previous_was_dash && !slug.is_empty() {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "script".to_string()
    } else {
        slug
    }
}

fn generated_code_date_prefix() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm_buf) };

    format!(
        "{:04}-{:02}-{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday
    )
}

fn generated_code_script_path(base_dir: &Path, runtime: &str, name: &str) -> Option<PathBuf> {
    let (_, suffix) = generated_code_runtime_spec(runtime)?;
    let codes_dir = base_dir.join("codes");
    let date_prefix = generated_code_date_prefix();
    let base_name = sanitize_generated_code_name(name);

    for attempt in 0..1000usize {
        let file_name = if attempt == 0 {
            format!("{date_prefix}-generated-{base_name}{suffix}")
        } else {
            format!("{date_prefix}-generated-{base_name}-{attempt}{suffix}")
        };
        let candidate = codes_dir.join(file_name);
        if !candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn list_generated_code_entries(codes_dir: &Path) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(codes_dir).map_err(|err| {
        format!(
            "Failed to read codes dir '{}': {}",
            codes_dir.display(),
            err
        )
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|err| format!("Failed to read codes entry: {}", err))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|err| format!("Failed to read metadata for '{}': {}", path.display(), err))?;
        let name = entry.file_name().to_string_lossy().to_string();
        entries.push(json!({
            "name": name,
            "path": path.to_string_lossy().to_string(),
            "size_bytes": metadata.len(),
        }));
    }

    entries.sort_by(|left, right| {
        let left_name = left.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let right_name = right.get("name").and_then(|v| v.as_str()).unwrap_or("");
        left_name.cmp(right_name)
    });

    Ok(entries)
}

fn validate_generated_code_filename(name: &str) -> Result<&str, String> {
    if name.is_empty() {
        return Err("Missing generated code filename".to_string());
    }
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(format!("Invalid generated code filename '{}'", name));
    }
    Ok(name)
}

fn manage_generated_code_tool(operation: &str, name: Option<&str>, base_dir: &Path) -> Value {
    let codes_dir = base_dir.join("codes");
    if let Err(err) = std::fs::create_dir_all(&codes_dir) {
        return json!({"error": format!("Failed to create codes dir: {}", err)});
    }

    match operation {
        "list" => match list_generated_code_entries(&codes_dir) {
            Ok(entries) => json!({
                "status": "success",
                "operation": "list",
                "count": entries.len(),
                "entries": entries,
            }),
            Err(err) => json!({ "error": err }),
        },
        "delete" => {
            let Some(file_name) = name else {
                return json!({"error": "Missing 'name' for delete operation"});
            };
            let file_name = match validate_generated_code_filename(file_name) {
                Ok(file_name) => file_name,
                Err(err) => return json!({ "error": err }),
            };
            let target = codes_dir.join(file_name);
            if !target.exists() {
                return json!({
                    "error": format!("Generated code file '{}' was not found", file_name)
                });
            }
            match std::fs::remove_file(&target) {
                Ok(()) => json!({
                    "status": "success",
                    "operation": "delete",
                    "name": file_name,
                    "path": target.to_string_lossy().to_string(),
                }),
                Err(err) => json!({
                    "error": format!("Failed to delete '{}': {}", target.display(), err)
                }),
            }
        }
        "delete_all" => match list_generated_code_entries(&codes_dir) {
            Ok(entries) => {
                let mut deleted = Vec::new();
                for entry in entries {
                    if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                        if let Err(err) = std::fs::remove_file(path) {
                            return json!({
                                "error": format!("Failed to delete '{}': {}", path, err)
                            });
                        }
                        deleted.push(path.to_string());
                    }
                }
                json!({
                    "status": "success",
                    "operation": "delete_all",
                    "deleted_count": deleted.len(),
                    "deleted_paths": deleted,
                })
            }
            Err(err) => json!({ "error": err }),
        },
        other => json!({
            "error": format!(
                "Unsupported operation '{}'. Expected list, delete, or delete_all.",
                other
            )
        }),
    }
}

fn task_scheduler_dir(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join("tasks")
}

fn list_tasks_tool(base_dir: &Path) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::list_tasks_from_dir(&task_dir) {
        Ok(tasks) => json!({
            "status": "success",
            "count": tasks.len(),
            "tasks": tasks.into_iter().map(|task| json!({
                "id": task.id,
                "name": task.name,
                "session_id": task.session_id,
                "interval_secs": task.interval_secs,
                "schedule": task.schedule_expr,
                "one_shot": task.one_shot,
                "enabled": task.enabled,
                "project_dir": task.project_dir,
                "coding_backend": task.coding_backend,
                "coding_model": task.coding_model,
                "execution_mode": task.execution_mode,
                "auto_approve": task.auto_approve,
                "prompt": task.prompt,
            })).collect::<Vec<_>>(),
        }),
        Err(err) => json!({ "error": err }),
    }
}

fn create_task_tool(
    base_dir: &Path,
    schedule: &str,
    prompt: &str,
    project_dir: Option<&str>,
    coding_backend: Option<&str>,
    coding_model: Option<&str>,
    execution_mode: Option<&str>,
    auto_approve: bool,
) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::create_task_file(
        &task_dir,
        schedule,
        prompt,
        project_dir,
        coding_backend,
        coding_model,
        execution_mode,
        auto_approve,
    ) {
        Ok(task) => json!({
            "status": "success",
            "task": {
                "id": task.id,
                "name": task.name,
                "session_id": task.session_id,
                "interval_secs": task.interval_secs,
                "schedule": task.schedule_expr,
                "one_shot": task.one_shot,
                "enabled": task.enabled,
                "project_dir": task.project_dir,
                "coding_backend": task.coding_backend,
                "coding_model": task.coding_model,
                "execution_mode": task.execution_mode,
                "auto_approve": task.auto_approve,
                "prompt": task.prompt,
            }
        }),
        Err(err) => json!({ "error": err }),
    }
}

fn cancel_task_tool(base_dir: &Path, task_id: &str) -> Value {
    let task_dir = task_scheduler_dir(base_dir);
    match crate::core::task_scheduler::TaskScheduler::delete_task_file(&task_dir, task_id) {
        Ok(true) => json!({
            "status": "success",
            "task_id": task_id.trim(),
        }),
        Ok(false) => json!({
            "error": format!("Task '{}' was not found", task_id.trim()),
        }),
        Err(err) => json!({ "error": err }),
    }
}

async fn run_generated_code_tool(
    runtime: &str,
    name: Option<&str>,
    code: &str,
    args: &str,
    base_dir: &Path,
    workdir: Option<&Path>,
    declared_output_path: Option<&str>,
    declared_output_level: Option<&str>,
    enforce_atomic_answer: bool,
) -> Value {
    let code_trimmed = code.trim_start();
    let looks_like_web_markup = code_trimmed.starts_with("<!DOCTYPE html")
        || code_trimmed.starts_with("<html")
        || code_trimmed.contains("<head")
        || code_trimmed.contains("<body")
        || code_trimmed.contains("<script")
        || code_trimmed.contains("<style");
    if looks_like_web_markup {
        return json!({
            "error": "HTML/CSS/JS browser content is not supported by run_generated_code. Use generate_web_app instead."
        });
    }

    let binary = match generated_code_runtime_spec(runtime) {
        Some((binary, _suffix)) => binary,
        None => {
            return json!({
                "error": format!(
                    "Unsupported runtime '{}'. Expected python, python3, node, or bash.",
                    runtime
                )
            });
        }
    };

    let target_dir = workdir
        .map(|path| path.join("codes"))
        .unwrap_or_else(|| base_dir.join("codes"));
    if let Err(err) = std::fs::create_dir_all(&target_dir) {
        return json!({"error": format!("Failed to create codes dir: {}", err)});
    }

    let Some((_, suffix)) = generated_code_runtime_spec(runtime) else {
        return json!({"error": format!("Unsupported runtime '{}'.", runtime)});
    };
    let script_name = format!(
        "{}{}",
        sanitize_generated_code_name(name.unwrap_or("script")),
        suffix
    );
    let script_path = target_dir.join(script_name);
    let mut temp_file = match std::fs::File::create(&script_path) {
        Ok(file) => file,
        Err(err) => {
            return json!({"error": format!("Failed to create code file: {}", err)});
        }
    };

    if let Err(err) = temp_file.write_all(code.as_bytes()) {
        return json!({"error": format!("Failed to write generated code: {}", err)});
    }
    if let Err(err) = temp_file.flush() {
        return json!({"error": format!("Failed to flush generated code: {}", err)});
    }
    let script_path = script_path.to_string_lossy().to_string();
    let mut exec_args = vec![script_path.clone()];
    exec_args.extend(parse_shell_like_args(args));
    let exec_args_ref: Vec<&str> = exec_args.iter().map(|value| value.as_str()).collect();

    let engine = crate::infra::container_engine::ContainerEngine::new();
    let cwd = target_dir.to_string_lossy().to_string();
    match engine
        .execute_oneshot(binary, &exec_args_ref, Some(cwd.as_str()))
        .await
    {
        Ok(result) => {
            let mut saved_output_path = None;
            let stderr = result
                .get("stderr")
                .and_then(|value| value.as_str())
                .unwrap_or("");

            if result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                let stdout = result
                    .get("stdout")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if let Err(err) = validate_generated_code_execution_output(
                    stdout,
                    declared_output_level,
                    enforce_atomic_answer,
                ) {
                    return json!({
                        "runtime": runtime,
                        "script_path": script_path,
                        "name": script_path.rsplit('/').next().unwrap_or(""),
                        "saved_output_path": saved_output_path,
                        "result": result,
                        "error": err
                    });
                }

                if let Some(path) = declared_output_path {
                    let output_path = Path::new(path);
                    if let Err(err) = persist_generated_code_copy(code, output_path) {
                        return json!({ "error": err });
                    }
                    saved_output_path = Some(output_path.to_string_lossy().to_string());
                }
            }

            let module_error = if stderr.contains("ModuleNotFoundError")
                && (stderr.contains("pandas") || stderr.contains("numpy"))
            {
                Some(
                    "Required third-party Python packages are unavailable on the target runtime. Generate code that executes successfully with the available standard library instead of importing missing packages."
                        .to_string(),
                )
            } else {
                None
            };

            json!({
                "runtime": runtime,
                "script_path": script_path,
                "name": script_path.rsplit('/').next().unwrap_or(""),
                "saved_output_path": saved_output_path,
                "result": result,
                "error": module_error
            })
        }
        Err(err) => json!({
            "runtime": runtime,
            "script_path": script_path,
            "name": script_path.rsplit('/').next().unwrap_or(""),
            "saved_output_path": serde_json::Value::Null,
            "error": format!("Failed to execute generated code: {}", err)
        }),
    }
}

/// LLM backend configuration loaded from `llm_config.json`.
#[derive(Debug)]
struct LlmConfig {
    active_backend: String,
    fallback_backends: Vec<String>,
    backends: Value,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self::from_document(&llm_config_store::default_document())
    }
}

impl LlmConfig {
    fn from_document(json: &Value) -> Self {
        LlmConfig {
            active_backend: json["active_backend"]
                .as_str()
                .unwrap_or("gemini")
                .to_string(),
            fallback_backends: json["fallback_backends"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_else(|| vec!["openai".into(), "ollama".into()]),
            backends: json.get("backends").cloned().unwrap_or_else(|| json!({})),
        }
    }

    /// Load LLM config from a JSON file.
    fn load(path: &str) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("LLM config not found at {}, using defaults", path);
                return Self::default();
            }
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to parse LLM config: {}", e);
                return Self::default();
            }
        };

        Self::from_document(&json)
    }

    /// Get config for a specific backend.
    fn backend_config(&self, name: &str) -> Value {
        self.backends.get(name).cloned().unwrap_or(json!({}))
    }
}

/// Merge backend auth material from `KeyStore` into a backend config `Value`.
///
/// Priority:
///  1. Explicit `api_key` in the JSON config block (non-empty)
///  2. Explicit `oauth.access_token` in the JSON config block (non-empty)
///  3. `{config_dir}/keys/<backend>.key`
///  4. `{config_dir}/keys/<backend>.access_token.key`
///  5. `{config_dir}/keys/<backend>.refresh_token.key`
fn merge_backend_auth(mut cfg: Value, name: &str, ks: &crate::infra::key_store::KeyStore) -> Value {
    let has_api_key = cfg
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let has_access_token = cfg
        .get("oauth")
        .and_then(|v| v.get("access_token"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    if !has_api_key && !has_access_token {
        if let Some(key) = ks.get(name) {
            if !key.is_empty() {
                cfg["api_key"] = Value::String(key);
            }
        } else if let Some(access_token) = ks.get(&format!("{}.access_token", name)) {
            if !access_token.is_empty() {
                if !cfg.get("oauth").map(|v| v.is_object()).unwrap_or(false) {
                    cfg["oauth"] = json!({});
                }
                cfg["oauth"]["access_token"] = Value::String(access_token);
            }
        }
    }

    let has_refresh_token = cfg
        .get("oauth")
        .and_then(|v| v.get("refresh_token"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if !has_refresh_token {
        if let Some(refresh_token) = ks.get(&format!("{}.refresh_token", name)) {
            if !refresh_token.is_empty() {
                if !cfg.get("oauth").map(|v| v.is_object()).unwrap_or(false) {
                    cfg["oauth"] = json!({});
                }
                cfg["oauth"]["refresh_token"] = Value::String(refresh_token);
            }
        }
    }

    cfg
}

fn config_string<'a>(config: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = config;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn backend_has_direct_auth(cfg: &Value) -> bool {
    config_string(cfg, &["api_key"]).is_some()
        || config_string(cfg, &["oauth", "access_token"]).is_some()
        || config_string(cfg, &["oauth", "refresh_token"]).is_some()
}

fn codex_auth_path_from_config(cfg: &Value) -> Option<PathBuf> {
    config_string(cfg, &["oauth", "auth_path"]).map(PathBuf::from)
}

fn codex_default_auth_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let home = home.trim();
    if home.is_empty() {
        return None;
    }
    Some(Path::new(home).join(".codex").join("auth.json"))
}

fn codex_auth_file_has_tokens(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };

    doc.get("tokens")
        .and_then(Value::as_object)
        .map(|tokens| {
            ["access_token", "refresh_token"].iter().all(|key| {
                tokens
                    .get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(|value| !value.is_empty())
                    .unwrap_or(false)
            })
        })
        .unwrap_or_else(|| {
            ["access_token", "refresh_token"].iter().all(|key| {
                doc.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(|value| !value.is_empty())
                    .unwrap_or(false)
            })
        })
}

fn backend_has_preferred_auth(name: &str, cfg: &Value) -> bool {
    if backend_has_direct_auth(cfg) {
        return true;
    }

    if name != "openai-codex" {
        return false;
    }

    codex_auth_path_from_config(cfg)
        .or_else(codex_default_auth_path)
        .map(|path| codex_auth_file_has_tokens(&path))
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
struct CircuitBreakerState {
    consecutive_failures: u32,
    last_failure_time: Option<std::time::Instant>,
}

struct BackendCandidate {
    name: String,
    priority: i64,
}

fn sort_backend_candidates(candidates: &mut [BackendCandidate], config: &LlmConfig) {
    candidates.sort_by(|a, b| {
        let p_res = b.priority.cmp(&a.priority);
        if p_res != std::cmp::Ordering::Equal {
            return p_res;
        }

        // Tie-breaker: active_backend > fallback_backends (in array order) > others
        let score = |name: &str| -> i32 {
            if name == config.active_backend {
                1000
            } else if let Some(idx) = config.fallback_backends.iter().position(|r| r == name) {
                900 - (idx as i32)
            } else {
                0
            }
        };
        score(&b.name).cmp(&score(&a.name))
    });
}

fn build_backend_candidates(
    config: &LlmConfig,
    plugin_manager: &crate::llm::plugin_manager::PluginManager,
    ks: &crate::infra::key_store::KeyStore,
) -> Vec<BackendCandidate> {
    let mut candidates = Vec::new();
    let mut all_names: Vec<String> = Vec::new();

    if let Some(obj) = config.backends.as_object() {
        for key in obj.keys() {
            all_names.push(key.clone());
        }
    }
    if !all_names.contains(&config.active_backend) {
        all_names.push(config.active_backend.clone());
    }
    for fb in &config.fallback_backends {
        if !all_names.contains(fb) {
            all_names.push(fb.clone());
        }
    }

    for plugin_name in plugin_manager.available_plugins() {
        if !all_names.contains(&plugin_name) {
            all_names.push(plugin_name);
        }
    }

    for name in all_names {
        let mut priority = 0;
        let mut is_explicitly_in_config = false;
        let effective_cfg = merge_backend_auth(config.backend_config(&name), &name, ks);

        if name == config.active_backend
            || config.fallback_backends.contains(&name)
            || config.backends.get(&name).is_some()
        {
            priority = 1;
            is_explicitly_in_config = true;
        }

        if let Some(p) = config
            .backends
            .get(&name)
            .and_then(|v| v.get("priority"))
            .and_then(|v| v.as_i64())
        {
            priority = p;
            is_explicitly_in_config = true;
        }

        if !is_explicitly_in_config {
            if let Some(cfg) = plugin_manager.get_plugin_config(&name) {
                priority = cfg.get("priority").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }

        if backend_has_preferred_auth(&name, &effective_cfg) {
            // OAuth-backed Codex sessions are more fragile than API-key
            // backends because the daemon may restart while the CLI auth
            // file remains the only surviving credential source. Give any
            // authenticated backend a strong priority lift so valid auth is
            // consumed first instead of silently falling back to a weaker
            // default backend.
            priority = priority.max(AUTHENTICATED_BACKEND_PRIORITY_BOOST);
        }

        candidates.push(BackendCandidate { name, priority });
    }

    sort_backend_candidates(&mut candidates, config);
    candidates
}
