//! The one HTTP server shared by every feature; features add their routes to it.

use anyhow::Result;
use esp_idf_svc::http::server::{Configuration, EspHttpServer};

/// Creates and starts the shared HTTP server on port 80.
///
/// The stack is sized for the OTA upload handler. Under load the oldest idle connection gives
/// way, so a client that keeps polling cannot starve an upload.
///
/// # Returns
/// The running server, to which features register their routes; it stops when dropped.
pub fn new_server() -> Result<EspHttpServer<'static>> {
    Ok(EspHttpServer::new(&Configuration {
        stack_size: 12 * 1024,
        lru_purge_enable: true,
        ..Default::default()
    })?)
}
