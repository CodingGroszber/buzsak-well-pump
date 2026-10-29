//! Vetting of an uploaded firmware image before anything is written to flash.

use std::ffi::CStr;

use esp_idf_svc::{ota::EspFirmwareInfoLoad, sys::esp_chip_id_t_ESP_CHIP_ID_ESP32};
use well_core::ota_policy::{Version, is_prerelease, parse_version};

use super::error::Rejection;
use crate::platform::ACCEPT_PRERELEASE;

/// Largest accepted image in bytes; must match the app partition size in `partitions.csv`.
pub const MAX_BYTES: u64 = 0x64_0000;

/// Smallest image that can be vetted: image header, first segment header and app descriptor.
pub const MIN_BYTES: u64 = 24 + 8 + 256;

/// First byte of every ESP image.
const MAGIC: u8 = 0xE9;

/// Checks the first [`MIN_BYTES`] of an image against the running firmware.
///
/// The image must be an ESP32 image of this project with a well-formed version that is newer
/// than the running one; pre-release versions are only accepted where the platform allows it.
///
/// # Arguments
/// * `head` - The first bytes of the upload, at least [`MIN_BYTES`].
/// * `running` - The version of the running image.
///
/// # Returns
/// The new image's version text, or the [`Rejection`] (HTTP status and reason) explaining why
/// the image is refused.
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
    if is_prerelease(&version) && !ACCEPT_PRERELEASE {
        return Err(Rejection(
            422,
            "pre-release images are for the simulator only",
        ));
    }
    match parse_version(&version) {
        Some(v) if v > running => Ok(version),
        Some(_) => Err(Rejection(422, "version must be newer than the running one")),
        None => Err(Rejection(422, "image version is not major.minor.patch")),
    }
}

/// Converts a fixed-size, NUL-terminated C string field of the app descriptor to a `String`.
///
/// # Arguments
/// * `field` - The raw field from the app descriptor.
///
/// # Returns
/// The text up to the first NUL byte.
fn c_str(field: &[core::ffi::c_char]) -> String {
    // SAFETY: app_desc string fields are fixed-size, NUL-terminated by the image format.
    unsafe { CStr::from_ptr(field.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
