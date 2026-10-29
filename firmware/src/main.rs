//! Firmware entry point: initialisation and orchestration only.
//!
//! The logic lives in the modules:
//! - `platform`: network, timing and policy of the device or of the simulator.
//! - `http` and `web`: the shared HTTP server and the page that uses it.
//! - `ota`: the over-the-air update path.

// The esp_app_desc! macro checks cfgs that esp-idf-sys does not declare for rustc.
#![allow(unexpected_cfgs)]

mod http;
mod ota;
mod platform;
mod secrets;
mod web;

use std::sync::Arc;

use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::peripherals::Peripherals, nvs::EspDefaultNvsPartition,
};
use well_core::interfaces::UpdateHooks;

esp_idf_svc::sys::esp_app_desc!();

/// Stand-in for the I/O layer's [`UpdateHooks`]: only logs, because no outputs are driven yet.
struct LogHooks;

impl UpdateHooks for LogHooks {
    /// Logs that the flash is about to be written.
    fn flash_starting(&self) {
        log::warn!("OTA about to write flash");
    }

    /// Logs that the update was aborted and the old image keeps running.
    fn flash_aborted(&self) {
        log::warn!("OTA aborted, old image keeps running");
    }
}

/// Boots the firmware and then keeps the network link alive forever.
///
/// Order matters: the boot counter runs first so a repeatedly failing image is rolled back
/// before it can do anything else.
///
/// # Returns
/// Only on an initialisation error; in normal operation it never returns.
fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let nvs = EspDefaultNvsPartition::take()?;
    let boot_guard = Arc::new(ota::BootGuard::open(nvs.clone())?);
    boot_guard.check_boot()?;
    platform::at_boot();

    let peripherals = Peripherals::take()?;
    let network = platform::start_network(
        peripherals.modem,
        peripherals.mac,
        EspSystemEventLoop::take()?,
        nvs,
    )?;

    let mut server = http::new_server()?;
    web::register(&mut server)?;
    ota::register(&mut server, Arc::new(LogHooks), boot_guard.clone())?;
    ota::confirm_when_healthy(platform::health(network.link()), boot_guard)?;

    network.keep_connected()
}
