//! Confirms the running image after a healthy soak, or rolls back to the previous one.

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use esp_idf_svc::ota::{EspOta, SlotState};
use well_core::interfaces::HealthSource;

use super::BootGuard;
use crate::platform::{DEADLINE, SOAK};

/// How often the watcher thread samples the health source.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Stack size of the watcher thread in bytes.
const WATCHER_STACK_BYTES: usize = 6 * 1024;

/// Starts the confirmation watcher when the running image is not confirmed yet.
///
/// Does nothing when the image is already confirmed by the bootloader and no software
/// rollback counter is pending.
///
/// # Arguments
/// * `health` - Tells whether the image currently works; it must stay healthy for `SOAK`.
/// * `guard` - The software rollback counter, cleared on confirmation.
///
/// # Returns
/// `Ok(())` once the watcher is running or was not needed; an error when the OTA state cannot
/// be read or the thread cannot be started.
pub fn confirm_when_healthy(
    health: impl HealthSource + 'static,
    guard: Arc<BootGuard>,
) -> Result<()> {
    let state = EspOta::new()?.get_running_slot()?.state;
    log::info!("running image state: {state:?}");
    let bootloader_pending = matches!(state, SlotState::Unverified);
    if !bootloader_pending && !guard.is_armed()? {
        return Ok(());
    }
    thread::Builder::new()
        .stack_size(WATCHER_STACK_BYTES)
        .spawn(move || watch(health, &guard, bootloader_pending))?;
    Ok(())
}

/// Watcher loop: confirms after an uninterrupted healthy `SOAK`, rolls back after `DEADLINE`.
///
/// # Arguments
/// * `health` - The health source to sample every second.
/// * `guard` - The software rollback counter.
/// * `bootloader_pending` - `true` when the bootloader also awaits confirmation of this image.
///
/// # Returns
/// Nothing; the loop ends after a confirmation or a rollback attempt.
fn watch(health: impl HealthSource, guard: &BootGuard, bootloader_pending: bool) {
    let started = Instant::now();
    let mut healthy_since: Option<Instant> = None;
    loop {
        thread::sleep(POLL_INTERVAL);
        if health.is_healthy() {
            let since = *healthy_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= SOAK && confirm(guard, bootloader_pending) {
                return;
            }
        } else {
            healthy_since = None;
        }
        if started.elapsed() >= DEADLINE {
            roll_back(guard, bootloader_pending);
            return;
        }
    }
}

/// Confirms the running image to the bootloader and clears the software counter.
///
/// # Arguments
/// * `guard` - The software rollback counter to clear.
/// * `bootloader_pending` - `true` when the bootloader awaits the confirmation.
///
/// # Returns
/// `false` when the OTA singleton is busy so the caller should retry, `true` otherwise.
fn confirm(guard: &BootGuard, bootloader_pending: bool) -> bool {
    if bootloader_pending {
        let Ok(mut ota) = EspOta::new() else {
            return false;
        };
        if let Err(err) = ota.mark_running_slot_valid() {
            log::error!("confirming image failed: {err}");
            return true;
        }
    }
    match guard.confirmed() {
        Ok(()) => log::info!("new image confirmed"),
        Err(err) => log::error!("clearing the boot counter failed: {err}"),
    }
    true
}

/// Restores the previous image: first through the bootloader, then through the software guard.
///
/// # Arguments
/// * `guard` - The software rollback counter and slot switch.
/// * `bootloader_pending` - `true` when the bootloader's own rollback can be used.
///
/// # Returns
/// Nothing; on success the device reboots, on failure the image stays and the error is logged.
fn roll_back(guard: &BootGuard, bootloader_pending: bool) {
    log::error!("new image never became healthy, rolling back");
    if bootloader_pending && let Ok(mut ota) = EspOta::new() {
        let err = ota.mark_running_slot_invalid_and_reboot();
        log::error!("bootloader rollback failed: {err}");
    }
    if let Err(err) = guard.roll_back() {
        log::error!("software rollback failed, staying on this image: {err}");
    }
}
