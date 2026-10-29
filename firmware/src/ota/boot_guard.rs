//! Software rollback: counts the boots of an image that has not confirmed itself yet.
//!
//! The bootloader's app rollback is the primary safety net; this covers a bootloader that does
//! not support it and a bootloader/app mismatch. The decision logic is
//! [`well_core::ota_policy::on_boot`]; this file persists the counter and switches the slot.

use std::sync::{Mutex, MutexGuard};

use anyhow::{Result, anyhow, bail};
use esp_idf_svc::{
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
    sys::{esp, esp_ota_get_next_update_partition, esp_ota_set_boot_partition},
};
use well_core::ota_policy::{BootDecision, MAX_UNCONFIRMED_BOOTS, on_boot};

/// NVS namespace that holds the OTA state.
const NAMESPACE: &str = "ota";

/// NVS key of the unconfirmed-boot counter; absent when no update is pending.
const KEY: &str = "unconfirmed";

/// Persistent boot counter of the software rollback.
pub struct BootGuard {
    /// The NVS namespace, locked because the upload handler and the confirm thread share it.
    nvs: Mutex<EspNvs<NvsDefault>>,
}

impl BootGuard {
    /// Opens the OTA namespace in NVS, creating it when missing.
    ///
    /// # Arguments
    /// * `partition` - The default NVS partition.
    ///
    /// # Returns
    /// The boot guard, or an error when NVS cannot be opened.
    pub fn open(partition: EspDefaultNvsPartition) -> Result<Self> {
        Ok(Self {
            nvs: Mutex::new(EspNvs::new(partition, NAMESPACE, true)?),
        })
    }

    /// Counts this boot and restores the previous image if this one keeps failing to confirm.
    ///
    /// Call first thing at boot. On a rollback the device reboots and this call does not return.
    ///
    /// # Returns
    /// `Ok(())` when the boot may proceed; an error when NVS access fails.
    pub fn check_boot(&self) -> Result<()> {
        let (decision, next) = on_boot(self.read()?);
        self.write(next)?;
        if decision == BootDecision::RollBack {
            log::error!(
                "image not confirmed after {MAX_UNCONFIRMED_BOOTS} boots, restoring the previous image"
            );
            if let Err(err) = switch_to_previous_slot() {
                log::error!("software rollback failed, staying on this image: {err}");
            }
        }
        Ok(())
    }

    /// Marks an update as activated: the new image's boots are counted until it confirms itself.
    ///
    /// # Returns
    /// `Ok(())` once the counter is persisted.
    pub fn arm(&self) -> Result<()> {
        self.write(Some(0))
    }

    /// Tells whether an activated update is still waiting for its confirmation.
    ///
    /// # Returns
    /// `true` while the counter exists.
    pub fn is_armed(&self) -> Result<bool> {
        Ok(self.read()?.is_some())
    }

    /// Clears the counter: the running image has confirmed itself.
    ///
    /// # Returns
    /// `Ok(())` once the counter is removed.
    pub fn confirmed(&self) -> Result<()> {
        self.write(None)
    }

    /// Clears the counter and switches to the previous image, then reboots.
    ///
    /// # Returns
    /// Only on failure (for example when there is no other slot); on success the device reboots.
    pub fn roll_back(&self) -> Result<()> {
        self.write(None)?;
        switch_to_previous_slot()
    }

    /// Reads the counter.
    ///
    /// # Returns
    /// The stored counter, or `None` when no update is pending.
    fn read(&self) -> Result<Option<u8>> {
        Ok(self.lock()?.get_u8(KEY)?)
    }

    /// Stores or removes the counter.
    ///
    /// # Arguments
    /// * `value` - The counter to store, or `None` to remove it.
    ///
    /// # Returns
    /// `Ok(())` once the change is persisted.
    fn write(&self, value: Option<u8>) -> Result<()> {
        let nvs = self.lock()?;
        match value {
            Some(v) => nvs.set_u8(KEY, v)?,
            None => {
                nvs.remove(KEY)?;
            }
        }
        Ok(())
    }

    /// Locks the NVS namespace.
    ///
    /// # Returns
    /// The lock guard, or an error when a previous holder panicked.
    fn lock(&self) -> Result<MutexGuard<'_, EspNvs<NvsDefault>>> {
        self.nvs.lock().map_err(|_| anyhow!("NVS lock poisoned"))
    }
}

/// Makes the other OTA slot the boot slot and reboots into it.
///
/// # Returns
/// Only on failure: no other slot exists or its image is invalid. On success it reboots.
fn switch_to_previous_slot() -> Result<()> {
    // SAFETY: plain ESP-IDF queries; the returned partition table entry lives for the whole run.
    let other = unsafe { esp_ota_get_next_update_partition(core::ptr::null()) };
    if other.is_null() {
        bail!("no other OTA slot");
    }
    // SAFETY: `other` is a valid app partition from the table; the call validates its image.
    esp!(unsafe { esp_ota_set_boot_partition(other) })?;
    esp_idf_svc::hal::reset::restart()
}
