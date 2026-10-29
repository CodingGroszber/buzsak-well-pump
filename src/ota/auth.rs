use base64::{Engine, engine::general_purpose::STANDARD};

use crate::secrets::{OTA_PASSWORD, OTA_USER};

/// HTTP Basic auth against the OTA credentials in `secrets`.
pub struct Auth {
    expected: String,
}

impl Auth {
    pub fn from_secrets() -> Self {
        Self {
            expected: format!(
                "Basic {}",
                STANDARD.encode(format!("{OTA_USER}:{OTA_PASSWORD}"))
            ),
        }
    }

    pub fn allows(&self, authorization_header: Option<&str>) -> bool {
        authorization_header
            .is_some_and(|got| constant_time_eq(got.as_bytes(), self.expected.as_bytes()))
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
