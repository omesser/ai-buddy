//! Shared X11 connection for all X11 platform modules.
//!
//! Single process-lifetime connection reused by pointer, overlay, window_source,
//! and sensing modules. Lazily initialized on first use.
//!
//! Thread-safety: RustConnection implements Sync, making it safe to share
//! across threads. OnceLock provides thread-safe initialization. Read operations
//! like get_property are safe for concurrent access from the frame thread and
//! GTK main thread.

use std::sync::OnceLock;
use x11rb::rust_connection::RustConnection;

/// Opened on first call and reused for the process lifetime. Returns None if
/// the connection fails or `DISPLAY` is not set.
///
/// A failed connection is cached as None, so all subsequent calls will return
/// None without retrying. In practice this is not a problem: if X11 is
/// unavailable at startup, it will not become available later, and window
/// handles will not be X11 anyway (they will be Wayland or unavailable).
pub fn connection() -> Option<&'static RustConnection> {
    static CONN: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONN.get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}
