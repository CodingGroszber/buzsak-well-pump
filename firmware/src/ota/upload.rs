//! Streams an upload into the inactive OTA slot and activates it once it is validated.

use esp_idf_svc::ota::{EspOta, SlotState};
use sha2::{Digest, Sha256};
use well_core::{
    interfaces::UpdateHooks,
    ota_policy::{parse_version, to_hex},
};

use super::{
    BootGuard, Req,
    error::Rejection,
    image::{self, MAX_BYTES, MIN_BYTES},
};

/// Size of the buffer used to copy the request body into flash.
const CHUNK_BYTES: usize = 4096;

/// Tells the hooks that the flash is about to be written and, unless disarmed, that the update
/// was aborted. It reports the abort on every early return, including a panic.
struct FlashGuard<'a> {
    /// The hooks to notify.
    hooks: &'a dyn UpdateHooks,
    /// `true` until the update succeeded; while armed, dropping reports an abort.
    armed: bool,
}

impl<'a> FlashGuard<'a> {
    /// Notifies the hooks that flashing starts and arms the abort report.
    ///
    /// # Arguments
    /// * `hooks` - The hooks to notify.
    ///
    /// # Returns
    /// The armed guard.
    fn start(hooks: &'a dyn UpdateHooks) -> Self {
        hooks.flash_starting();
        Self { hooks, armed: true }
    }

    /// Marks the update as successful so dropping the guard reports nothing.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for FlashGuard<'_> {
    /// Reports the abort to the hooks when the guard was never disarmed.
    fn drop(&mut self) {
        if self.armed {
            self.hooks.flash_aborted();
        }
    }
}

/// Receives an image, checks it, writes it to the inactive slot and activates it.
///
/// The boot slot is only changed after the image passed the ESP-IDF validation and its SHA-256
/// matched the one announced in the `X-Sha256` header.
///
/// # Arguments
/// * `req` - The upload request; its body is the raw image.
/// * `hooks` - Notified before the first flash write and when the update is aborted.
/// * `guard` - The software rollback counter, armed after a successful activation.
///
/// # Returns
/// The new image's version once the slot is activated (the caller then reboots), or the
/// [`Rejection`] explaining why the upload was refused.
pub fn receive(
    req: &mut Req<'_, '_>,
    hooks: &dyn UpdateHooks,
    guard: &BootGuard,
) -> Result<String, Rejection> {
    let len = req
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or(Rejection(411, "Content-Length required"))?;
    if len < MIN_BYTES {
        return Err(Rejection(400, "image too small"));
    }
    if len > MAX_BYTES {
        return Err(Rejection(413, "image larger than the app partition"));
    }
    let expected_sha = req
        .header("X-Sha256")
        .map(str::to_ascii_lowercase)
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(Rejection(400, "X-Sha256 header (64 hex chars) required"))?;

    let mut ota = EspOta::new().map_err(|_| Rejection(503, "OTA busy"))?;
    let running = ota.get_running_slot()?;
    if matches!(running.state, SlotState::Unverified) || guard.is_armed()? {
        return Err(Rejection(409, "running image is not confirmed yet"));
    }
    let running_version = running
        .firmware
        .as_ref()
        .and_then(|f| parse_version(f.version.as_str()))
        .ok_or(Rejection(500, "running version unreadable"))?;

    // Vet the header before anything is written to flash.
    let mut head = Vec::with_capacity(MIN_BYTES as usize);
    let mut buf = [0u8; CHUNK_BYTES];
    while (head.len() as u64) < MIN_BYTES {
        let want = (MIN_BYTES as usize - head.len()).min(buf.len());
        let n = read_some(req, &mut buf[..want])?;
        head.extend_from_slice(&buf[..n]);
    }
    let new_version = image::vet_header(&head, running_version)?;

    log::info!("OTA accepted: {len} bytes, version {new_version}");
    let flash = FlashGuard::start(hooks);

    let mut update = ota.initiate_update_with_known_size(len as usize)?;
    let mut hasher = Sha256::new();
    hasher.update(&head);
    update.write(&head)?;
    let mut received = head.len() as u64;
    while received < len {
        let want = ((len - received) as usize).min(buf.len());
        let n = read_some(req, &mut buf[..want])?;
        hasher.update(&buf[..n]);
        update.write(&buf[..n])?;
        received += n as u64;
    }

    // `finish` validates the image; the boot slot is untouched until `activate`.
    let finished = update
        .finish()
        .map_err(|_| Rejection(422, "image validation failed"))?;
    if to_hex(&hasher.finalize()) != expected_sha {
        return Err(Rejection(422, "SHA-256 mismatch"));
    }
    finished.activate()?;
    // The device reboots into the new image from here on, so the outputs stay off.
    flash.disarm();

    if let Err(err) = guard.arm() {
        log::error!("could not arm the boot counter, relying on the bootloader rollback: {err:#}");
    }
    log::info!("OTA activated, rebooting into {new_version}");
    Ok(new_version)
}

/// Reads at least one byte of the request body.
///
/// # Arguments
/// * `req` - The request to read from.
/// * `buf` - The buffer to fill; its length is the most that is read.
///
/// # Returns
/// The number of bytes read (at least one), or a `400` rejection when the client stopped
/// sending before the announced length.
fn read_some(req: &mut Req<'_, '_>, buf: &mut [u8]) -> Result<usize, Rejection> {
    match req.read(buf) {
        Ok(0) => Err(Rejection(400, "upload ended early")),
        Ok(n) => Ok(n),
        Err(_) => Err(Rejection(400, "upload interrupted")),
    }
}
