//! Platform selection: the device build and the simulator build differ only in this module.
//!
//! Exactly one implementation is compiled, chosen by the `qemu` cargo feature:
//! - `device`: station-mode Wi-Fi with production timing and policy.
//! - `sim`: QEMU Ethernet, short timing, pre-release images accepted and optional fault hooks.
//!
//! The rest of the firmware sees the same API either way and contains no feature checks.

mod link;

#[cfg(not(feature = "qemu"))]
mod device;
#[cfg(feature = "qemu")]
mod sim;

#[cfg(not(feature = "qemu"))]
pub use device::{ACCEPT_PRERELEASE, DEADLINE, SOAK, at_boot, health, start_network};
#[cfg(feature = "qemu")]
pub use sim::{ACCEPT_PRERELEASE, DEADLINE, SOAK, at_boot, health, start_network};
