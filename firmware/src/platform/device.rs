//! Device platform: station-mode Wi-Fi with production timing and policy, no fault injection.
//!
//! A down link is only ever retried; control logic must not depend on the network.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Result, anyhow};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{mac::MAC, modem::Modem},
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use well_core::interfaces::HealthSource;

use super::link::Link;
use crate::secrets::{WIFI_PASSWORD, WIFI_SSID};

/// How long a new image must stay healthy before it is confirmed.
pub const SOAK: Duration = Duration::from_secs(120);

/// If the soak is not reached within this time, the new image is rolled back.
pub const DEADLINE: Duration = Duration::from_secs(600);

/// Whether images with a pre-release version (`-suffix`) may be uploaded; never on the device.
pub const ACCEPT_PRERELEASE: bool = false;

/// Pause between link checks while the network is up or being retried.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// The Wi-Fi station and its link state.
pub struct Network {
    /// Blocking wrapper around the ESP-IDF Wi-Fi driver.
    driver: BlockingWifi<EspWifi<'static>>,
    /// `true` while the station is connected and has an IP address.
    up: Arc<AtomicBool>,
}

/// Starts the Wi-Fi driver with the credentials from `secrets`; does not wait for a connection.
///
/// # Arguments
/// * `modem` - The Wi-Fi modem peripheral.
/// * `_mac` - Unused here; the simulator build needs the Ethernet MAC.
/// * `sys_loop` - The system event loop.
/// * `nvs` - The default NVS partition, used by the Wi-Fi driver for calibration data.
///
/// # Returns
/// The started network, or an error when the driver or the configuration is rejected.
pub fn start_network(
    modem: Modem<'static>,
    _mac: MAC<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<Network> {
    let mut driver =
        BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;
    driver.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID
            .try_into()
            .map_err(|_| anyhow!("Wi-Fi SSID too long"))?,
        password: WIFI_PASSWORD
            .try_into()
            .map_err(|_| anyhow!("Wi-Fi password too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;
    driver.start()?;
    Ok(Network {
        driver,
        up: Arc::new(AtomicBool::new(false)),
    })
}

impl Network {
    /// Returns a view of the link state for the health checks.
    pub fn link(&self) -> Link {
        Link::new(self.up.clone())
    }

    /// Keeps the link up forever, reconnecting whenever it drops.
    ///
    /// # Returns
    /// Never returns; the caller's thread is dedicated to the link.
    pub fn keep_connected(mut self) -> ! {
        loop {
            if !self.driver.is_connected().unwrap_or(false) {
                log::warn!("Wi-Fi down, connecting");
                if let Err(err) = self
                    .driver
                    .connect()
                    .and_then(|()| self.driver.wait_netif_up())
                {
                    log::warn!("Wi-Fi connect failed: {err}");
                }
            }
            self.up.store(
                self.driver.is_connected().unwrap_or(false),
                Ordering::Relaxed,
            );
            thread::sleep(RETRY_INTERVAL);
        }
    }
}

/// Early-boot hook for fault injection; does nothing on the device.
pub fn at_boot() {}

/// Wraps the real health source; on the device it is passed through unchanged.
///
/// # Arguments
/// * `real` - The health source to use.
///
/// # Returns
/// `real`, unchanged.
pub fn health(real: impl HealthSource) -> impl HealthSource {
    real
}
