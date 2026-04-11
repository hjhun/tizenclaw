use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SummaryCompressionResult {
    pub original_items: usize,
    pub retained_items: usize,
    pub summary: String,
}
