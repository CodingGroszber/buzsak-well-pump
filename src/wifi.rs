//! Station-mode Wi-Fi. A down link is only ever retried; control logic must not depend on it.

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
    hal::modem::Modem,
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};

use crate::secrets::{WIFI_PASSWORD, WIFI_SSID};

const RETRY_INTERVAL: Duration = Duration::from_secs(5);

pub struct Wifi {
    driver: BlockingWifi<EspWifi<'static>>,
    up: Arc<AtomicBool>,
}

/// Cheap, cloneable view of whether the link is currently up.
#[derive(Clone)]
pub struct Link(Arc<AtomicBool>);

impl Link {
    pub fn is_up(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Wifi {
    /// Starts the driver; does not wait for a connection.
    pub fn start(
        modem: Modem<'static>,
        sys_loop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
    ) -> Result<Self> {
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
        Ok(Self {
            driver,
            up: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn link(&self) -> Link {
        Link(self.up.clone())
    }

    /// Keeps the link up forever, reconnecting whenever it drops.
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
