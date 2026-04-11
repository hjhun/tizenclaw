use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GreenContract {
    pub contract_name: String,
    pub required_checks: Vec<String>,
    pub last_verified_by: Option<String>,
}
