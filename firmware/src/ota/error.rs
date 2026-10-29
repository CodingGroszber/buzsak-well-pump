//! The rejection type of the upload path.

use esp_idf_svc::sys::EspError;

/// Why an upload was refused: the HTTP status and a short message for the client.
///
/// The first field is the HTTP status code, the second the human-readable reason that ends up
/// in the JSON error response.
pub struct Rejection(pub u16, pub &'static str);

impl From<anyhow::Error> for Rejection {
    /// Logs the internal error and turns it into a generic `500`.
    ///
    /// # Arguments
    /// * `err` - The internal error.
    ///
    /// # Returns
    /// A `500` rejection that does not leak details to the client.
    fn from(err: anyhow::Error) -> Self {
        log::error!("OTA internal error: {err:#}");
        Rejection(500, "internal error")
    }
}

impl From<EspError> for Rejection {
    /// Logs the flash or driver error and turns it into a generic `500`.
    ///
    /// # Arguments
    /// * `err` - The ESP-IDF error.
    ///
    /// # Returns
    /// A `500` rejection that does not leak details to the client.
    fn from(err: EspError) -> Self {
        log::error!("OTA flash/driver error: {err}");
        Rejection(500, "flash or driver error")
    }
}
