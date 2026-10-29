//! Build script: passes the ESP-IDF build configuration (link arguments and `cfg` flags
//! produced by `esp-idf-sys`) on to this crate.

/// Emits the ESP-IDF link arguments and cfg flags for the compiler.
///
/// # Returns
/// Nothing; cargo reads the instructions printed to stdout.
fn main() {
    embuild::espidf::sysenv::output();
}
