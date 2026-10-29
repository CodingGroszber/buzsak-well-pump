//! Streams an upload into the inactive OTA slot and activates it once it is validated.

use esp_idf_svc::ota::{EspOta, SlotState};
use sha2::{Digest, Sha256};

use super::{
    Req,
    error::Rejection,
    image::{self, MAX_BYTES, MIN_BYTES},
};

const CHUNK_BYTES: usize = 4096;

/// Returns the new version once the slot is activated; the caller reboots.
pub fn receive(
    req: &mut Req<'_, '_>,
    before_flash: &(dyn Fn() + Send + Sync),
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
    if matches!(running.state, SlotState::Unverified) {
        return Err(Rejection(409, "running image is not confirmed yet"));
    }
    let running_version = running
        .firmware
        .as_ref()
        .and_then(|f| image::parse_version(f.version.as_str()))
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
    before_flash();

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
    if hex(&hasher.finalize()) != expected_sha {
        return Err(Rejection(422, "SHA-256 mismatch"));
    }
    finished.activate()?;
    log::info!("OTA activated, rebooting into {new_version}");
    Ok(new_version)
}

fn read_some(req: &mut Req<'_, '_>, buf: &mut [u8]) -> Result<usize, Rejection> {
    match req.read(buf) {
        Ok(0) => Err(Rejection(400, "upload ended early")),
        Ok(n) => Ok(n),
        Err(_) => Err(Rejection(400, "upload interrupted")),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
