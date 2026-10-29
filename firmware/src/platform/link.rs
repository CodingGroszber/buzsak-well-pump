//! The link-state flag shared between the network driver and the health checks.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use well_core::interfaces::HealthSource;

/// Cheap, cloneable view of whether the network link is currently up.
///
/// Doubles as the [`HealthSource`] of the OTA confirmation: an image that cannot keep its
/// network link is not healthy.
#[derive(Clone)]
pub struct Link(Arc<AtomicBool>);

impl Link {
    /// Wraps the flag that the network driver keeps up to date.
    ///
    /// # Arguments
    /// * `up` - Shared flag, `true` while the link is up.
    ///
    /// # Returns
    /// A view of that flag.
    pub fn new(up: Arc<AtomicBool>) -> Self {
        Self(up)
    }
}

impl HealthSource for Link {
    /// Returns `true` while the network link is up.
    fn is_healthy(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
