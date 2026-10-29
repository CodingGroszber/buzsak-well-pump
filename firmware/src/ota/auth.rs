//! HTTP Basic authentication for the OTA routes.

use base64::{Engine, engine::general_purpose::STANDARD};
use well_core::ota_policy::constant_time_eq;

use crate::secrets::{OTA_PASSWORD, OTA_USER};

/// HTTP Basic auth against the OTA credentials in `secrets`.
pub struct Auth {
    /// The complete expected `Authorization` header value, e.g. `Basic dXNlcjpwYXNz`.
    expected: String,
}

impl Auth {
    /// Builds the expected header value from the OTA credentials in `secrets`.
    ///
    /// # Returns
    /// An authenticator that accepts exactly that user and password.
    pub fn from_secrets() -> Self {
        Self {
            expected: format!(
                "Basic {}",
                STANDARD.encode(format!("{OTA_USER}:{OTA_PASSWORD}"))
            ),
        }
    }

    /// Checks a request's `Authorization` header in constant time.
    ///
    /// # Arguments
    /// * `authorization_header` - The header value, or `None` when the request has none.
    ///
    /// # Returns
    /// `true` only when the header matches the expected credentials.
    pub fn allows(&self, authorization_header: Option<&str>) -> bool {
        authorization_header
            .is_some_and(|got| constant_time_eq(got.as_bytes(), self.expected.as_bytes()))
    }
}
