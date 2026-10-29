use esp_idf_svc::sys::EspError;

/// Why an upload was refused: HTTP status and a short message for the client.
pub struct Rejection(pub u16, pub &'static str);

impl From<anyhow::Error> for Rejection {
    fn from(err: anyhow::Error) -> Self {
        log::error!("OTA internal error: {err:#}");
        Rejection(500, "internal error")
    }
}

impl From<EspError> for Rejection {
    fn from(err: EspError) -> Self {
        log::error!("OTA flash/driver error: {err}");
        Rejection(500, "flash or driver error")
    }
}
