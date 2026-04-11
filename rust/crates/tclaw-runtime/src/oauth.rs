use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OAuthProvider {
    OpenAi,
    Github,
    Google,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OAuthState {
    pub provider: Option<OAuthProvider>,
    pub account_email: Option<String>,
    pub access_token_present: bool,
    pub refresh_token_present: bool,
}
