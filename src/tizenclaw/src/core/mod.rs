//! Core module — Full agent engine with all subsystems.

pub mod agent_core;
pub mod tool_dispatcher;
pub mod tool_declaration_builder;
pub mod tool_policy;
pub mod prompt_builder;
pub mod skill_support;
pub mod textual_skill_scanner;

pub mod intent_analyzer;
pub mod tool_indexer;
pub mod ipc_server;
pub mod event_bus;
pub mod safety_guard;
pub mod task_scheduler;
pub mod offline_fallback;
pub mod system_context_provider;
pub mod user_profile_store;
// Batch 2: Skill & Plugin
pub mod tool_watcher;
pub mod cli_plugin_manager;
pub mod system_cli_adapter;
pub mod capability_registry;
// Batch 3: Agent Framework
pub mod agent_role;
pub mod agent_factory;
pub mod workflow_engine;
pub mod pipeline_executor;
pub use crate::tizen::core::action_bridge;
pub mod autonomous_trigger;
pub mod swarm_manager;
pub mod proactive_advisor;
pub mod auto_skill_agent;
pub mod context_fusion_engine;
pub mod fallback_parser;
pub mod context_engine;
pub mod agent_loop_state;
pub mod device_profiler;
pub mod perception_engine;
// Embedding Engine
pub mod wordpiece_tokenizer;
pub mod on_device_embedding;
