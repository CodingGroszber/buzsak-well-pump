//! Initial logic of the HTTP page: registering its routes (placeholder).

use anyhow::Result;
use esp_idf_svc::http::server::EspHttpServer;

/// Registers the page's routes on the shared HTTP server.
///
/// Placeholder: registers nothing yet. Later it adds `GET /`, `GET /api/state` and
/// `POST /api/control`.
///
/// # Arguments
/// * `_server` - The shared HTTP server the routes will be added to.
///
/// # Returns
/// `Ok(())` once all routes are registered.
pub fn register(_server: &mut EspHttpServer<'static>) -> Result<()> {
    log::info!("web page: no routes registered yet (placeholder)");
    Ok(())
}
