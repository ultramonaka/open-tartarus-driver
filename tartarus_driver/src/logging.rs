// ===========================================================================
// Locating config.toml / logs/run.log relative to where the binary is
// actually running from
// ===========================================================================
//
// A prior version resolved these paths at COMPILE time via
// `env!("CARGO_MANIFEST_DIR")`. That bakes in the absolute path of whichever
// machine built the binary — harmless for a local `cargo build`, but it means
// a binary built on a CI runner (e.g. `D:\a\open-tartarus-driver\...`) ships
// with that CI path hardcoded, so config.toml/run.log can never be found on
// a user's machine no matter where the exe is placed (confirmed in the wild
// via the GitHub Actions release build). Resolved at runtime instead:
//   - Distributed build: the exe sits next to config.toml/logs/ (the
//     packaged release layout), so its own directory is the right base.
//   - Dev build (`cargo run`/`cargo build`, debug or release): the exe lives
//     under `tartarus_driver/target/<profile>/`, so walk back up past
//     `target/<profile>` and the crate dir to the repo root, matching where
//     config.toml has always lived for development.
fn app_root() -> std::path::PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let looks_like_build_profile_dir = exe_dir
        .file_name()
        .is_some_and(|n| n == "debug" || n == "release");
    if looks_like_build_profile_dir
        && let Some(target_dir) = exe_dir.parent()
        && target_dir.file_name().is_some_and(|n| n == "target")
        && let Some(repo_root) = target_dir.parent().and_then(|p| p.parent())
    {
        return repo_root.to_path_buf();
    }
    exe_dir
}

// Re-exported at the crate root (see main.rs's `pub use logging::config_path;`)
// since config/load.rs and config/payload.rs both need it as `crate::config_path()`.
pub fn config_path() -> std::path::PathBuf {
    app_root().join("config.toml")
}

fn log_path() -> std::path::PathBuf {
    app_root().join("logs").join("run.log")
}

// v1.0.6 hot-reload: config.toml's current mtime, or None if it can't be
// stat'd (missing, or some other I/O error) — a plain sentinel rather than
// panicking/crashing, since "no file" is an ordinary, already-handled state
// (config::load()/try_reload() both fall back gracefully). Comparing two
// `Option<SystemTime>` values with `!=` also means a config.toml that's
// created for the first time WHILE the driver is already running (None ->
// Some(...)) is correctly detected as a change, not just edits to an
// existing file.
pub(crate) fn config_mtime_now() -> Option<std::time::SystemTime> {
    std::fs::metadata(config_path()).and_then(|m| m.modified()).ok()
}

// ===========================================================================
// Always-on file logging
// ===========================================================================
//
// Piping stdout through `Tee-Object` from PowerShell has repeatedly failed in
// practice (wrong shell cwd, mangled multi-line pastes, etc.), losing test
// output. So the program writes its own log directly, independent of however
// it's invoked.
//
// The actual disk write happens on its own background thread, off of
// whichever thread called println!/eprintln!. This matters because the
// D-pad/wheel/Hypershift path (dpad.rs) and the analog key-read loop in
// main.rs both log on every single keystroke/wheel-notch transition, on the
// same thread that's also responsible for actually forwarding/remapping that
// event as fast as possible — a synchronous file write (a syscall, however
// fast) sitting in that path directly adds to input latency, which runs
// against this project's whole point. println!/eprintln! now only do a
// cheap in-memory channel send; the writer thread does the real I/O
// whenever it gets to it, however far behind that ends up being.
pub(crate) static LOG_SENDER: std::sync::OnceLock<std::sync::mpsc::Sender<String>> = std::sync::OnceLock::new();

pub(crate) fn init_log_file() {
    let path = log_path();
    // `logs/` is gitignored and not part of a fresh clone/release download
    // (see .gitignore), so it may not exist yet; File::create alone would
    // fail since it never creates missing parent directories.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::File::create(&path) {
        Ok(mut file) => {
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            let _ = LOG_SENDER.set(tx);
            std::thread::spawn(move || {
                use std::io::{Seek, Write};
                // `tray` mode is meant to run for days at a time, and every
                // keystroke/wheel-notch transition logs a line — cap the
                // file at ~5 MiB instead of growing it unbounded for the
                // life of the process. Restarting the file (rather than
                // deleting/renaming) keeps this thread's single open handle
                // valid throughout.
                const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
                let mut written: u64 = 0;
                for line in rx {
                    if written >= MAX_LOG_BYTES {
                        let _ = file.set_len(0);
                        let _ = file.rewind();
                        let _ = file.write_all(b"[log truncated at ~5 MiB; continuing]\n");
                        written = 0;
                    }
                    written += line.len() as u64 + 1;
                    let _ = file.write_all(line.as_bytes());
                    let _ = file.write_all(b"\n");
                }
            });
            std::println!("Logging to {}", path.display());
        }
        Err(e) => std::eprintln!(
            "WARNING: could not open log file {}: {e} (console-only)",
            path.display()
        ),
    }
}

// Shadow println!/eprintln! everywhere so every call site across the crate
// gets file logging for free, with zero other changes needed.
// #[macro_export] (rather than relying on textual scoping) so every other
// module can use them too via `crate::{println, eprintln}` regardless of
// where `mod logging;` etc. appear relative to these definitions — the
// $crate::logging::LOG_SENDER path below is resolved from the *caller's*
// crate root, which is what makes that work from any module.
#[macro_export]
macro_rules! println {
    () => {{ std::println!(); }};
    ($($arg:tt)*) => {{
        let s = format!($($arg)*);
        std::println!("{s}");
        if let Some(tx) = $crate::logging::LOG_SENDER.get() {
            let _ = tx.send(s);
        }
    }};
}
#[macro_export]
macro_rules! eprintln {
    () => {{ std::eprintln!(); }};
    ($($arg:tt)*) => {{
        let s = format!($($arg)*);
        std::eprintln!("{s}");
        if let Some(tx) = $crate::logging::LOG_SENDER.get() {
            let _ = tx.send(s);
        }
    }};
}
