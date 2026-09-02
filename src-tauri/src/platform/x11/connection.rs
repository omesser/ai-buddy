//! Shared X11 connection for all X11 platform modules.
//!
//! Single process-lifetime connection reused by pointer, overlay, window_source,
//! and sensing modules. Lazily initialized on first use.

use std::sync::OnceLock;
use x11rb::rust_connection::RustConnection;

/// Get the cached X11 connection for this process.
///
/// Connection is opened on first call and reused for the process lifetime.
/// Returns None if connection fails or DISPLAY is not set.
pub fn connection() -> Option<&'static RustConnection> {
    static CONN: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONN.get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}
