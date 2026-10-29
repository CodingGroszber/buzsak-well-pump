//! Simulator platform (QEMU): Ethernet through the emulated OpenCores MAC, short timing,
//! pre-release images accepted and optional fault hooks. Never part of a device build.

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
    eth::{BlockingEth, EspEth, EthDriver, OpenEth},
    eventloop::EspSystemEventLoop,
    hal::{mac::MAC, modem::Modem},
    nvs::EspDefaultNvsPartition,
};
use well_core::interfaces::HealthSource;

use super::link::Link;

/// How long a new image must stay healthy before it is confirmed (shortened for the simulator).
pub const SOAK: Duration = Duration::from_secs(10);

/// If the soak is not reached within this time, the new image is rolled back.
pub const DEADLINE: Duration = Duration::from_secs(60);

/// Pre-release images (`-suffix`) are the simulator's test images, so they are accepted here.
pub const ACCEPT_PRERELEASE: bool = true;

/// Pause between link checks while the network is up or being waited for.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// The emulated Ethernet interface and its link state.
pub struct Network {
    /// Blocking wrapper around the ESP-IDF Ethernet driver.
    driver: BlockingEth<EspEth<'static, OpenEth>>,
    /// `true` while the interface has an IP address.
    up: Arc<AtomicBool>,
}

/// Starts the emulated Ethernet interface; does not wait for an IP address.
///
/// # Arguments
/// * `_modem` - Unused; only the device build needs the Wi-Fi modem.
/// * `mac` - The Ethernet MAC peripheral.
/// * `sys_loop` - The system event loop.
/// * `_nvs` - Unused; only the Wi-Fi driver needs the NVS partition.
///
/// # Returns
/// The started network, or an error when the driver cannot be created.
pub fn start_network(
    _modem: Modem<'static>,
    mac: MAC<'static>,
    sys_loop: EspSystemEventLoop,
    _nvs: EspDefaultNvsPartition,
) -> Result<Network> {
    let eth = EspEth::wrap(EthDriver::new_openeth(mac, sys_loop.clone())?)?;
    let mut driver = BlockingEth::wrap(eth, sys_loop)?;
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

    /// Keeps waiting for the interface forever and logs `network up` when it first comes up.
    ///
    /// # Returns
    /// Never returns; the caller's thread is dedicated to the link.
    pub fn keep_connected(self) -> ! {
        loop {
            if !self.driver.is_up().unwrap_or(false) {
                log::warn!("Ethernet down, waiting for the network");
                if let Err(err) = self.driver.wait_netif_up() {
                    log::warn!("Ethernet wait failed: {err}");
                }
            }
            let up = self.driver.is_up().unwrap_or(false);
            if up && !self.up.load(Ordering::Relaxed) {
                log::info!("network up");
            }
            self.up.store(up, Ordering::Relaxed);
            thread::sleep(RETRY_INTERVAL);
        }
    }
}

/// Early-boot hook for fault injection; call it right after the boot counter check.
///
/// With `fault-panic` the image panics immediately; with `fault-panic-late` it panics five
/// seconds after boot. Without a fault feature it does nothing.
pub fn at_boot() {
    #[cfg(feature = "fault-panic")]
    panic!("injected fault: panic at boot");

    #[cfg(feature = "fault-panic-late")]
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(5));
        panic!("injected fault: panic after the network came up");
    });
}

/// Wraps the real health source; with `fault-unhealthy` the image never reports healthy.
///
/// # Arguments
/// * `real` - The health source to wrap.
///
/// # Returns
/// A health source that reports unhealthy under `fault-unhealthy` and otherwise follows `real`.
pub fn health(real: impl HealthSource) -> impl HealthSource {
    Faulty(real)
}

/// Health source wrapper that can force "unhealthy" for fault injection.
struct Faulty<H>(H);

impl<H: HealthSource> HealthSource for Faulty<H> {
    /// Returns `false` under `fault-unhealthy`, otherwise the wrapped source's answer.
    fn is_healthy(&self) -> bool {
        !cfg!(feature = "fault-unhealthy") && self.0.is_healthy()
    }
}
