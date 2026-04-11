use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RecoveryRecipe {
    pub recipe_name: String,
    pub trigger: String,
    pub steps: Vec<String>,
}
