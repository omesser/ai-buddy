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

/// None if `DISPLAY` is unset or the open fails.
pub fn connection() -> Option<&'static RustConnection> {
    static CONN: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONN.get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}
