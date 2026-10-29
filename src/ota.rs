//! LAN OTA: authenticated upload endpoint plus rollback confirmation of the running image.

mod auth;
mod confirm;
mod error;
mod image;
mod server;
mod upload;

use esp_idf_svc::http::server::{EspHttpConnection, Request};

pub use confirm::confirm_when_healthy;
pub use server::start_server;

type Req<'r, 'c> = Request<&'r mut EspHttpConnection<'c>>;
