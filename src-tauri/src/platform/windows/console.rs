//! Ctrl+C is delivered to every process attached to this console.
//!
//! `SetConsoleCtrlHandler(null, true)` makes this process ignore Ctrl+C, and
//! a child spawned while that is set inherits the ignore. WebView2's browser
//! process is such a child. Its default handler would `ExitProcess` off the
//! UI thread, and Chromium would then fail `UnregisterClass` for
//! `Chrome_WidgetWin_0` with ERROR_CLASS_DOES_NOT_EXIST (1411).
//!
//! Existing children keep the bit when this process turns Ctrl+C back on, so
//! the handler installed afterwards is the host's alone.

use windows_sys::Win32::Foundation::{FALSE, TRUE};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

// `pub` so `windows` can re-export these. `pub(super)` is only visible inside
// `windows`, and rustc rejects a re-export wider than the item.
/// Children spawned while this is set inherit "ignore Ctrl+C".
pub fn suppress_ctrl_c_for_children() -> bool {
    // SAFETY: a null handler with TRUE is the documented ignore switch, not
    // a function pointer we call.
    unsafe { SetConsoleCtrlHandler(None, TRUE) != FALSE }
}

/// This process receives Ctrl+C again. Already-spawned children do not.
pub fn restore_ctrl_c() -> bool {
    // SAFETY: null with FALSE clears the ignore bit set above. No pointer.
    unsafe { SetConsoleCtrlHandler(None, FALSE) != FALSE }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use windows_sys::Win32::System::Console::CTRL_C_EVENT;

    static HITS: AtomicU32 = AtomicU32::new(0);

    unsafe extern "system" fn on_ctrl(kind: u32) -> i32 {
        if kind == CTRL_C_EVENT {
            HITS.fetch_add(1, Ordering::SeqCst);
        }
        TRUE
    }

    /// The ignore bit is process-wide. Put it back even when an assert fails,
    /// or a later test in this process never sees Ctrl+C.
    struct Restore;

    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = restore_ctrl_c();
        }
    }

    #[test]
    fn spawned_children_can_be_kept_off_ctrl_c_and_this_process_turned_back_on() {
        let _lock = crate::platform::ctrl_c::lock_tests();
        let _restore = Restore;
        assert!(
            suppress_ctrl_c_for_children(),
            "SetConsoleCtrlHandler(null, true) is what a WebView2 child inherits"
        );
        assert!(
            restore_ctrl_c(),
            "the host has to receive Ctrl+C again or the quit handler never runs"
        );
    }

    /// Re-exec'd with `FIDGET_CTRL_C_SLEEPER`. A handler that exits is how a
    /// child that did *not* inherit ignore proves the signal arrived.
    #[test]
    fn ctrl_c_inheritance_sleeper() {
        if std::env::var_os("FIDGET_CTRL_C_SLEEPER").is_none() {
            return;
        }
        let ready = std::env::var("FIDGET_CTRL_C_READY").expect("ready path");
        unsafe extern "system" fn die(kind: u32) -> i32 {
            if kind == windows_sys::Win32::System::Console::CTRL_C_EVENT {
                std::process::exit(7);
            }
            TRUE
        }
        // SAFETY: `die` is a system handler. It only exits on Ctrl+C.
        unsafe { SetConsoleCtrlHandler(Some(die), TRUE) };
        std::fs::write(&ready, b"ready").expect("ready");
        std::thread::sleep(std::time::Duration::from_secs(30));
        std::process::exit(0);
    }

    /// While suppress is held, a child inherits ignore. After release, a new
    /// child does not, and this process receives Ctrl+C again.
    #[test]
    fn a_child_spawned_during_suppress_inherits_ignore_and_a_later_one_does_not() {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        if std::env::var_os("FIDGET_CTRL_C_PROBE").is_some() {
            std::process::exit(inheritance_probe());
        }
        let _lock = crate::platform::ctrl_c::lock_tests();
        let exe = std::env::current_exe().expect("test binary");
        let filter = exact_filter(
            "a_child_spawned_during_suppress_inherits_ignore_and_a_later_one_does_not",
        );
        let output = std::process::Command::new(exe)
            .env("FIDGET_CTRL_C_PROBE", "1")
            .arg(&filter)
            .arg("--exact")
            .arg("--test-threads=1")
            .creation_flags(CREATE_NEW_CONSOLE)
            .output()
            .expect("probe process");
        assert_eq!(
            output.status.code(),
            Some(0),
            "inheritance probe failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn exact_filter(name: &str) -> String {
        let module = module_path!();
        let rest = module.split_once("::").map_or(module, |(_, rest)| rest);
        format!("{rest}::{name}")
    }

    fn inheritance_probe() -> i32 {
        match inheritance_probe_result() {
            Ok(()) => 0,
            Err(code) => code,
        }
    }

    fn inheritance_probe_result() -> Result<(), i32> {
        // SAFETY: `on_ctrl` only counts. Returning TRUE keeps the default
        // handler from exiting this probe.
        if unsafe { SetConsoleCtrlHandler(Some(on_ctrl), TRUE) } == FALSE {
            eprintln!("could not listen for Ctrl+C in the probe");
            return Err(3);
        }

        let mut ignored = spawn_sleeper("ignored", true)?;
        wait_ready(&ignored.ready)?;
        if !signal_and_heard() {
            eprintln!("this process did not receive Ctrl+C after the first restore");
            return Err(3);
        }
        if !alive(&mut ignored.child) {
            eprintln!("child spawned during suppress died on Ctrl+C");
            return Err(2);
        }

        // Host listens again. A child spawned now must see Ctrl+C.
        let mut exposed = spawn_sleeper("exposed", false)?;
        wait_ready(&exposed.ready)?;
        if !signal_and_heard() {
            eprintln!("this process did not receive Ctrl+C after restore");
            return Err(3);
        }
        if !died(&mut exposed.child) {
            eprintln!("child spawned after restore survived Ctrl+C");
            return Err(5);
        }
        if !alive(&mut ignored.child) {
            eprintln!("the ignored child died when a later child was signaled");
            return Err(2);
        }

        // A webview created after the handler is installed takes a new hold.
        let mut late = spawn_sleeper("late", true)?;
        wait_ready(&late.ready)?;
        if !signal_and_heard() {
            eprintln!("this process stopped receiving Ctrl+C after a later hold");
            return Err(3);
        }
        if !alive(&mut late.child) || !alive(&mut ignored.child) {
            eprintln!("child spawned during a later suppress died on Ctrl+C");
            return Err(4);
        }
        Ok(())
    }

    fn signal_and_heard() -> bool {
        let before = HITS.load(Ordering::SeqCst);
        signal_ctrl_c().is_ok() && HITS.load(Ordering::SeqCst) > before
    }

    struct Sleeper {
        child: std::process::Child,
        ready: std::path::PathBuf,
    }

    impl Drop for Sleeper {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_file(&self.ready);
        }
    }

    fn spawn_sleeper(label: &str, inherit_ignore: bool) -> Result<Sleeper, i32> {
        let ready = std::env::temp_dir().join(format!(
            "fidget-ctrlc-{}-{}-{}",
            std::process::id(),
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&ready);
        // Held across `spawn` only. The child copies the bit; dropping it
        // turns Ctrl+C back on here before the caller signals.
        let hold = inherit_ignore.then(crate::platform::SpawnedCtrlC::hold);
        let exe = std::env::current_exe().map_err(|_| 6)?;
        let child = std::process::Command::new(exe)
            .env("FIDGET_CTRL_C_SLEEPER", "1")
            .env("FIDGET_CTRL_C_READY", &ready)
            .env_remove("FIDGET_CTRL_C_PROBE")
            .arg(exact_filter("ctrl_c_inheritance_sleeper"))
            .arg("--exact")
            .arg("--test-threads=1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|_| 6)?;
        drop(hold);
        Ok(Sleeper { child, ready })
    }

    fn wait_ready(path: &std::path::Path) -> Result<(), i32> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < until {
            if path.is_file() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        eprintln!("sleeper never became ready: {}", path.display());
        Err(6)
    }

    fn signal_ctrl_c() -> Result<(), i32> {
        use windows_sys::Win32::System::Console::GenerateConsoleCtrlEvent;
        use windows_sys::Win32::System::Console::CTRL_C_EVENT;
        // SAFETY: group 0 is every process on this console. The probe is on
        // its own console, so cargo is not in the set.
        let ok = unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) != FALSE };
        if !ok {
            eprintln!("GenerateConsoleCtrlEvent failed");
            return Err(3);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        Ok(())
    }

    fn alive(child: &mut std::process::Child) -> bool {
        matches!(child.try_wait(), Ok(None))
    }

    fn died(child: &mut std::process::Child) -> bool {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) if std::time::Instant::now() < until => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                _ => return false,
            }
        }
    }
}
