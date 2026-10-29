//! Contracts between the firmware layers.
//!
//! - [`plc`]: the physical interfaces of the Industrial Shields ESP32 PLC 14 IO (terminals,
//!   pins, buses) as structs, named exactly as on the board (`I0_0`, `Q0_0`, `R0_0`, ...).
//! - The traits below are what the OTA path needs from the rest of the firmware.
//!
//! Nothing here contains production logic: no pump, tank or selector semantics.

pub mod plc;

/// Called around a flash write so the rest of the firmware can put the plant in a safe state.
///
/// Implemented later by the I/O layer. The OTA path guarantees that every
/// [`flash_starting`](UpdateHooks::flash_starting) is followed either by a reboot into the new
/// image or by [`flash_aborted`](UpdateHooks::flash_aborted).
pub trait UpdateHooks: Send + Sync {
    /// The first flash write is about to happen: stop all outputs and keep them off.
    fn flash_starting(&self);

    /// The update failed before activation: the old image keeps running, so release the outputs.
    fn flash_aborted(&self);
}

/// Whether the running image is doing its job.
///
/// A freshly updated image is only confirmed (and therefore kept) while this reports healthy
/// for the whole soak period.
pub trait HealthSource: Send + Sync {
    /// Returns `true` when the image currently works as intended.
    fn is_healthy(&self) -> bool;
}
