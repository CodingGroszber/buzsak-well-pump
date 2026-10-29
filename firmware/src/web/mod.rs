//! HTTP server page: the dashboard (`/`) and the JSON API (`/api/state`, `/api/control`).
//!
//! PLACEHOLDER. The routes, handlers and their update loop are implemented in a later phase;
//! the files exist now so the structure is fixed:
//! - [`init`]: the initial logic, which registers the routes on the shared server.
//! - [`run`]: the loop logic, which keeps the data behind the page up to date.

mod init;
mod run;

pub use init::register;
#[allow(unused_imports)] // Placeholder: nothing calls the loop yet.
pub use run::poll;
