//! Vetting of an uploaded firmware image before anything is written to flash.

use std::ffi::CStr;

use esp_idf_svc::{ota::EspFirmwareInfoLoad, sys::esp_chip_id_t_ESP_CHIP_ID_ESP32};

use super::error::Rejection;

// Must match the app partition size in partitions.csv.
pub const MAX_BYTES: u64 = 0x64_0000;
// Image header + first segment header + app descriptor.
pub const MIN_BYTES: u64 = 24 + 8 + 256;
const MAGIC: u8 = 0xE9;

pub type Version = (u32, u32, u32);

/// Checks the first `MIN_BYTES` of an image and returns its version string.
pub fn vet_header(head: &[u8], running: Version) -> Result<String, Rejection> {
    let info = EspFirmwareInfoLoad
        .fetch_native(head)
        .ok_or(Rejection(400, "image header unreadable"))?;
    if info.image_header.magic != MAGIC {
        return Err(Rejection(422, "not an ESP image"));
    }
    if info.image_header.chip_id != esp_chip_id_t_ESP_CHIP_ID_ESP32 {
        return Err(Rejection(422, "image is for a different chip"));
    }
    if c_str(&info.app_desc.project_name) != env!("CARGO_PKG_NAME") {
        return Err(Rejection(422, "image is for a different project"));
    }
    let version = c_str(&info.app_desc.version);
    match parse_version(&version) {
        Some(v) if v > running => Ok(version),
        Some(_) => Err(Rejection(422, "version must be newer than the running one")),
        None => Err(Rejection(422, "image version is not major.minor.patch")),
    }
}

pub fn parse_version(v: &str) -> Option<Version> {
    let core = v.trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn c_str(field: &[core::ffi::c_char]) -> String {
    // SAFETY: app_desc string fields are fixed-size, NUL-terminated by the image format.
    unsafe { CStr::from_ptr(field.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
