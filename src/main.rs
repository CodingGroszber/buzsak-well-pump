// The esp_app_desc! macro checks cfgs that esp-idf-sys does not declare for rustc.
#![allow(unexpected_cfgs)]

mod ota;
mod secrets;
mod wifi;

use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, hal::peripherals::Peripherals, nvs::EspDefaultNvsPartition,
};

esp_idf_svc::sys::esp_app_desc!();

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let wifi = wifi::Wifi::start(
        peripherals.modem,
        EspSystemEventLoop::take()?,
        EspDefaultNvsPartition::take()?,
    )?;

    let _server = ota::start_server(|| {
        // Outputs are not ported yet; force them OFF here before the first flash write.
        log::warn!("OTA about to write flash");
    })?;
    let link = wifi.link();
    ota::confirm_when_healthy(move || link.is_up())?;

    wifi.keep_connected()
}
