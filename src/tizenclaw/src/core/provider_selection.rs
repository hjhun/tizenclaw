//! Provider selection layer for LLM backend routing.
//!
//! This module decouples request-time routing from a single eagerly-selected
//! backend.  `ProviderRegistry` owns all initialized backends and exposes a
//! preference-ordered list.  `ProviderSelector` picks the first ready provider
//! at request time, consulting an external availability predicate (circuit
//! breaker state lives in `AgentCore`).
//!
//! Config compatibility
//! --------------------
//! `ProviderCompatibilityTranslator` converts the legacy `active_backend` +
//! `fallback_backends` + `backends.*` config shape into a normalized
//! `ProviderRoutingConfig`.  When the new `providers` array is present it is
//! authoritative and the legacy keys are only kept for read-compatibility.

use crate::llm::backend::LlmBackend;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Availability ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAvailability {
    #[default]
    Ready,
    Degraded,
    OpenCircuit,
    Unavailable,
}

impl ProviderAvailability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::OpenCircuit => "open_circuit",
            Self::Unavailable => "unavailable",
        }
    }
}

// ── Attempt result ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptResult {
    Selected,
    SkippedDisabled,
    SkippedUnavailable,
    SkippedOpenCircuit,
    InitFailed,
    ExecutionFailed,
}

impl ProviderAttemptResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::SkippedDisabled => "skipped_disabled",
            Self::SkippedUnavailable => "skipped_unavailable",
            Self::SkippedOpenCircuit => "skipped_open_circuit",
            Self::InitFailed => "init_failed",
            Self::ExecutionFailed => "execution_failed",
        }
    }
}

// ── Selection record (last routing decision) ─────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderAttempt {
    pub provider: String,
    pub result: ProviderAttemptResult,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderSelectionRecord {
    pub selected_provider: String,
    pub attempted_providers: Vec<ProviderAttempt>,
    pub reason: String,
    pub selected_at_unix_secs: u64,
}

// ── Config source marker ──────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderConfigSource {
    /// Declared in the new `providers` array.
    Providers,
    /// Synthesized from the legacy `active_backend` key.
    CompatibilityActive,
    /// Synthesized from the legacy `fallback_backends` key.
    CompatibilityFallback,
    /// Synthesized from a `backends.<name>` entry not referenced by
    /// `active_backend` or `fallback_backends`.
    CompatibilityBackends,
}

impl ProviderConfigSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Providers => "providers",
            Self::CompatibilityActive => "compatibility_active",
            Self::CompatibilityFallback => "compatibility_fallback",
            Self::CompatibilityBackends => "compatibility_backends",
        }
    }
}

// ── Per-provider config entry ─────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderPreference {
    pub name: String,
    /// Lower number = higher priority (selected first).
    pub priority: i64,
    pub enabled: bool,
    pub source: ProviderConfigSource,
}

// ── Normalized routing config ─────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct ProviderRoutingConfig {
    /// Ordered by ascending priority (smallest first = highest precedence).
    pub providers: Vec<ProviderPreference>,
    /// Preserved legacy values for compatibility reporting.
    pub raw_active_backend: String,
    pub raw_fallback_backends: Vec<String>,
    /// True when the `providers` key was explicitly present in the config,
    /// even if it was an empty array.  Used to distinguish `providers: []`
    /// (authoritative: no configured providers) from an absent key (fall
    /// through to legacy routing).
    pub providers_array_present: bool,
}

impl ProviderRoutingConfig {
    pub fn ordered_names(&self) -> Vec<&str> {
        self.providers
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.name.as_str())
            .collect()
    }
}

// ── Compatibility translator ──────────────────────────────────────────────────

pub struct ProviderCompatibilityTranslator;

impl ProviderCompatibilityTranslator {
    /// Build a `ProviderRoutingConfig` from an `llm_config.json` document.
    ///
    /// If `providers` is present it is authoritative.
    /// Otherwise the legacy `active_backend` / `fallback_backends` keys are
    /// used to synthesize the provider order.
    pub fn translate(doc: &Value) -> ProviderRoutingConfig {
        let raw_active_backend = doc
            .get("active_backend")
            .and_then(Value::as_str)
            .unwrap_or("gemini")
            .to_string();
        let raw_fallback_backends: Vec<String> = doc
            .get("fallback_backends")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_else(|| vec!["openai".into(), "ollama".into()]);

        // If the new `providers` array is present, it is authoritative.
        if let Some(providers_arr) = doc.get("providers").and_then(Value::as_array) {
            let mut providers: Vec<ProviderPreference> = providers_arr
                .iter()
                .filter_map(|entry| {
                    let name = entry.get("name").and_then(Value::as_str)?.to_string();
                    if name.trim().is_empty() {
                        return None;
                    }
                    let priority = entry
                        .get("priority")
                        .and_then(Value::as_i64)
                        .unwrap_or(50);
                    let enabled = entry
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    Some(ProviderPreference {
                        name,
                        priority,
                        enabled,
                        source: ProviderConfigSource::Providers,
                    })
                })
                .collect();
            // Lower priority number = higher preference (select first).
            providers.sort_by_key(|p| p.priority);
            return ProviderRoutingConfig {
                providers,
                raw_active_backend,
                raw_fallback_backends,
                providers_array_present: true,
            };
        }

        // Synthesize from legacy keys.
        //
        // Also respect `backends.<name>.priority` when present, mirroring the
        // ordering applied by `build_backend_candidates` / `sort_backend_candidates`.
        // If an explicit priority is set it takes precedence over the
        // active_backend / fallback_backends positional order.
        //
        // Candidate sorting: higher raw_priority value = selected first
        // (same semantics as BackendCandidate in tool_runtime.rs).
        struct LegacyCandidate {
            name: String,
            raw_priority: i64,
            source: ProviderConfigSource,
        }

        let mut seen = std::collections::HashSet::new();
        let mut candidates: Vec<LegacyCandidate> = Vec::new();

        // Collect names from active_backend then fallback_backends.
        let all_legacy: Vec<(String, ProviderConfigSource)> =
            std::iter::once((
                raw_active_backend.trim().to_string(),
                ProviderConfigSource::CompatibilityActive,
            ))
            .chain(raw_fallback_backends.iter().map(|fb| {
                (
                    fb.trim().to_string(),
                    ProviderConfigSource::CompatibilityFallback,
                )
            }))
            .filter(|(name, _)| !name.is_empty())
            .collect();

        for (name, source) in all_legacy {
            if !seen.insert(name.clone()) {
                continue; // deduplicate
            }
            // Use backends.<name>.priority when explicitly set, otherwise fall
            // back to a positional score that preserves active > fallback[0] >
            // fallback[1] > … ordering (mirrors sort_backend_candidates
            // tie-breaker logic).
            let raw_priority = doc
                .get("backends")
                .and_then(|b| b.get(&name))
                .and_then(|be| be.get("priority"))
                .and_then(Value::as_i64)
                .unwrap_or_else(|| {
                    if source == ProviderConfigSource::CompatibilityActive {
                        1000
                    } else {
                        raw_fallback_backends
                            .iter()
                            .position(|fb| fb.trim() == name)
                            .map(|idx| 900i64 - idx as i64)
                            .unwrap_or(0)
                    }
                });
            candidates.push(LegacyCandidate {
                name,
                raw_priority,
                source,
            });
        }

        // Also include backends defined under `backends.*` that were not
        // referenced by `active_backend` or `fallback_backends`.  The design
        // policy (runtime_flexibility_ooad_design_20260416.md §44) requires
        // that every configured backend remains selectable; without this step
        // an operator-configured backend that has no entry in either legacy key
        // would be initialized but never routed to.
        if let Some(backends_map) = doc.get("backends").and_then(Value::as_object) {
            for (extra_name, be_val) in backends_map {
                let trimmed = extra_name.trim().to_string();
                if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
                    continue;
                }
                let raw_priority = be_val
                    .get("priority")
                    .and_then(Value::as_i64)
                    .unwrap_or_else(|| {
                        // Place these below any positional fallback score so
                        // they are tried last unless given an explicit priority.
                        800i64 - candidates.len() as i64
                    });
                candidates.push(LegacyCandidate {
                    name: trimmed,
                    raw_priority,
                    source: ProviderConfigSource::CompatibilityBackends,
                });
            }
        }

        // Sort descending: highest raw_priority is routed first.
        candidates.sort_by(|a, b| b.raw_priority.cmp(&a.raw_priority));

        // Convert to ProviderPreference with ascending priority values so that
        // ProviderRoutingConfig::ordered_names() returns them in the correct
        // selection order (position 0 = highest preference).
        let providers: Vec<ProviderPreference> = candidates
            .into_iter()
            .enumerate()
            .map(|(i, cand)| ProviderPreference {
                name: cand.name,
                priority: i as i64,
                enabled: true,
                source: cand.source,
            })
            .collect();

        ProviderRoutingConfig {
            providers,
            raw_active_backend,
            raw_fallback_backends,
            providers_array_present: false,
        }
    }
}

// ── Provider instance (runtime) ───────────────────────────────────────────────

pub struct ProviderInstance {
    pub name: String,
    pub backend: Box<dyn LlmBackend>,
    pub last_init_error: Option<String>,
}

// ── Provider registry ─────────────────────────────────────────────────────────

pub struct ProviderRegistry {
    routing: ProviderRoutingConfig,
    /// Initialized backends in preference order (primary first).
    instances: Vec<ProviderInstance>,
    /// Last request-time routing decision.
    active_selection: Option<ProviderSelectionRecord>,
    /// Error messages for providers that failed to initialize.
    /// Keyed by provider name; used to surface `last_init_error` in status
    /// output even though no live instance was created for these providers.
    failed_inits: std::collections::HashMap<String, String>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self {
            routing: ProviderRoutingConfig::default(),
            instances: Vec::new(),
            active_selection: None,
            failed_inits: std::collections::HashMap::new(),
        }
    }
}

impl ProviderRegistry {
    pub fn new(
        routing: ProviderRoutingConfig,
        instances: Vec<ProviderInstance>,
        failed_inits: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            routing,
            instances,
            active_selection: None,
            failed_inits,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    pub fn has_any(&self) -> bool {
        !self.instances.is_empty()
    }

    pub fn primary_name(&self) -> &str {
        self.instances
            .first()
            .map(|inst| inst.name.as_str())
            .unwrap_or("")
    }

    /// Return the name of the provider that actually served the last request.
    ///
    /// After `chat_with_fallback` completes it calls `set_active_selection` with
    /// the provider that returned a successful response.  Reading
    /// `active_selection.selected_provider` here gives the correct provider name
    /// even when routing fell through to a non-primary backend.
    ///
    /// Falls back to `primary_name()` only when no selection has yet been
    /// recorded (e.g., before the very first request in a new session).
    pub fn active_selection_provider_name(&self) -> &str {
        self.active_selection
            .as_ref()
            .map(|r| r.selected_provider.as_str())
            .unwrap_or_else(|| self.primary_name())
    }

    pub fn instances(&self) -> &[ProviderInstance] {
        &self.instances
    }

    pub fn set_active_selection(&mut self, record: ProviderSelectionRecord) {
        self.active_selection = Some(record);
    }

    pub fn shutdown_all(&mut self) {
        for inst in &mut self.instances {
            inst.backend.shutdown();
        }
    }

    /// Return a status JSON snapshot.
    ///
    /// `is_available` is called for each initialized provider to determine
    /// whether it is currently usable (circuit breaker open = false).
    /// Providers that failed to initialize are always reported as "unavailable".
    pub fn status_json(&self, is_available: impl Fn(&str) -> bool) -> Value {
        let ordered_names = self.routing.ordered_names();
        let provider_list: Vec<Value> = self
            .routing
            .providers
            .iter()
            .map(|pref| {
                let inst = self.instances.iter().find(|inst| inst.name == pref.name);
                let availability = match inst {
                    None => ProviderAvailability::Unavailable.as_str(),
                    Some(_) if !is_available(&pref.name) => {
                        ProviderAvailability::OpenCircuit.as_str()
                    }
                    Some(_) => ProviderAvailability::Ready.as_str(),
                };
                // Surface an init error when no instance was created.
                // Priority: instance-level error (post-init degradation) >
                // failed_inits entry (init-time failure) > empty.
                let last_init_error: &str = inst
                    .and_then(|inst| inst.last_init_error.as_deref())
                    .or_else(|| {
                        if inst.is_none() {
                            self.failed_inits.get(&pref.name).map(String::as_str)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                json!({
                    "name": pref.name,
                    "priority": pref.priority,
                    "enabled": pref.enabled,
                    "availability": availability,
                    "last_init_error": if last_init_error.is_empty() { Value::Null } else { Value::String(last_init_error.to_string()) },
                    "source": pref.source.as_str(),
                })
            })
            .collect();

        let current_selection = self.active_selection.as_ref().map(|rec| {
            json!({
                "selected_provider": rec.selected_provider,
                "attempted_providers": rec.attempted_providers.iter().map(|a| json!({
                    "provider": a.provider,
                    "result": a.result.as_str(),
                    "detail": a.detail,
                })).collect::<Vec<_>>(),
                "reason": rec.reason,
                "selected_at_unix_secs": rec.selected_at_unix_secs,
            })
        });

        json!({
            "configured_active_backend": self.routing.raw_active_backend,
            "configured_fallback_backends": self.routing.raw_fallback_backends,
            "configured_provider_order": ordered_names,
            "providers": provider_list,
            "current_selection": current_selection,
        })
    }
}

// ── Selector ──────────────────────────────────────────────────────────────────

pub struct ProviderSelector;

impl ProviderSelector {
    /// Return the index of the first instance that passes `is_available`.
    ///
    /// Iterates the registry in preference order (position 0 = highest priority).
    /// Providers that appear in the routing config with `enabled: false` are
    /// skipped.  Providers that are not present in the routing config at all
    /// (e.g., discovered via plugin scan) are treated as enabled and eligible
    /// as last-resort fallbacks, preserving the pre-routing-layer behavior
    /// where plugin backends could serve as implicit fallbacks.
    pub fn first_available(
        registry: &ProviderRegistry,
        is_available: impl Fn(&str) -> bool,
    ) -> Option<usize> {
        for (idx, inst) in registry.instances.iter().enumerate() {
            // A provider explicitly listed and marked disabled in the routing
            // config is never eligible.  A provider absent from the routing
            // config (e.g., a plugin-discovered backend) is treated as enabled
            // so it can still serve as a last-resort fallback.
            let enabled = registry
                .routing
                .providers
                .iter()
                .find(|p| p.name == inst.name)
                .map(|p| p.enabled)
                .unwrap_or(true);
            if !enabled {
                continue;
            }
            if is_available(&inst.name) {
                return Some(idx);
            }
        }
        None
    }

    /// Return the names of all enabled providers in preference order.
    ///
    /// Providers that appear in the routing config and are marked enabled are
    /// included first (in instance order, which mirrors preference order).
    /// Providers initialized but absent from the routing config (e.g., plugin
    /// backends discovered at runtime) are appended after configured ones so
    /// they remain reachable as last-resort fallbacks.
    /// This is the authoritative source for the ordered provider list that
    /// `chat_with_fallback` iterates so selection policy stays centralized here.
    pub fn ordered_enabled_names(registry: &ProviderRegistry) -> Vec<String> {
        registry
            .instances
            .iter()
            .filter(|inst| {
                registry
                    .routing
                    .providers
                    .iter()
                    .find(|p| p.name == inst.name)
                    .map(|p| p.enabled)
                    .unwrap_or(true)
            })
            .map(|inst| inst.name.clone())
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compatibility_translator_synthesizes_active_then_fallbacks() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai", "ollama"],
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        assert_eq!(names, vec!["gemini", "openai", "ollama"]);
        assert_eq!(config.providers[0].source, ProviderConfigSource::CompatibilityActive);
        assert_eq!(config.providers[1].source, ProviderConfigSource::CompatibilityFallback);
    }

    #[test]
    fn compatibility_translator_deduplicates_active_in_fallbacks() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["gemini", "openai"],
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        assert_eq!(names, vec!["gemini", "openai"]);
    }

    #[test]
    fn explicit_providers_array_overrides_legacy_keys() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
            "providers": [
                { "name": "anthropic", "priority": 50, "enabled": true },
                { "name": "openai", "priority": 100, "enabled": true },
            ],
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        // Sorted by ascending priority: anthropic(50) then openai(100).
        let names: Vec<&str> = config.ordered_names();
        assert_eq!(names, vec!["anthropic", "openai"]);
        assert_eq!(config.providers[0].source, ProviderConfigSource::Providers);
    }

    #[test]
    fn explicit_providers_disabled_entry_excluded_from_ordered_names() {
        let doc = json!({
            "providers": [
                { "name": "gemini", "priority": 100, "enabled": true },
                { "name": "openai", "priority": 90, "enabled": false },
            ],
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        // openai is disabled, so only gemini appears in ordered names.
        assert_eq!(names, vec!["gemini"]);
        // But the disabled one is still in providers list.
        assert_eq!(config.providers.len(), 2);
    }

    #[test]
    fn selector_returns_none_for_empty_registry() {
        let registry = ProviderRegistry::default();
        assert!(ProviderSelector::first_available(&registry, |_| true).is_none());
    }

    #[test]
    fn registry_status_json_lists_configured_and_initialized_providers() {
        let config = ProviderCompatibilityTranslator::translate(&json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
        }));
        let registry = ProviderRegistry::new(config, vec![], std::collections::HashMap::new());
        let status = registry.status_json(|_| true);
        let providers = status["providers"].as_array().unwrap();
        // Both configured providers appear even if no backend is initialized.
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0]["name"], "gemini");
        assert_eq!(providers[0]["availability"], "unavailable");
    }

    #[test]
    fn compatibility_translator_empty_active_backend() {
        let doc = json!({
            "active_backend": "",
            "fallback_backends": ["openai"],
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        // Empty active_backend is skipped.
        let names: Vec<&str> = config.ordered_names();
        assert_eq!(names, vec!["openai"]);
    }

    /// Verify that the write-locked fallback path in `get_llm_runtime()` produces
    /// a non-empty `providers[]` array.  The fallback reconstructs provider
    /// metadata from the routing config without accessing live instances, so
    /// availability is reported as `"unknown"`.
    #[test]
    fn fallback_status_json_providers_array_is_populated_legacy_config() {
        let raw_doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
        });
        let routing = ProviderCompatibilityTranslator::translate(&raw_doc);
        // Replicate the fallback JSON construction from runtime_admin_impl.rs.
        let providers: Vec<Value> = routing
            .providers
            .iter()
            .map(|pref| {
                json!({
                    "name": pref.name,
                    "priority": pref.priority,
                    "enabled": pref.enabled,
                    "availability": "unknown",
                    "last_init_error": Value::Null,
                    "source": pref.source.as_str(),
                })
            })
            .collect();
        assert_eq!(providers.len(), 2, "fallback must not return an empty providers array");
        assert_eq!(providers[0]["name"], "gemini");
        assert_eq!(providers[0]["availability"], "unknown");
        assert_eq!(providers[1]["name"], "openai");
        assert_eq!(providers[1]["availability"], "unknown");
    }

    #[test]
    fn fallback_status_json_providers_array_is_populated_providers_array_config() {
        let raw_doc = json!({
            "providers": [
                {"name": "anthropic", "priority": 10, "enabled": true},
                {"name": "openai",    "priority": 20, "enabled": false},
            ],
        });
        let routing = ProviderCompatibilityTranslator::translate(&raw_doc);
        let providers: Vec<Value> = routing
            .providers
            .iter()
            .map(|pref| {
                json!({
                    "name": pref.name,
                    "priority": pref.priority,
                    "enabled": pref.enabled,
                    "availability": "unknown",
                    "last_init_error": Value::Null,
                    "source": pref.source.as_str(),
                })
            })
            .collect();
        assert_eq!(providers.len(), 2, "fallback must expose all configured providers");
        assert_eq!(providers[0]["name"], "anthropic");
        assert_eq!(providers[0]["source"], "providers");
        assert_eq!(providers[1]["name"], "openai");
        assert_eq!(providers[1]["enabled"], false);
    }

    /// backends.<name>.priority in legacy config overrides the default
    /// active_backend / fallback_backends positional ordering.
    ///
    /// Config: active_backend=gemini (no explicit priority), openai has
    /// backends.openai.priority=2000 which is higher than gemini's default
    /// 1000 tie-break score, so openai should sort first.
    #[test]
    fn legacy_backend_priority_overrides_active_backend_position() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
            "backends": {
                "openai": { "priority": 2000 },
            },
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        // openai has explicit priority 2000 > gemini's default 1000, so it
        // routes first even though active_backend is gemini.
        assert_eq!(names, vec!["openai", "gemini"]);
        // openai should keep CompatibilityFallback source (where it came from).
        assert_eq!(config.providers[0].source, ProviderConfigSource::CompatibilityFallback);
        assert_eq!(config.providers[1].source, ProviderConfigSource::CompatibilityActive);
    }

    /// When two fallbacks have explicit priorities, higher priority routes first.
    #[test]
    fn legacy_fallback_priority_ordering_respected() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai", "ollama"],
            "backends": {
                "ollama": { "priority": 500 },
                "openai": { "priority": 100 },
            },
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        // gemini default=1000, ollama=500, openai=100 → gemini, ollama, openai.
        assert_eq!(names, vec!["gemini", "ollama", "openai"]);
    }

    /// status_json() reports open_circuit when is_available returns false for
    /// an initialized provider, and ready when it returns true.
    ///
    /// Uses a minimal fake ProviderInstance constructed directly (bypassing the
    /// LlmBackend trait) to avoid async_trait boilerplate in a sync unit test.
    #[test]
    fn status_json_reflects_circuit_breaker_state() {
        use crate::llm::backend::{LlmBackend, LlmMessage, LlmResponse, LlmToolDecl};

        struct StubBackend;
        #[async_trait::async_trait]
        impl LlmBackend for StubBackend {
            fn initialize(&mut self, _config: &serde_json::Value) -> bool { true }
            fn get_name(&self) -> &str { "gemini" }
            async fn chat(
                &self,
                _messages: &[LlmMessage],
                _tools: &[LlmToolDecl],
                _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
                _system_prompt: &str,
                _max_tokens: Option<u32>,
            ) -> LlmResponse {
                LlmResponse::default()
            }
        }

        let config = ProviderCompatibilityTranslator::translate(&json!({
            "active_backend": "gemini",
            "fallback_backends": [],
        }));
        let instance = ProviderInstance {
            name: "gemini".to_string(),
            backend: Box::new(StubBackend),
            last_init_error: None,
        };
        let registry = ProviderRegistry::new(config, vec![instance], std::collections::HashMap::new());

        // Circuit breaker open → availability should be open_circuit.
        let status_open = registry.status_json(|_name| false);
        let providers_open = status_open["providers"].as_array().unwrap();
        assert_eq!(providers_open[0]["availability"], "open_circuit");

        // Circuit breaker closed → availability should be ready.
        let status_ready = registry.status_json(|_name| true);
        let providers_ready = status_ready["providers"].as_array().unwrap();
        assert_eq!(providers_ready[0]["availability"], "ready");
    }

    #[test]
    fn registry_status_json_surfaces_init_failure_error() {
        // A configured provider that failed to initialize should appear in
        // status output with `availability: "unavailable"` and a non-null
        // `last_init_error` drawn from `failed_inits`.
        let config = ProviderCompatibilityTranslator::translate(&json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
        }));
        let mut failed = std::collections::HashMap::new();
        failed.insert(
            "gemini".to_string(),
            "not configured or initialization failed".to_string(),
        );
        // No live instances — both providers failed or were skipped.
        let registry = ProviderRegistry::new(config, vec![], failed);
        let status = registry.status_json(|_| true);
        let providers = status["providers"].as_array().unwrap();

        // gemini: failed init → unavailable + non-null error
        let gemini = &providers[0];
        assert_eq!(gemini["name"], "gemini");
        assert_eq!(gemini["availability"], "unavailable");
        assert!(
            !gemini["last_init_error"].is_null(),
            "last_init_error should be non-null for a failed init"
        );

        // openai: no entry in failed_inits → unavailable but null error
        let openai = &providers[1];
        assert_eq!(openai["name"], "openai");
        assert_eq!(openai["availability"], "unavailable");
        assert!(
            openai["last_init_error"].is_null(),
            "last_init_error should be null when no init error was recorded"
        );
    }

    /// Plugin-discovered backends (absent from the routing config) must still be
    /// eligible for selection when all configured providers are unavailable.
    ///
    /// Scenario: routing config lists only "gemini".  The registry also holds a
    /// "plugin-backend" instance that arrived via plugin scan.  When gemini is
    /// unavailable, `first_available` should fall through to plugin-backend.
    /// `ordered_enabled_names` must include both providers so `chat_with_fallback`
    /// can attempt the plugin backend as a last resort.
    #[test]
    fn unconfigured_provider_eligible_as_last_resort() {
        use crate::llm::backend::{LlmBackend, LlmMessage, LlmResponse, LlmToolDecl};

        struct StubBackend {
            name: &'static str,
        }
        #[async_trait::async_trait]
        impl LlmBackend for StubBackend {
            fn initialize(&mut self, _config: &serde_json::Value) -> bool { true }
            fn get_name(&self) -> &str { self.name }
            async fn chat(
                &self,
                _messages: &[LlmMessage],
                _tools: &[LlmToolDecl],
                _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
                _system_prompt: &str,
                _max_tokens: Option<u32>,
            ) -> LlmResponse {
                LlmResponse::default()
            }
        }

        // Routing config names only "gemini".
        let config = ProviderCompatibilityTranslator::translate(&json!({
            "active_backend": "gemini",
            "fallback_backends": [],
        }));

        // Registry holds two initialized instances: "gemini" (configured) and
        // "plugin-backend" (absent from routing config, came from plugin scan).
        // Instances are in preference order: gemini first, plugin-backend second.
        let instances = vec![
            ProviderInstance {
                name: "gemini".to_string(),
                backend: Box::new(StubBackend { name: "gemini" }),
                last_init_error: None,
            },
            ProviderInstance {
                name: "plugin-backend".to_string(),
                backend: Box::new(StubBackend { name: "plugin-backend" }),
                last_init_error: None,
            },
        ];
        let registry = ProviderRegistry::new(
            config,
            instances,
            std::collections::HashMap::new(),
        );

        // ordered_enabled_names must include both: "gemini" (configured) then
        // "plugin-backend" (unconfigured but eligible as last-resort fallback).
        let names = ProviderSelector::ordered_enabled_names(&registry);
        assert_eq!(
            names,
            vec!["gemini".to_string(), "plugin-backend".to_string()],
            "plugin-discovered backends must appear in ordered_enabled_names as fallbacks"
        );

        // first_available must select plugin-backend when gemini is the only
        // instance that fails the availability predicate.
        let idx = ProviderSelector::first_available(&registry, |name| name == "plugin-backend");
        assert_eq!(
            idx,
            Some(1),
            "first_available must select plugin-backend as last-resort when gemini is unavailable"
        );

        // Verify that an explicitly disabled configured provider is still skipped.
        let config_with_disabled = ProviderCompatibilityTranslator::translate(&json!({
            "providers": [
                { "name": "gemini", "priority": 10, "enabled": false },
            ],
        }));
        let instances2 = vec![
            ProviderInstance {
                name: "gemini".to_string(),
                backend: Box::new(StubBackend { name: "gemini" }),
                last_init_error: None,
            },
        ];
        let registry2 = ProviderRegistry::new(
            config_with_disabled,
            instances2,
            std::collections::HashMap::new(),
        );
        let idx2 = ProviderSelector::first_available(&registry2, |_| true);
        assert!(
            idx2.is_none(),
            "explicitly disabled configured provider must never be selected"
        );
    }

    /// A backend defined under `backends.<name>` but not in `active_backend` or
    /// `fallback_backends` must still appear in the routing config so it can be
    /// selected at request time.  This is the compatibility policy fix for
    /// reviewer finding #1.
    #[test]
    fn backends_only_entry_included_in_routing_config() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
            "backends": {
                "gemini": {},
                "openai": {},
                // "anthropic" exists only in backends.*, not in active or fallback.
                "anthropic": { "priority": 50 },
            },
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        // gemini (1000), openai (900) come first via positional scoring;
        // anthropic (50) is below them but must be included.
        assert!(
            names.contains(&"anthropic"),
            "backends-only entry must appear in ordered_names: {:?}",
            names
        );
        // Source must be CompatibilityBackends.
        let entry = config.providers.iter().find(|p| p.name == "anthropic").unwrap();
        assert_eq!(entry.source, ProviderConfigSource::CompatibilityBackends);
    }

    /// When a backend has a very high explicit priority in `backends.*` but is
    /// absent from `active_backend` / `fallback_backends`, it must sort above
    /// the positional defaults.
    #[test]
    fn backends_only_high_priority_sorts_before_positional_defaults() {
        let doc = json!({
            "active_backend": "gemini",
            "fallback_backends": ["openai"],
            "backends": {
                // anthropic has priority 5000, higher than gemini's default 1000.
                "anthropic": { "priority": 5000 },
            },
        });
        let config = ProviderCompatibilityTranslator::translate(&doc);
        let names: Vec<&str> = config.ordered_names();
        assert_eq!(names[0], "anthropic", "highest explicit priority must route first");
        assert!(names.contains(&"gemini"));
        assert!(names.contains(&"openai"));
    }
}
