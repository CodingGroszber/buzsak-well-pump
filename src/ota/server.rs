//! HTTP routes: `POST /ota` (upload) and `GET /ota/status`, both behind Basic auth.

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
    http::{
        Method,
        server::{Configuration, EspHttpServer},
    },
    io::Write,
    ota::{EspOta, SlotState},
};

use super::{Req, auth::Auth, error::Rejection, upload};

/// `before_flash` runs right before the first flash write (force all outputs OFF there).
pub fn start_server(
    before_flash: impl Fn() + Send + Sync + 'static,
) -> Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        stack_size: 12 * 1024,
        ..Default::default()
    })?;

    let auth = Arc::new(Auth::from_secrets());
    let busy = Arc::new(AtomicBool::new(false));
    let before_flash = Arc::new(before_flash);

    {
        let auth = auth.clone();
        server.fn_handler::<anyhow::Error, _>("/ota/status", Method::Get, move |req| {
            status_route(req, &auth)
        })?;
    }
    server.fn_handler::<anyhow::Error, _>("/ota", Method::Post, move |req| {
        upload_route(req, &auth, &busy, &*before_flash)
    })?;

    Ok(server)
}

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

fn upload_route(
    mut req: Req<'_, '_>,
    auth: &Auth,
    busy: &AtomicBool,
    before_flash: &(dyn Fn() + Send + Sync),
) -> Result<()> {
    if !auth.allows(req.header("Authorization")) {
        return unauthorized(req);
    }
    if busy.swap(true, Ordering::AcqRel) {
        return json_reply(req, 409, r#"{"error":"update already in progress"}"#);
    }
    let _guard = BusyGuard(busy);

    match upload::receive(&mut req, before_flash) {
        Ok(version) => {
            json_reply(req, 200, &format!(r#"{{"ok":true,"version":"{version}"}}"#))?;
            thread::spawn(|| {
                thread::sleep(Duration::from_secs(1));
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

/// Clears the busy flag on every exit path.
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn unauthorized(req: Req<'_, '_>) -> Result<()> {
    req.into_response(
        401,
        Some("Unauthorized"),
        &[("WWW-Authenticate", r#"Basic realm="ota""#)],
    )?;
    Ok(())
}

fn json_reply(req: Req<'_, '_>, status: u16, body: &str) -> Result<()> {
    let mut resp = req.into_response(status, None, &[("Content-Type", "application/json")])?;
    resp.write_all(body.as_bytes())?;
    Ok(())
}
