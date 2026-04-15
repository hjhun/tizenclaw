use serde::{Deserialize, Serialize};

use crate::task_packet::{TaskPacket, TaskPriority};
use crate::task_registry::{TaskRegistry, TaskRegistryError};
use crate::trust_resolver::TrustRequirement;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamCronEntry {
    pub entry_id: String,
    pub schedule: String,
    pub task_name: String,
    pub lane_id: String,
    pub enabled: bool,
    pub priority: TaskPriority,
    pub labels: Vec<String>,
    pub trust: Option<TrustRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TeamCronRegistry {
    pub entries: Vec<TeamCronEntry>,
}

impl TeamCronRegistry {
    pub fn register(&mut self, entry: TeamCronEntry) {
        self.entries.push(entry);
        self.entries
            .sort_by(|left, right| left.entry_id.cmp(&right.entry_id));
    }

    pub fn enabled_entries(&self) -> Vec<TeamCronEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled)
            .cloned()
            .collect()
    }

    pub fn emit_task_packet(&self, entry_id: &str) -> Option<TaskPacket> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.entry_id == entry_id && entry.enabled)?;
        let mut packet = TaskPacket::queued(
            format!("cron:{}", entry.entry_id),
            entry.task_name.clone(),
            entry.priority.clone(),
        )
        .with_lane(entry.lane_id.clone())
        .with_labels(entry.labels.clone());
        if let Some(requirement) = entry.trust.clone() {
            packet = packet.with_trust_requirement(requirement);
        }
        Some(packet)
    }

    pub fn sync_into_task_registry(
        &self,
        entry_id: &str,
        task_registry: &TaskRegistry,
    ) -> Result<TaskPacket, TaskRegistryError> {
        let packet =
            self.emit_task_packet(entry_id)
                .ok_or_else(|| TaskRegistryError::UnknownTask {
                    task_id: entry_id.to_string(),
                })?;
        task_registry.register(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_registry::TaskRegistry;
    use crate::trust_resolver::{TrustLevel, TrustRequirement};

    fn cron_entry(entry_id: &str) -> TeamCronEntry {
        TeamCronEntry {
            entry_id: entry_id.to_string(),
            schedule: "0 * * * *".to_string(),
            task_name: "metadata.sync".to_string(),
            lane_id: "maintenance".to_string(),
            enabled: true,
            priority: TaskPriority::Normal,
            labels: vec!["cron".to_string(), "metadata".to_string()],
            trust: Some(TrustRequirement::at_least(TrustLevel::Restricted)),
        }
    }

    #[test]
    fn cron_registry_emits_deterministic_task_packets() {
        let mut registry = TeamCronRegistry::default();
        registry.register(cron_entry("entry-b"));
        registry.register(cron_entry("entry-a"));

        let enabled = registry.enabled_entries();
        assert_eq!(enabled[0].entry_id, "entry-a");
        assert_eq!(enabled[1].entry_id, "entry-b");

        let packet = registry
            .emit_task_packet("entry-a")
            .expect("emit enabled task");
        assert_eq!(packet.task_id, "cron:entry-a");
        assert_eq!(
            packet.assignment.as_ref().map(|a| a.lane_id.as_str()),
            Some("maintenance")
        );
        assert_eq!(
            packet.labels,
            vec!["cron".to_string(), "metadata".to_string()]
        );
    }

    #[test]
    fn cron_registry_can_enqueue_into_task_registry() {
        let mut cron = TeamCronRegistry::default();
        cron.register(cron_entry("entry-sync"));
        let tasks = TaskRegistry::new();

        let packet = cron
            .sync_into_task_registry("entry-sync", &tasks)
            .expect("sync into task registry");

        assert_eq!(packet.task_id, "cron:entry-sync");
        assert_eq!(tasks.snapshot().active_tasks.len(), 1);
    }
}
