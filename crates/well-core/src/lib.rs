//! Hardware-free core of the well pump firmware.
//!
//! Everything in this crate builds and tests on the host (`cargo test` at the repository
//! root) without an ESP32 toolchain. The firmware crate depends on it.
//!
//! - [`interfaces`]: the contracts between the firmware layers (traits and data structs).
//! - [`ota_policy`]: pure decision logic of the OTA update path.

#![forbid(unsafe_code)]

pub mod interfaces;
pub mod ota_policy;
