//! LAN over-the-air update: an authenticated upload endpoint plus the rollback confirmation of
//! the running image.
//!
//! - `server`: the HTTP routes `POST /ota` and `GET /ota/status`.
//! - `upload`: streams an upload into the inactive slot and activates it once validated.
//! - `image`: vets an uploaded image before anything is written.
//! - `auth`: HTTP Basic authentication.
//! - `confirm`: confirms a new image after a healthy soak, or rolls back.
//! - `boot_guard`: software rollback counter kept in NVS.
//! - `error`: the rejection type of the upload path.

mod auth;
mod boot_guard;
mod confirm;
mod error;
mod image;
mod server;
mod upload;

use esp_idf_svc::http::server::{EspHttpConnection, Request};

pub use boot_guard::BootGuard;
pub use confirm::confirm_when_healthy;
pub use server::register;

/// An incoming HTTP request on the shared server, as handed to route handlers.
type Req<'r, 'c> = Request<&'r mut EspHttpConnection<'c>>;
