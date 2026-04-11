//! Core module — Full agent engine with all subsystems.

pub mod agent_core;
pub mod prompt_builder;
pub mod registration_store;
pub mod runtime_capabilities;
pub mod runtime_paths;
pub mod skill_support;
pub mod textual_skill_scanner;
pub mod tool_declaration_builder;
pub mod tool_dispatcher;
pub mod tool_policy;

pub mod event_bus;
pub mod intent_analyzer;
pub mod ipc_server;
pub mod llm_config_store;
pub mod offline_fallback;
pub mod safety_guard;
pub mod system_context_provider;
pub mod task_scheduler;
pub mod tool_indexer;
pub mod user_profile_store;
// Batch 2: Skill & Plugin
pub mod capability_registry;
pub mod cli_plugin_manager;
pub mod devel_mode;
pub mod system_cli_adapter;
pub mod tool_watcher;
// Batch 3: Agent Framework
pub mod agent_factory;
pub mod agent_role;
pub mod pipeline_executor;
pub mod workflow_engine;
pub use crate::tizen::core::action_bridge;
pub mod agent_loop_state;
pub mod auto_skill_agent;
pub mod autonomous_trigger;
pub mod context_engine;
pub mod context_fusion_engine;
pub mod device_profiler;
pub mod fallback_parser;
pub mod feature_tools;
pub mod perception_engine;
pub mod proactive_advisor;
pub mod swarm_manager;
// Embedding Engine
pub mod on_device_embedding;
pub mod wordpiece_tokenizer;
