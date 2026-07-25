use hidapi::HidApi;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

mod config;
mod configui;
mod dpad;
mod emulate;
mod hypershift;
mod lighting;
mod tray;
mod vkname;
use windows::Win32::Foundation::BOOL;
use windows::Win32::System::Console::{FreeConsole, SetConsoleCtrlHandler};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY,
};

// ===========================================================================
// Locating config.toml / tasks/run.log relative to where the binary is
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
//   - Distributed build: the exe sits next to config.toml/tasks/ (the layout
//     `release.yml` packages), so its own directory is the right base.
//   - Dev build (`cargo run`/`cargo build`, debug or release): the exe lives
//     under `tartarus_driver/target/<profile>/`, so walk back up past
//     `target/<profile>` and the crate dir to the repo root, matching where
//     config.toml has always lived for development.
pub fn app_root() -> std::path::PathBuf {
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

pub fn config_path() -> std::path::PathBuf {
    app_root().join("config.toml")
}

fn log_path() -> std::path::PathBuf {
    app_root().join("tasks").join("run.log")
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
// D-pad/wheel/Hypershift path (dpad.rs) and the analog key-read loop below
// both log on every single keystroke/wheel-notch transition, on the same
// thread that's also responsible for actually forwarding/remapping that
// event as fast as possible — a synchronous file write (a syscall, however
// fast) sitting in that path directly adds to input latency, which runs
// against this project's whole point. println!/eprintln! now only do a
// cheap in-memory channel send; the writer thread does the real I/O
// whenever it gets to it, however far behind that ends up being.
static LOG_SENDER: OnceLock<std::sync::mpsc::Sender<String>> = OnceLock::new();

fn init_log_file() {
    let path = log_path();
    // `tasks/` isn't published in the public repo (development-history
    // working notes, gitignored — see .gitignore), so a fresh clone/release
    // download won't have this directory at all; File::create alone would
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

// Shadow println!/eprintln! everywhere below this point so every existing
// call site gets file logging for free, with zero other changes needed.
// #[macro_export] (rather than relying on textual scoping) so config.rs /
// configui.rs can use them too via `crate::{println, eprintln}` regardless
// of where `mod config;` etc. appear relative to these definitions.
#[macro_export]
macro_rules! println {
    () => {{ std::println!(); }};
    ($($arg:tt)*) => {{
        let s = format!($($arg)*);
        std::println!("{s}");
        if let Some(tx) = $crate::LOG_SENDER.get() {
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
        if let Some(tx) = $crate::LOG_SENDER.get() {
            let _ = tx.send(s);
        }
    }};
}

// Phase 4: loaded once at startup (config.toml if present, else built-in
// placeholder defaults — see config.rs). A OnceLock rather than passing a
// reference through every function because the Interception thread and the
// main analog-read loop both need it and are set up independently; it is
// written exactly once, before either thread starts using it, so there is no
// mutation race.
static CONFIG: OnceLock<config::DriverConfig> = OnceLock::new();
fn cfg() -> &'static config::DriverConfig {
    CONFIG.get().expect("CONFIG must be set at the start of main() before anything reads it")
}

const VID: u16 = 0x1532;
const PID: u16 = 0x0244;

// Interface 1 / endpoint 0x82 emits this report ID for the 20 analog keys.
// Reverse-engineered via USBPcap capture on 2026-07-18 (tasks/capture.pcap):
// byte[0] = report ID, byte[1..=20] = one 0-255 depth value per physical key.
// Confirmed 2026-07-18 (tasks/keymap_log.txt): byte offset N == the number
// printed on keycap N (identity mapping, no permutation).
const ANALOG_REPORT_ID: u8 = 0x06;
const NUM_KEYS: usize = 20;

// Phase v1.0.5 (Hyper Shift redesign): the analog keymap now holds up to 3
// layers (index 0 = Default, 1 = Layer1, 2 = Layer2) instead of two fixed
// fields. Toggle-style Hypershift can cycle through 2 or 3 of them
// (config.toml's [hypershift] layer_count); momentary style always uses
// exactly layers 0/1 regardless of layer_count (see hypershift.rs).
pub const MAX_LAYERS: usize = 3;

// Hysteresis thresholds per Purpose.md §6.1 (recommended values). Phase 4:
// these are now specifically the BUILT-IN DEFAULT used by config.rs
// whenever config.toml has no [actuation] section (or an invalid t_on/t_off
// pair) — see config::DriverConfig::defaults(). The actual analog loop below
// always reads the live values via cfg().actuation, never these consts
// directly.
const T_ON: u8 = 100;
const T_OFF: u8 = 80;

// TEST/PLACEHOLDER keymap — not a real layout, just enough to prove the
// hysteresis + SendInput pipeline end to end. key01..key20 -> '1'..'9','0','A'..'J'.
// Phase 4: this is now specifically the BUILT-IN DEFAULT used by config.rs
// whenever config.toml doesn't override a given key (or doesn't exist at
// all) — see config::DriverConfig::defaults(). Editing this array changes
// what a machine with no config.toml (or an incomplete one) falls back to.
const TEST_KEYMAP: [VIRTUAL_KEY; NUM_KEYS] = [
    VIRTUAL_KEY(0x31), // key01 -> '1'
    VIRTUAL_KEY(0x32), // key02 -> '2'
    VIRTUAL_KEY(0x33), // key03 -> '3'
    VIRTUAL_KEY(0x34), // key04 -> '4'
    VIRTUAL_KEY(0x35), // key05 -> '5'
    VIRTUAL_KEY(0x36), // key06 -> '6'
    VIRTUAL_KEY(0x37), // key07 -> '7'
    VIRTUAL_KEY(0x38), // key08 -> '8'
    VIRTUAL_KEY(0x39), // key09 -> '9'
    VIRTUAL_KEY(0x30), // key10 -> '0'
    VIRTUAL_KEY(0x41), // key11 -> 'A'
    VIRTUAL_KEY(0x42), // key12 -> 'B'
    VIRTUAL_KEY(0x43), // key13 -> 'C'
    VIRTUAL_KEY(0x44), // key14 -> 'D'
    VIRTUAL_KEY(0x45), // key15 -> 'E'
    VIRTUAL_KEY(0x46), // key16 -> 'F'
    VIRTUAL_KEY(0x47), // key17 -> 'G'
    VIRTUAL_KEY(0x48), // key18 -> 'H'
    VIRTUAL_KEY(0x49), // key19 -> 'I'
    VIRTUAL_KEY(0x4A), // key20 -> 'J'
];

// TEST/PLACEHOLDER Layer1 (Hypershift) keymap — not a real layout, just enough
// to prove the layer switch end to end. key01..key20 -> F1..F20 so Layer1 hits
// are trivially distinguishable from the Default layer during testing.
const LAYER1_TEST_KEYMAP: [VIRTUAL_KEY; NUM_KEYS] = [
    VIRTUAL_KEY(0x70), // key01 -> F1
    VIRTUAL_KEY(0x71), // key02 -> F2
    VIRTUAL_KEY(0x72), // key03 -> F3
    VIRTUAL_KEY(0x73), // key04 -> F4
    VIRTUAL_KEY(0x74), // key05 -> F5
    VIRTUAL_KEY(0x75), // key06 -> F6
    VIRTUAL_KEY(0x76), // key07 -> F7
    VIRTUAL_KEY(0x77), // key08 -> F8
    VIRTUAL_KEY(0x78), // key09 -> F9
    VIRTUAL_KEY(0x79), // key10 -> F10
    VIRTUAL_KEY(0x7A), // key11 -> F11
    VIRTUAL_KEY(0x7B), // key12 -> F12
    VIRTUAL_KEY(0x7C), // key13 -> F13
    VIRTUAL_KEY(0x7D), // key14 -> F14
    VIRTUAL_KEY(0x7E), // key15 -> F15
    VIRTUAL_KEY(0x7F), // key16 -> F16
    VIRTUAL_KEY(0x80), // key17 -> F17
    VIRTUAL_KEY(0x81), // key18 -> F18
    VIRTUAL_KEY(0x82), // key19 -> F19
    VIRTUAL_KEY(0x83), // key20 -> F20
];

// TEST/PLACEHOLDER Layer2 (Hypershift toggle, 3rd layer) keymap — same
// throwaway style as TEST_KEYMAP/LAYER1_TEST_KEYMAP, only reachable when
// config.toml sets [hypershift] switch_style="toggle", layer_count=3. Reuses
// vkname.rs's 20 named specials (LEFT..INSERT) so it's trivially
// distinguishable from both other layers during testing without needing any
// new vkname vocabulary.
const LAYER2_TEST_KEYMAP: [VIRTUAL_KEY; NUM_KEYS] = [
    VIRTUAL_KEY(0x25), // key01 -> LEFT
    VIRTUAL_KEY(0x26), // key02 -> UP
    VIRTUAL_KEY(0x27), // key03 -> RIGHT
    VIRTUAL_KEY(0x28), // key04 -> DOWN
    VIRTUAL_KEY(0x20), // key05 -> SPACE
    VIRTUAL_KEY(0x0D), // key06 -> ENTER
    VIRTUAL_KEY(0x09), // key07 -> TAB
    VIRTUAL_KEY(0x1B), // key08 -> ESCAPE
    VIRTUAL_KEY(0x08), // key09 -> BACKSPACE
    VIRTUAL_KEY(0xA0), // key10 -> LSHIFT
    VIRTUAL_KEY(0xA1), // key11 -> RSHIFT
    VIRTUAL_KEY(0xA2), // key12 -> LCTRL
    VIRTUAL_KEY(0xA3), // key13 -> RCTRL
    VIRTUAL_KEY(0xA4), // key14 -> LALT
    VIRTUAL_KEY(0xA5), // key15 -> RALT
    VIRTUAL_KEY(0x24), // key16 -> HOME
    VIRTUAL_KEY(0x23), // key17 -> END
    VIRTUAL_KEY(0x21), // key18 -> PAGEUP
    VIRTUAL_KEY(0x22), // key19 -> PAGEDOWN
    VIRTUAL_KEY(0x2D), // key20 -> INSERT
];

// Set by console_ctrl_handler (Ctrl+C, Ctrl+Break, console window closed,
// logoff, or shutdown) so the main analog-read loop can notice and exit its
// own way — running the existing "force-release any key still logically
// held" cleanup at the bottom of main() — instead of Windows just killing
// the process outright, which would skip that cleanup and could leave a key
// stuck down on the OS. The loop's sleep granularity (500us) means this is
// noticed almost immediately, well within the few seconds Windows grants a
// console handler to actually exit.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn console_ctrl_handler(_ctrl_type: u32) -> BOOL {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    BOOL(1) // handled: don't run Windows' default action (immediate termination)
}

fn send_key(vk: VIRTUAL_KEY, key_up: bool) {
    let flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

// Purpose.md §6② step 3: on the transition BACK TO Default (from any other
// layer — not on every transition, and specifically not on the Default ->
// Layer1/Layer2 press edge), force-send KeyUp for every key still logically
// down so nothing stays stuck sending a non-Default layer's key after
// returning to Default. (If the physical key is still past T_ON afterwards,
// the next report re-presses it fresh under Default.) A key already held
// when Hyper Shift engages (leaving Default) deliberately keeps sending
// whatever it was pressed with for the rest of that hold — this is the
// original, hardware-verified momentary design, and generalizing it to fire
// on every transition (tried briefly during v1.0.5 development) turned out
// to be a real regression: it force-released+re-pressed an already-held key
// the instant Hyper Shift engaged, sending both the Default and the new
// layer's key back-to-back instead of a clean switch. Shared by the real
// driver loop and `emulate` mode (see emulate.rs) so both exercise this edge
// exactly the same way.
pub(crate) fn force_keyup_on_layer_change(
    pressed_vk: &mut [Option<VIRTUAL_KEY>; NUM_KEYS],
    start: Instant,
) {
    for (i, slot) in pressed_vk.iter_mut().enumerate() {
        if let Some(vk) = slot.take() {
            send_key(vk, true);
            println!(
                "[t={:>8.3}s] key{:02} UP   (forced: Hyper Shift layer changed)",
                start.elapsed().as_secs_f64(),
                i + 1
            );
        }
    }
}

fn layer_name(layer: usize) -> String {
    if layer == 0 {
        "Default".to_string()
    } else {
        format!("Layer{layer}")
    }
}

// Runs the hysteresis + keymap + SendInput decision for one already-parsed
// analog report: `depths[i]` is the 0-255 depth for key(i+1) (report ID 6,
// bytes[1..=NUM_KEYS]). `layer` is the active Hyper Shift layer index (0 =
// Default, per hypershift::CURRENT_LAYER — always < MAX_LAYERS). Shared by
// the real driver loop (fed from an actual HID read) and `emulate` mode (fed
// from a synthetic depth array typed at a terminal — see emulate.rs), so
// both exercise identical logic bit-for-bit.
pub(crate) fn process_key_depths(
    depths: &[u8; NUM_KEYS],
    layer: usize,
    pressed_vk: &mut [Option<VIRTUAL_KEY>; NUM_KEYS],
    start: Instant,
) {
    for i in 0..NUM_KEYS {
        let depth = depths[i];
        let (t_on, t_off) = cfg().actuation.for_key(i);
        if pressed_vk[i].is_none() && depth > t_on {
            let vk = cfg().analog.layers[layer][i];
            pressed_vk[i] = Some(vk);
            send_key(vk, false);
            println!(
                "[t={:>8.3}s] key{:02} DOWN (depth={:#04x}, layer={})",
                start.elapsed().as_secs_f64(),
                i + 1,
                depth,
                layer_name(layer)
            );
        } else if depth < t_off
            && let Some(vk) = pressed_vk[i].take()
        {
            send_key(vk, true);
            println!(
                "[t={:>8.3}s] key{:02} UP   (depth={:#04x})",
                start.elapsed().as_secs_f64(),
                i + 1,
                depth
            );
        }
    }
}

// Filters hidapi's device list down to the Tartarus Pro's collections that
// are actually readable: matching VID/PID, minus the two boot collections
// (Usage Page 0x01, Usage 0x02 "Mouse" / 0x06 "Keyboard") that Windows' HID
// class driver claims exclusively (ReadFile on them always fails with
// ACCESS_DENIED). Shared by open_analog_devices below (which exits the
// process if this comes back empty — fine for the driver's fail-fast CLI
// startup) and configui's try_open_analog_devices (which must fail soft
// instead, since it runs inside the long-lived config web server).
pub(crate) fn analog_device_infos(api: &HidApi) -> Vec<hidapi::DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
        .filter(|d| !(d.usage_page() == 0x0001 && (d.usage() == 0x0002 || d.usage() == 0x0006)))
        .cloned()
        .collect()
}

fn open_analog_devices(api: &HidApi) -> Vec<(i32, hidapi::HidDevice)> {
    let infos = analog_device_infos(api);

    if infos.is_empty() {
        eprintln!(
            "Tartarus Pro (VID {:#06x} / PID {:#06x}) not found. Is it plugged in?",
            VID, PID
        );
        std::process::exit(1);
    }

    let mut devices = Vec::new();
    for info in &infos {
        match info.open_device(api) {
            Ok(device) => {
                if let Err(e) = device.set_blocking_mode(false) {
                    eprintln!(
                        "[if{}] failed to set non-blocking mode: {e}",
                        info.interface_number()
                    );
                    continue;
                }
                devices.push((info.interface_number(), device));
            }
            Err(e) => {
                eprintln!(
                    "[if{}] failed to open (skipping): {e}",
                    info.interface_number()
                );
            }
        }
    }

    if devices.is_empty() {
        eprintln!("No interfaces could be opened.");
        std::process::exit(1);
    }

    devices
}

fn open_razer_control_device(api: &HidApi) -> Option<hidapi::HidDevice> {
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == 0x0001 && d.usage() == 0x0002)?
        .clone();
    match info.open_device(api) {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("[razer] Interface 2 (Razer Control Device) open failed: {e}");
            None
        }
    }
}

// Build an arbitrary razer_report (91 bytes incl. leading report-ID 0 byte).
// CRC = XOR of struct bytes 2..88 (i.e. buf[3..89] here, after the report-ID byte).
fn build_razer_cmd(txn: u8, class: u8, cmd: u8, args: &[u8]) -> [u8; 91] {
    let mut buf = [0u8; 91];
    buf[2] = txn;
    buf[6] = args.len() as u8; // data_size
    buf[7] = class;
    buf[8] = cmd;
    buf[9..9 + args.len()].copy_from_slice(args);
    let mut crc = 0u8;
    for b in &buf[3..89] {
        crc ^= *b;
    }
    buf[89] = crc;
    buf
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    init_log_file();
    println!("tartarus_driver v{VERSION}");

    let subcommand = env::args().nth(1);
    match subcommand.as_deref() {
        Some("configui") => {
            configui::run_configui_server();
            return;
        }
        Some("tray") => {
            run_tray_mode();
            return;
        }
        Some("emulate") => {
            emulate::run_emulator();
            return;
        }
        _ => {}
    }

    // Historical one-shot investigation subcommands (`razerheartbeat`,
    // `razermode`, `razerinit`/`razerburst`, `enumall`, and the earlier
    // `rawinputlog`) were removed 2026-07-20 once Phase 1-3 and the
    // Interception-based D-pad/wheel/middle-click remap were all fully
    // verified on real hardware and superseded them — their findings are
    // preserved in `tasks/research/` and this file's other doc comments
    // (e.g. the device-mode-3 unlock sequence right below, and the
    // Interception module doc comment above `run_interception_thread`).
    // Interception's own per-event "[dpad] Interception device N hardware
    // id: ... -> TARTARUS/other" log line gives the same device-
    // classification observability during normal operation that
    // `rawinputlog`/`enumall` used to provide standalone.

    // No argument (the normal day-to-day invocation) -> run indefinitely,
    // stopped only by Ctrl+C/console close (see console_ctrl_handler below).
    // An explicit numeric argument still time-boxes the run, as before —
    // useful for scripted tests. 0 explicitly also means "forever".
    let duration_secs: u64 = subcommand.and_then(|s| s.parse().ok()).unwrap_or(0);
    let run_forever = duration_secs == 0;

    unsafe {
        if SetConsoleCtrlHandler(Some(console_ctrl_handler), true).is_err() {
            eprintln!(
                "WARNING: failed to install Ctrl+C handler — stopping via Ctrl+C may leave a \
                 key stuck down if one happens to be held at that exact moment. Ctrl+C still \
                 works to end the process, just without the usual cleanup."
            );
        }
    }

    run_driver(run_forever, duration_secs);
}

// `tray` subcommand: a background, console-window-free mode with a system
// tray icon instead (see tray.rs). Detaches the console (best-effort —
// there may not even be one, e.g. if launched from a shortcut) so this
// doesn't leave a window open, starts configui's web server so the tray
// menu's "設定を開く" always has something to point the browser at, spawns
// the tray icon itself, then runs the exact same driver loop as the normal
// path — indefinitely, stopped by the tray menu's "終了" (or Ctrl+C, on the
// off chance a console is still attached after all).
fn run_tray_mode() {
    unsafe {
        let _ = FreeConsole();
    }

    std::thread::spawn(configui::run_configui_server);
    tray::spawn_tray_icon_thread();

    run_driver(true, 0);
}

// The actual analog-key-read + hysteresis + SendInput driver loop, shared by
// the normal (console) invocation and `tray` mode. `duration_secs` is
// ignored when `run_forever` is true.
fn run_driver(run_forever: bool, duration_secs: u64) {
    let api = HidApi::new().expect("hidapi init failed");

    // Reverse-engineered 2026-07-18 (see try_razer_mode / tasks/research
    // findings report): the device only streams analog reports on Interface 1
    // after Interface 2 (Razer Control Device) is told to enter "device mode 3"
    // via this class-0x00/cmd-0x04 feature report. Synapse sends this at
    // startup and mode 0 on exit; this is the *entire* lock/unlock mechanism —
    // no Synapse process needs to be running, we just need to send this once.
    let devices = open_analog_devices(&api);

    // Phase 4: load config.toml (or built-in placeholder defaults) once,
    // before either the Hypershift hook thread or the Interception thread
    // starts — both read it via cfg() and neither ever mutates it. Loaded
    // here (rather than after the unlock block below) because the lighting
    // command, if any is configured, is sent once at startup right
    // alongside the mode-3 unlock, using the same Interface 2 handle.
    CONFIG.set(config::load()).ok();

    // Kept open (not just a local inside this block) for the lifetime of the
    // function: the layer-indicator LED (below) needs to send a command on
    // every Hypershift press/release, using this same Interface 2 handle.
    let ctrl = open_razer_control_device(&api);
    match &ctrl {
        Some(ctrl) => {
            let cmd = build_razer_cmd(0x01, 0x00, 0x04, &[0x03, 0x00]);
            match ctrl.send_feature_report(&cmd) {
                Ok(()) => println!("Sent device-mode-3 unlock command to Interface 2."),
                Err(e) => eprintln!("WARNING: failed to send unlock command: {e} (analog data may not flow)"),
            }
            if let Some(lighting_cfg) = &cfg().lighting {
                lighting::apply(ctrl, lighting_cfg);
            }
            if let Some(indicator) = &cfg().layer_indicator {
                // Start in the "off" (Default layer) state; the loop below
                // sends the "on" state the moment Hypershift is first held.
                lighting::set_layer_indicator(ctrl, &indicator.color, false);
            }
        }
        None => eprintln!("WARNING: Interface 2 (Razer Control Device) not found; analog data may not flow."),
    }

    if run_forever {
        println!(
            "Opened {} HID interface(s). Running until Ctrl+C — press keys on the Tartarus Pro now.",
            devices.len()
        );
    } else {
        println!(
            "Opened {} HID interface(s). Running for {duration_secs}s — press keys on the Tartarus Pro now.",
            devices.len()
        );
    }

    // D-pad / wheel / middle-click remap via the Interception kernel driver
    // (see the module doc comment in dpad.rs for the full design, and
    // README.md "既知の制約" for driver install steps). Phase 3 (Purpose.md
    // §6②) Hypershift trigger detection now lives INSIDE this too (as of
    // 2026-07-21 — see handle_interception_keyboard in dpad.rs): the old
    // unconditional hook-based approach blocked Alt on every keyboard, not
    // just the Tartarus's, breaking real Alt+Tab while the driver ran.
    // dpad::run_interception_thread only falls back to
    // hypershift::spawn_hypershift_hook_thread() itself, internally, if
    // Interception isn't installed/running.
    dpad::spawn_interception_thread();

    // NOTE: reading these HidDevice handles from a *different* thread than the
    // one that opened them silently returned zero reports in testing on
    // Windows (2026-07-18) even though the exact same read loop works fine
    // on the opening thread. So for now this stays single-threaded: read +
    // hysteresis + SendInput all happen in the same loop. Revisit Purpose.md's
    // two-thread split later if this turns out to matter for latency.
    // Per-key "logically down" tracking. Some(vk) = down, storing the VK that
    // was actually sent at press time, so KeyUp (normal, forced-by-layer-exit,
    // or forced-at-shutdown) always releases under the keymap the key was
    // pressed with, even if the layer changed in between.
    let mut pressed_vk: [Option<VIRTUAL_KEY>; NUM_KEYS] = [None; NUM_KEYS];
    let mut layer_prev: usize = 0;
    let start = Instant::now();
    let deadline = Duration::from_secs(duration_secs);
    let mut buf = [0u8; 64];

    while !SHUTDOWN_REQUESTED.load(Ordering::SeqCst) && (run_forever || start.elapsed() < deadline) {
        let layer = hypershift::CURRENT_LAYER.load(Ordering::SeqCst) as usize;

        // On any Hyper Shift layer change: reflect it on the indicator LED
        // (if configured — on for any non-Default layer, off for Default;
        // TASK-009: unverified on real hardware whether this LED actually
        // lights, kept as a harmless opt-in regardless).
        if layer != layer_prev
            && let Some(ctrl) = &ctrl
            && let Some(indicator) = &cfg().layer_indicator
        {
            lighting::set_layer_indicator(ctrl, &indicator.color, layer != 0);
        }
        // Force-send KeyUp for every key still logically down, but ONLY on
        // the transition back to Default (from any other layer) — NOT on
        // every transition. This matches the original, hardware-verified
        // momentary design exactly (Purpose.md §6② step 3): a key already
        // held when Hyper Shift engages keeps sending whatever it was
        // pressed with for the rest of that hold, and only gets forcibly
        // reset when the layer returns to Default. v1.0.5 initially
        // generalized this to fire on EVERY transition (including the
        // Default->Layer1 press edge), which turned out to be a real
        // regression: an analog key already held under Default would get an
        // immediate KeyUp+KeyDown pair the instant Hyper Shift engaged,
        // visibly sending BOTH the Default and Layer1 key in quick
        // succession (e.g. "1" then "6") instead of a clean switch —
        // reverted back to this narrower, originally-verified condition.
        if layer_prev != 0 && layer == 0 {
            force_keyup_on_layer_change(&mut pressed_vk, start);
        }
        layer_prev = layer;

        for (_interface, device) in &devices {
            if let Ok(len) = device.read(&mut buf) {
                if len < 1 + NUM_KEYS || buf[0] != ANALOG_REPORT_ID {
                    continue;
                }
                let depths: [u8; NUM_KEYS] = buf[1..1 + NUM_KEYS].try_into().unwrap();
                process_key_depths(&depths, layer, &mut pressed_vk, start);
            }
        }
        std::thread::sleep(Duration::from_micros(500));
    }

    // Safety: force-release any key still held when the loop ends so we
    // never leave a stuck key pressed on the OS.
    for slot in pressed_vk.iter_mut() {
        if let Some(vk) = slot.take() {
            send_key(vk, true);
        }
    }
    dpad::release_held_dpad_test_keys();

    println!("Done.");
}
