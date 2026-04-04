//! Infrastructure compatibility namespace.
//!
//! Re-exports modules from `crate::generic::infra` and
//! `crate::tizen::infra` so existing call sites can keep using
//! `crate::infra::*` during migration.

pub use crate::generic::infra::canvas_ipc_server;
pub use crate::generic::infra::container_engine;
pub use crate::generic::infra::dbus_probe;
pub use crate::generic::infra::fleet_agent;
pub use crate::generic::infra::health_monitor;
pub use crate::generic::infra::http_client;
pub use crate::generic::infra::key_store;
pub use crate::generic::infra::onnx_runtime;
pub use crate::generic::infra::ota_updater;
pub use crate::generic::infra::tunnel_manager;

pub use crate::tizen::infra::app_lifecycle_adapter;
pub use crate::tizen::infra::package_event_adapter;
pub use crate::tizen::infra::pkgmgr_client;
pub use crate::tizen::infra::recent_app_adapter;
pub use crate::tizen::infra::tizen_system_event_adapter;
