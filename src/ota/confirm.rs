//! Confirms the running image after a healthy soak, or rolls back to the previous one.

use std::{
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use esp_idf_svc::ota::{EspOta, SlotState};

// The new image must be healthy this long before it is confirmed.
const SOAK: Duration = Duration::from_secs(120);
// Healthy soak not reached by then: roll back.
const DEADLINE: Duration = Duration::from_secs(600);

/// Does nothing when the running image is already confirmed.
pub fn confirm_when_healthy(is_healthy: impl Fn() -> bool + Send + 'static) -> Result<()> {
    let state = EspOta::new()?.get_running_slot()?.state;
    log::info!("running image state: {state:?}");
    if !matches!(state, SlotState::Unverified) {
        return Ok(());
    }
    thread::Builder::new()
        .stack_size(6 * 1024)
        .spawn(move || watch(is_healthy))?;
    Ok(())
}

fn watch(is_healthy: impl Fn() -> bool) {
    let started = Instant::now();
    let mut healthy_since: Option<Instant> = None;
    loop {
        thread::sleep(Duration::from_secs(1));
        if is_healthy() {
            let since = *healthy_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= SOAK && confirm() {
                return;
            }
        } else {
            healthy_since = None;
        }
        if started.elapsed() >= DEADLINE {
            roll_back();
            return;
        }
    }
}

/// False when the OTA singleton is busy, so the caller retries.
fn confirm() -> bool {
    let Ok(mut ota) = EspOta::new() else {
        return false;
    };
    match ota.mark_running_slot_valid() {
        Ok(()) => log::info!("new image confirmed"),
        Err(err) => log::error!("confirming image failed: {err}"),
    }
    true
}

fn roll_back() {
    log::error!("new image never became healthy, rolling back");
    if let Ok(mut ota) = EspOta::new() {
        let err = ota.mark_running_slot_invalid_and_reboot();
        log::error!("rollback failed, staying on this image: {err}");
    }
}
