//! HTTP routes of the OTA path: `POST /ota` (upload) and `GET /ota/status`, both behind Basic auth.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::Result;
use esp_idf_svc::{
    http::{Method, server::EspHttpServer},
    io::Write,
    ota::{EspOta, SlotState},
};
use well_core::interfaces::UpdateHooks;

use super::{BootGuard, Req, auth::Auth, error::Rejection, upload};

/// Delay between sending the success response and rebooting, so the client receives it.
const REBOOT_DELAY: Duration = Duration::from_secs(1);

/// Adds the OTA routes to the shared HTTP server.
///
/// # Arguments
/// * `server` - The shared HTTP server.
/// * `hooks` - Notified around flash writes so the plant can be made safe.
/// * `guard` - The software rollback counter, armed after a successful upload.
///
/// # Returns
/// `Ok(())` once both routes are registered.
pub fn register(
    server: &mut EspHttpServer<'static>,
    hooks: Arc<dyn UpdateHooks>,
    guard: Arc<BootGuard>,
) -> Result<()> {
    let auth = Arc::new(Auth::from_secrets());
    let busy = Arc::new(AtomicBool::new(false));

    {
        let auth = auth.clone();
        server.fn_handler::<anyhow::Error, _>("/ota/status", Method::Get, move |req| {
            status_route(req, &auth)
        })?;
    }
    server.fn_handler::<anyhow::Error, _>("/ota", Method::Post, move |req| {
        upload_route(req, &auth, &busy, &*hooks, &guard)
    })?;
    Ok(())
}

/// Handles `GET /ota/status`: reports the running image.
///
/// # Arguments
/// * `req` - The request.
/// * `auth` - The authenticator for the `Authorization` header.
///
/// # Returns
/// `Ok(())` after a response was sent (`401`, `200` or `500`); an error only when sending fails.
fn status_route(req: Req<'_, '_>, auth: &Auth) -> Result<()> {
    if !auth.allows(req.header("Authorization")) {
        return unauthorized(req);
    }
    match status_json() {
        Ok(body) => json_reply(req, 200, &body),
        Err(err) => {
            log::error!("OTA status failed: {err:#}");
            json_reply(req, 500, r#"{"error":"status unavailable"}"#)
        }
    }
}

/// Handles `POST /ota`: receives an image and reboots into it when it was accepted.
///
/// # Arguments
/// * `req` - The request; its body is the raw image.
/// * `auth` - The authenticator for the `Authorization` header.
/// * `busy` - Set while an upload runs; a second concurrent upload gets `409`.
/// * `hooks` - Notified around flash writes.
/// * `guard` - The software rollback counter.
///
/// # Returns
/// `Ok(())` after a response was sent; an error only when sending fails. After a `200` the
/// device reboots shortly afterwards.
fn upload_route(
    mut req: Req<'_, '_>,
    auth: &Auth,
    busy: &AtomicBool,
    hooks: &dyn UpdateHooks,
    guard: &BootGuard,
) -> Result<()> {
    if !auth.allows(req.header("Authorization")) {
        return unauthorized(req);
    }
    if busy.swap(true, Ordering::AcqRel) {
        return json_reply(req, 409, r#"{"error":"update already in progress"}"#);
    }
    let _busy = BusyGuard(busy);

    match upload::receive(&mut req, hooks, guard) {
        Ok(version) => {
            json_reply(req, 200, &format!(r#"{{"ok":true,"version":"{version}"}}"#))?;
            thread::spawn(|| {
                thread::sleep(REBOOT_DELAY);
                esp_idf_svc::hal::reset::restart();
            });
            Ok(())
        }
        Err(Rejection(status, msg)) => {
            log::warn!("OTA rejected: {status} {msg}");
            json_reply(req, status, &format!(r#"{{"ok":false,"error":"{msg}"}}"#))
        }
    }
}

/// Renders the status of the running image as JSON.
///
/// # Returns
/// `{"project":..,"version":..,"slot":..,"state":..}`, or an error when the OTA state cannot
/// be read.
fn status_json() -> Result<String> {
    let ota = EspOta::new()?;
    let slot = ota.get_running_slot()?;
    let state = match slot.state {
        SlotState::Valid => "valid",
        SlotState::Unverified => "unverified",
        SlotState::Invalid => "invalid",
        SlotState::Factory => "factory",
        _ => "unknown",
    };
    let version = slot
        .firmware
        .as_ref()
        .map(|f| f.version.as_str().to_owned())
        .unwrap_or_default();
    Ok(format!(
        r#"{{"project":"{}","version":"{}","slot":"{}","state":"{}"}}"#,
        env!("CARGO_PKG_NAME"),
        version,
        slot.label.as_str(),
        state
    ))
}

/// Clears the busy flag on every exit path of an upload.
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    /// Releases the busy flag so the next upload can start.
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Sends a `401` with a Basic auth challenge.
///
/// # Arguments
/// * `req` - The request to answer.
///
/// # Returns
/// `Ok(())` once the response is sent.
fn unauthorized(req: Req<'_, '_>) -> Result<()> {
    req.into_response(
        401,
        Some("Unauthorized"),
        &[("WWW-Authenticate", r#"Basic realm="ota""#)],
    )?;
    Ok(())
}

/// Sends a JSON response.
///
/// # Arguments
/// * `req` - The request to answer.
/// * `status` - The HTTP status code.
/// * `body` - The JSON text of the body.
///
/// # Returns
/// `Ok(())` once the response is sent.
fn json_reply(req: Req<'_, '_>, status: u16, body: &str) -> Result<()> {
    let mut resp = req.into_response(status, None, &[("Content-Type", "application/json")])?;
    resp.write_all(body.as_bytes())?;
    Ok(())
}
