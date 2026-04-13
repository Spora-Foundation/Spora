//!
//! User hint is a string that can be stored in the wallet
//! and presented to the user when the wallet opens to help
//! prevent phishing attacks.
//!

use crate::imports::*;
use borsh::{BorshDeserialize, BorshSerialize};
use std::fmt::{Display, Formatter};

#[derive(Default, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(transparent)]
pub struct Hint {
    pub text: String,
}

impl Hint {
    pub fn new(text: String) -> Self {
        Self { text }
    }
}

impl From<&str> for Hint {
    fn from(text: &str) -> Self {
        Self::new(text.to_string())
    }
}

impl From<String> for Hint {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl Display for Hint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_serializes_as_plain_string() {
        let hint = Hint::new("anti-phishing".to_string());
        let value = serde_json::to_value(&hint).expect("hint should serialize");
        assert_eq!(value, serde_json::Value::String("anti-phishing".to_string()));

        let roundtrip: Hint = serde_json::from_value(value).expect("hint should deserialize from string");
        assert_eq!(roundtrip.text, "anti-phishing");
    }
}
