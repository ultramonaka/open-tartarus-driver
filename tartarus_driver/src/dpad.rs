// ===========================================================================
// D-pad / scroll-wheel / wheel-click remap via the Interception kernel driver
// ===========================================================================
//
// The Tartarus Pro's 8-way D-pad sends plain arrow keys through an
// OS-reserved boot keyboard collection, its scroll wheel sends a standard
// mouse wheel, and the wheel-click sends a standard middle mouse button
// (confirmed empirically with Synapse fully closed — onboard bindings).
//
// PREVIOUS APPROACH AND WHY IT WAS REPLACED (2026-07-20): the original
// implementation used a Raw Input sink (WM_INPUT, for device identification)
// plus WH_KEYBOARD_LL / WH_MOUSE_LL low-level hooks (for suppression, since
// Raw Input cannot block an event from reaching other applications). This
// worked perfectly for the wheel/middle-click path, confirmed on real
// hardware with zero double input. It could NOT be made to fully work for
// the D-pad arrow-key path: real-hardware testing proved WH_KEYBOARD_LL
// always fires strictly before the matching WM_INPUT can be processed, even
// on the same thread with zero cross-thread latency — this is Windows' own
// low-level-hook-precedes-queued-message ordering, not a timing bug, and no
// amount of thread/queue restructuring could change it (see git history for
// the full verdict-queue design this replaces). That ordering guarantee is a
// structural property of user-mode Win32 APIs; the only way around it is a
// component that sits BELOW both hooks and Raw Input in the HID stack — a
// kernel-mode filter driver. Razer Synapse itself solves the identical
// problem with its own kernel driver (RzFilter.sys).
//
// NEW APPROACH: the Interception driver (github.com/oblitum/Interception,
// third-party, MUST be installed separately by a human with admin rights —
// see README.md "既知の制約" for the install steps) intercepts every
// keyboard/mouse event in kernel mode before Windows delivers it anywhere.
// Our code (via the `interception` Rust crate, a safe wrapper over the
// `interception-sys` FFI bindings) does exactly what the official samples
// (samples/hardwareid, samples/x2y in the upstream repo) do:
//   1. Open one InterceptionContext and set a broad filter capturing every
//      keyboard down/up/E0/E1 event and the mouse wheel/middle-button
//      events — NOT identified by device yet; filtering by hardware id has
//      to happen per-event because the driver's filter predicate only ever
//      receives a small stable per-session device index, not device info.
//   2. Block in ctx.wait()/ctx.receive() for the next captured stroke.
//   3. Look up (and cache) that device's hardware id string via
//      ctx.get_hardware_id() and check it for "VID_1532"+"PID_0244" — the
//      exact same substring check the old raw_input_device_is_tartarus used
//      on the Raw Input device path (see TARTARUS_HWID_MARKERS below).
//   4. Tartarus-confirmed events: do NOT forward the original stroke (this
//      is what "receive but don't send" means with this driver — dropping
//      it here is a real, unconditional, kernel-level suppression, unlike
//      the old best-effort hook race) and instead emit the placeholder test
//      key via the existing SendInput-based send_key()/dpad_send_test_key()
//      helpers, exactly as before.
//   5. Anything else — a different device, or a hardware id we failed to
//      read — is forwarded via ctx.send() completely unmodified. This is
//      the single fail-open point for this whole subsystem.
//
// Because receive()/send() happens synchronously in this one thread for one
// event at a time, there is no verdict queue, no cross-thread race, and no
// possibility of the old "hook fires before the sink" ordering problem: by
// construction, our code has already made its suppress/forward decision
// before the stroke can go anywhere else.
//
// FAIL-OPEN POLICY (safety-critical, unchanged from the Raw Input version):
// whenever anything is uncertain — the Interception driver isn't installed,
// context creation fails, a hardware id can't be read, the device is not a
// positively-identified Tartarus — the event is passed through unmodified.
// The worst possible failure mode is therefore "the Tartarus D-pad
// occasionally acts as plain arrow keys", never "the user's real
// keyboard/mouse stops working".
//
// DRIVER-NOT-INSTALLED HANDLING: interception-sys links dynamically against
// interception.dll (the driver's userland library; a different binary from
// the interception.sys kernel driver itself). Two things are needed for this
// EXE to degrade gracefully when Interception is entirely unavailable — e.g.
// on any machine (including this project's own dev/build machine) where a
// human has not yet run the Interception installer:
//   1. build.rs delay-loads interception.dll so a missing DLL cannot prevent
//      the EXE from LAUNCHING at all (Windows would otherwise refuse to
//      start the process the moment ANY statically-linked import can't be
//      resolved, before main() ever runs — taking down Phase 1/2/3 along
//      with the D-pad remap).
//   2. run_interception_thread() pre-flights interception.dll's presence
//      itself with a plain LoadLibraryW call before ever calling into the
//      interception crate. This was added after empirically verifying
//      (2026-07-20, this exact machine, no Interception installed) that
//      simply calling Interception::new() with the DLL entirely missing
//      crashes the process outright — the delay-load failure is a Windows
//      structured exception, not a Rust panic, and is NOT caught by
//      std::panic::catch_unwind. See the doc comment on
//      run_interception_thread for the full explanation.
// "Interception::new() returned None" (DLL present, but the interception.sys
// kernel driver/service itself is not installed or not running — the likely
// case once steps 1/2 above are also satisfied) is a separate, already-safe
// path: it is a normal Option, not a crash of any kind.

use crate::config::DpadKeymap;
use crate::{cfg, eprintln, println, send_key, vkname};
use interception::{
    is_invalid, is_keyboard, is_mouse, Device, Filter, Interception, KeyFilter, KeyState,
    MouseFilter, MouseFlags, MouseState, ScanCode, Stroke,
};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use windows::core::w;
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

// Scan codes for the arrow-key cluster. The interception crate's `ScanCode`
// enum only names the BASE (non-extended) PC/XT set — arrow keys reuse the
// same numeric codes as the numpad's 8/4/6/2 (see
// https://handmade.network/wiki/2823, cited by interception::scancode
// itself) and are distinguished from real numpad presses purely by the E0
// ("extended key") flag in KeyState. A real physical keyboard's numpad keys
// (NumLock off) also produce E0-flagged 8/4/6/2 in some configurations, but
// that keyboard is a DIFFERENT Interception device than the Tartarus, so it
// is never touched regardless — see from_tartarus in
// handle_interception_keyboard.
const SCANCODE_UP: u16 = ScanCode::Numpad8 as u16;
const SCANCODE_LEFT: u16 = ScanCode::Numpad4 as u16;
const SCANCODE_RIGHT: u16 = ScanCode::Numpad6 as u16;
const SCANCODE_DOWN: u16 = ScanCode::Numpad2 as u16;

// Alt scan code (docs/DESIGN.md §6②, Hypershift). Unlike the arrow-key/numpad
// overlap above, Left Alt and Right Alt share this SAME raw code (0x38) on
// the base PC/XT set — Right Alt is just the E0-extended version of it, the
// same convention as Right Ctrl vs Left Ctrl — so no E0 check is needed
// here: either variant means "some Alt key", and the Hyper Response button
// only ever sends one specific one anyway (which one was never determined;
// treating both identically since it's harmless — see 2026-07-21 migration
// notes on handle_interception_keyboard below).
const SCANCODE_ALT: u16 = ScanCode::LeftAlt as u16;

// TEST/PLACEHOLDER D-pad keymap — same throwaway style as main.rs's
// TEST_KEYMAP. The letters deliberately avoid everything TEST_KEYMAP
// ('1'..'0', 'A'..'J') and LAYER1_TEST_KEYMAP (F1..F20) already use (which
// rules out the obvious W/A/S/D set: A and D are taken), so remapped output
// is unambiguous during testing. These are the config module's built-in
// defaults for the D-pad/wheel/middle-click (see config::DriverConfig::defaults()).
pub const DPAD_ARROW_TEST_KEYMAP_LEFT: VIRTUAL_KEY = VIRTUAL_KEY(0x4B); // 'K'
pub const DPAD_ARROW_TEST_KEYMAP_UP: VIRTUAL_KEY = VIRTUAL_KEY(0x57); // 'W'
pub const DPAD_ARROW_TEST_KEYMAP_RIGHT: VIRTUAL_KEY = VIRTUAL_KEY(0x4C); // 'L'
pub const DPAD_ARROW_TEST_KEYMAP_DOWN: VIRTUAL_KEY = VIRTUAL_KEY(0x53); // 'S'
pub const WHEEL_UP_TEST_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x4F); // wheel up -> 'O' (tap per notch)
pub const WHEEL_DOWN_TEST_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x50); // wheel down -> 'P' (tap per notch)
pub const MIDDLE_CLICK_TEST_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0x4D); // middle click -> 'M' (held)

// Maps an arrow scan code to its configured remap key (Phase 4: from
// config.toml, or the built-in placeholder defaults if unset — see
// config/load.rs). Returns None for anything that is not one of the four arrow
// scan codes (including plain numpad presses without the E0 flag, which the
// caller filters separately).
fn dpad_arrow_test_key_for(scancode: u16, dpad: &DpadKeymap) -> Option<VIRTUAL_KEY> {
    match scancode {
        SCANCODE_LEFT => Some(dpad.left),
        SCANCODE_UP => Some(dpad.up),
        SCANCODE_RIGHT => Some(dpad.right),
        SCANCODE_DOWN => Some(dpad.down),
        _ => None,
    }
}

// Substrings that must ALL appear in an Interception hardware id string for
// a device to be treated as the Tartarus, e.g.
// "\\?\HID#VID_1532&PID_0244&MI_00#a&146b6398&0&0000#{...}" (compared
// upper-cased) — the exact same check the pre-Interception Raw Input code
// used on GetRawInputDeviceInfoW's RIDI_DEVICENAME string, just sourced from
// ctx.get_hardware_id() instead. Deliberately VID+PID only (no MI_xx): every
// collection of the physical device is ours, and requiring the interface
// number would only add ways to under-match.
const TARTARUS_HWID_MARKERS: [&str; 2] = ["VID_1532", "PID_0244"];

// Pure string-matching logic factored out of interception_device_is_tartarus
// so it is unit-testable without a live Interception context/driver.
fn hwid_string_is_tartarus(hardware_id: &str) -> bool {
    let upper = hardware_id.to_ascii_uppercase();
    TARTARUS_HWID_MARKERS.iter().all(|m| upper.contains(m))
}

// Remapped keys currently held down (D-pad arrows / middle-click), so
// main.rs's run_driver can force-release them at shutdown exactly like the
// analog keys.
static DPAD_HELD_TEST_KEYS: Mutex<Vec<VIRTUAL_KEY>> = Mutex::new(Vec::new());

// Interception device index -> is-Tartarus verdict cache. Unlike the old Raw
// Input hDevice (which Windows could recycle across an unplug, requiring an
// explicit WM_INPUT_DEVICE_CHANGE cache-clear), Interception's device
// indices are fixed virtual "slots" assigned once per driver session and
// stay associated with the same physical port for the lifetime of the OS
// session, so no invalidation logic is needed here.
static DPAD_DEVICE_CACHE: LazyLock<Mutex<HashMap<Device, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn dpad_send_test_key(vk: VIRTUAL_KEY, key_up: bool) {
    send_key(vk, key_up);
    let Ok(mut held) = DPAD_HELD_TEST_KEYS.lock() else {
        return;
    };
    if key_up {
        held.retain(|k| *k != vk);
    } else if !held.contains(&vk) {
        held.push(vk);
    }
}

// Shutdown safety: force-release any remapped D-pad/middle-click key still
// logically held so we never leave a stuck key on the OS (mirrors the analog
// force-release at the end of run_driver in main.rs).
pub fn release_held_dpad_test_keys() {
    let held: Vec<VIRTUAL_KEY> = match DPAD_HELD_TEST_KEYS.lock() {
        Ok(mut h) => h.drain(..).collect(),
        Err(_) => return,
    };
    for vk in held {
        send_key(vk, true);
    }
}

// Resolve (and cache) whether an Interception device index is the Tartarus.
// Any lookup failure -> false (fail open) and NOT cached, so a transient
// error can never permanently misclassify a device.
fn interception_device_is_tartarus(ctx: &Interception, device: Device) -> bool {
    if is_invalid(device) {
        return false;
    }
    if let Ok(cache) = DPAD_DEVICE_CACHE.lock()
        && let Some(known) = cache.get(&device)
    {
        return *known;
    }
    // Windows device instance id strings are well under this length; sized
    // generously (matches the upstream samples/hardwareid.cpp sample's
    // 500-wchar_t / 1000-byte buffer).
    let mut buf = [0u8; 2048];
    let written = ctx.get_hardware_id(device, &mut buf) as usize;
    // written >= buf.len() means the id was truncated (buffer too small):
    // per the upstream sample's own check, that result must not be trusted.
    if written == 0 || written >= buf.len() {
        return false; // fail open: cannot read hardware id
    }
    // Hardware id is a null-terminated UTF-16LE string (same convention as
    // the old Raw Input RIDI_DEVICENAME path).
    let words: Vec<u16> = buf[..written]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let name = String::from_utf16_lossy(&words)
        .trim_end_matches('\0')
        .to_string();
    let is_tartarus = hwid_string_is_tartarus(&name);
    // Only log the actual hardware id string for the positively-identified
    // Tartarus: other devices' VID/PID/GUID paths are the user's unrelated
    // keyboards/mice and have no reason to be recorded, even to a local,
    // gitignored log file.
    if is_tartarus {
        println!(
            "[dpad] Interception device {device} hardware id: {name} -> \
             TARTARUS (D-pad/wheel/middle-click events will be remapped)"
        );
    } else {
        println!("[dpad] Interception device {device} -> other device (never touched)");
    }
    if let Ok(mut cache) = DPAD_DEVICE_CACHE.lock() {
        cache.insert(device, is_tartarus);
    }
    is_tartarus
}

// Handles one received keyboard stroke. Confirmed-Tartarus arrow strokes are
// suppressed (never forwarded) and remapped to the placeholder test key via
// the existing SendInput-based helper; a confirmed-Tartarus Alt press/
// release (the Hyper Response / Hypershift trigger) is suppressed and routed
// through hypershift::on_trigger_edge (which layer/key it produces depends on
// config.toml's [hypershift] — see hypershift.rs); everything else —
// non-arrow/non-Alt keys, plain (non-E0) numpad presses, and ANY event from a
// device that is not a positively-identified Tartarus — is forwarded
// unmodified via ctx.send() (fail open).
//
// 2026-07-21: Hypershift's Alt detection MOVED HERE from a global
// WH_KEYBOARD_LL hook (see hypershift.rs) specifically to fix a real-world
// bug the user hit: that hook couldn't distinguish which keyboard sent an
// Alt keypress (Win32 hooks carry no per-device origin), so it suppressed
// EVERY keyboard's Alt system-wide, breaking Alt+Tab from the user's actual
// keyboard while the driver ran. Interception CAN distinguish devices (that
// is the entire reason it exists for the D-pad), so gating on `from_tartarus`
// here means only the Tartarus's own Alt is ever touched — a real keyboard's
// Alt is always forwarded unmodified, restoring normal Alt+Tab.
// hypershift.rs's hook thread is now only a FALLBACK for when Interception
// itself isn't installed/running (see run_interception_thread) — this
// function's Alt handling and that hook's Alt handling are mutually
// exclusive at runtime, never both active, so on_trigger_edge is never
// double-fired by two sources for the same physical press.
fn handle_interception_keyboard(
    ctx: &Interception,
    device: Device,
    code: ScanCode,
    state: KeyState,
    information: u32,
    from_tartarus: bool,
    dpad: &DpadKeymap,
) {
    let raw_code = code as u16;

    if from_tartarus && raw_code == SCANCODE_ALT {
        let key_down = !state.contains(KeyState::UP);
        println!(
            "[dpad] Tartarus Hyper Response (Alt) {} edge detected",
            if key_down { "DOWN" } else { "UP  " }
        );
        crate::hypershift::on_trigger_edge(key_down);
        // Original stroke intentionally NOT forwarded: suppressed at the
        // driver, exactly like the D-pad arrows below — the trigger key's
        // own keycode never reaches the OS (docs/DESIGN.md §6② requirement).
        return;
    }

    let is_arrow = state.contains(KeyState::E0) && dpad_arrow_test_key_for(raw_code, dpad).is_some();
    if !is_arrow || !from_tartarus {
        ctx.send(
            device,
            &[Stroke::Keyboard {
                code,
                state,
                information,
            }],
        );
        return;
    }
    let key_up = state.contains(KeyState::UP);
    let mapped = dpad_arrow_test_key_for(raw_code, dpad).expect("is_arrow checked this above");
    dpad_send_test_key(mapped, key_up);
    println!(
        "[dpad] Tartarus D-pad arrow scancode={raw_code:#04x} {} -> test key vk={:#04x}",
        if key_up { "UP  " } else { "DOWN" },
        mapped.0
    );
    // Original stroke intentionally NOT forwarded: suppressed at the driver.
}

// Handles one received mouse stroke (wheel or middle button, per the filter
// installed in run_interception_thread — nothing else is ever captured).
// Same fail-open / suppress-and-remap structure as the keyboard handler.
#[allow(clippy::too_many_arguments)]
fn handle_interception_mouse(
    ctx: &Interception,
    device: Device,
    state: MouseState,
    flags: MouseFlags,
    rolling: i16,
    x: i32,
    y: i32,
    information: u32,
    from_tartarus: bool,
    dpad: &DpadKeymap,
) {
    if !from_tartarus {
        ctx.send(
            device,
            &[Stroke::Mouse {
                state,
                flags,
                rolling,
                x,
                y,
                information,
            }],
        );
        return;
    }
    if state.contains(MouseState::WHEEL) {
        let mapped = if rolling >= 0 { dpad.wheel_up } else { dpad.wheel_down };
        // One key tap (down + up) per wheel notch.
        send_key(mapped, false);
        send_key(mapped, true);
        println!(
            "[dpad] Tartarus wheel rolling={rolling} -> test key vk={:#04x} tap",
            mapped.0
        );
    }
    if state.contains(MouseState::MIDDLE_BUTTON_DOWN) {
        dpad_send_test_key(dpad.middle_click, false);
        println!(
            "[dpad] Tartarus middle DOWN -> test key vk={:#04x}",
            dpad.middle_click.0
        );
    }
    if state.contains(MouseState::MIDDLE_BUTTON_UP) {
        dpad_send_test_key(dpad.middle_click, true);
        println!(
            "[dpad] Tartarus middle UP   -> test key vk={:#04x}",
            dpad.middle_click.0
        );
    }
    // Original stroke intentionally NOT forwarded: suppressed at the driver.
}

// interception-sys links interception.dll (the driver's userland library —
// see build.rs) as a normal, non-delay-loaded-by-Rust-code call target once
// resolved; build.rs's /DELAYLOAD flag only defers resolution from process
// LAUNCH time to FIRST CALL time, so a missing DLL cannot prevent this EXE
// from starting. It does NOT, by itself, make that first call fail
// gracefully: empirically verified (2026-07-20, this exact toolchain, no
// Interception installed) that calling Interception::new() with the DLL
// entirely absent terminates the process immediately with no panic message
// and is NOT caught by std::panic::catch_unwind — delay-load failures raise
// a Windows structured exception, a different mechanism from a Rust panic,
// and MSVC delay-load's default unhandled behavior is to let it crash the
// process. So this function pre-flights the DLL's presence itself, with a
// plain, safe LoadLibraryW call (ordinary Result-based failure, same
// pattern already used for GetModuleHandleW elsewhere in this file, and NOT
// going through the delay-load failure path at all) BEFORE ever touching
// the interception crate. If interception.dll cannot be found anywhere on
// the search path, this prints one clear warning and returns — every other
// subsystem (analog keys, Hypershift) keeps running. If the DLL IS found,
// this call also leaves it loaded (never freed) so the delay-load thunk
// resolves it trivially moments later inside Interception::new(), which can
// then only fail through its own normal, safe Option::None path (DLL
// present, but the interception.sys kernel driver/service itself is not
// installed/running — the far more likely case once a human has actually
// run the installer's DLL-copy step but not yet started the driver
// service). catch_unwind is kept around Interception::new() too as a
// defense-in-depth belt-and-suspenders measure; it does not replace the
// LoadLibraryW pre-flight, which is the actual fix.
// All three ways Interception can be unavailable (DLL missing, driver
// service not installed/running, or an unexpected panic from the FFI call)
// end the same way: warn with the specific cause, then fall back to the
// device-blind WH_KEYBOARD_LL hook so Hypershift still works (just without
// per-device discrimination — see hypershift.rs). USAGE.md's troubleshooting
// section tells users to grep the log for this exact trailing phrase, so it
// must stay byte-for-byte stable across all three causes.
fn fall_back_to_hook_based_hypershift(cause: &str) {
    eprintln!(
        "WARNING: {cause} (see README.md \"既知の制約\" for install steps). D-pad/wheel/\
         middle-click remap disabled; falling back to the hook-based Hypershift detection \
         (blocks Alt on ALL keyboards while running, see hypershift.rs for why)."
    );
    crate::hypershift::spawn_hypershift_hook_thread();
}

fn run_interception_thread() {
    unsafe {
        if let Err(e) = LoadLibraryW(w!("interception.dll")) {
            fall_back_to_hook_based_hypershift(&format!(
                "interception.dll could not be loaded ({e}) — the Interception driver does not \
                 appear to be installed"
            ));
            return;
        }
        // leave the DLL loaded; falls through to Interception::new() below
    }

    let new_result = std::panic::catch_unwind(Interception::new);
    let ctx = match new_result {
        Ok(Some(ctx)) => ctx,
        Ok(None) => {
            fall_back_to_hook_based_hypershift(
                "Interception::new() returned None — the Interception kernel driver does not \
                 appear to be installed/running",
            );
            return;
        }
        Err(_) => {
            fall_back_to_hook_based_hypershift(
                "Interception::new() failed unexpectedly — the Interception driver is most \
                 likely not installed or not running",
            );
            return;
        }
    };

    ctx.set_filter(is_keyboard, Filter::KeyFilter(KeyFilter::all()));
    ctx.set_filter(
        is_mouse,
        Filter::MouseFilter(
            MouseFilter::WHEEL | MouseFilter::MIDDLE_BUTTON_DOWN | MouseFilter::MIDDLE_BUTTON_UP,
        ),
    );
    println!(
        "Interception driver initialized: D-pad/wheel/middle-click remap + device-aware \
         Hypershift running (Tartarus D-pad -> {}/{}/{}/{}, wheel -> {}/{}, middle-click -> {}). \
         A real keyboard's Alt is never touched (fixed 2026-07-21 — see dpad.rs's \
         handle_interception_keyboard for why this used to block real Alt+Tab).",
        vkname::vk_to_name(cfg().dpad.left),
        vkname::vk_to_name(cfg().dpad.up),
        vkname::vk_to_name(cfg().dpad.right),
        vkname::vk_to_name(cfg().dpad.down),
        vkname::vk_to_name(cfg().dpad.wheel_up),
        vkname::vk_to_name(cfg().dpad.wheel_down),
        vkname::vk_to_name(cfg().dpad.middle_click),
    );

    // Placeholder stroke, overwritten in place by ctx.receive() below.
    let mut strokes = [Stroke::Keyboard {
        code: ScanCode::Esc,
        state: KeyState::empty(),
        information: 0,
    }];
    loop {
        let device = ctx.wait();
        if is_invalid(device) {
            continue; // fail open: nothing we can identify or act on
        }
        if ctx.receive(device, &mut strokes) <= 0 {
            continue;
        }
        let from_tartarus = interception_device_is_tartarus(&ctx, device);
        let dpad = &cfg().dpad;
        match strokes[0] {
            Stroke::Keyboard {
                code,
                state,
                information,
            } => handle_interception_keyboard(&ctx, device, code, state, information, from_tartarus, dpad),
            Stroke::Mouse {
                state,
                flags,
                rolling,
                x,
                y,
                information,
            } => handle_interception_mouse(
                &ctx,
                device,
                state,
                flags,
                rolling,
                x,
                y,
                information,
                from_tartarus,
                dpad,
            ),
        }
    }
}

// Spawns the Interception thread. Fire-and-forget, same lifetime pattern as
// hypershift::spawn_hypershift_hook_thread: runs for the process lifetime,
// never joined.
pub fn spawn_interception_thread() {
    std::thread::spawn(run_interception_thread);
}

// Pure-logic sanity checks for the Interception-based D-pad/wheel/middle-
// click remap, runnable with `cargo test` and requiring neither Tartarus
// hardware nor the Interception driver itself: these only exercise the
// scan-code mapping table and the hardware-id substring matcher, both plain
// functions with no dependency on a live InterceptionContext.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_scancodes_map_to_the_expected_test_keys() {
        let dpad = crate::config::DriverConfig::defaults().dpad;
        assert_eq!(
            dpad_arrow_test_key_for(SCANCODE_LEFT, &dpad),
            Some(DPAD_ARROW_TEST_KEYMAP_LEFT)
        );
        assert_eq!(
            dpad_arrow_test_key_for(SCANCODE_UP, &dpad),
            Some(DPAD_ARROW_TEST_KEYMAP_UP)
        );
        assert_eq!(
            dpad_arrow_test_key_for(SCANCODE_RIGHT, &dpad),
            Some(DPAD_ARROW_TEST_KEYMAP_RIGHT)
        );
        assert_eq!(
            dpad_arrow_test_key_for(SCANCODE_DOWN, &dpad),
            Some(DPAD_ARROW_TEST_KEYMAP_DOWN)
        );
    }

    #[test]
    fn non_arrow_scancodes_are_not_mapped() {
        // ScanCode::A, an ordinary letter key — never an arrow under any flag
        // combination, so callers must fail open (forward unmodified).
        let dpad = crate::config::DriverConfig::defaults().dpad;
        assert_eq!(dpad_arrow_test_key_for(ScanCode::A as u16, &dpad), None);
    }

    #[test]
    fn hwid_matches_only_when_both_vid_and_pid_markers_are_present() {
        assert!(hwid_string_is_tartarus(
            r"\??\HID#VID_1532&PID_0244&MI_00#a&146b6398&0&0000#{884b96c3-56ef-11d1-bc8c-00a0c91405dd}"
        ));
        // Case-insensitivity: the raw string may come back mixed-case.
        assert!(hwid_string_is_tartarus(
            r"\??\hid#vid_1532&pid_0244&mi_00#..."
        ));
    }

    #[test]
    fn hwid_fails_open_when_only_one_marker_matches() {
        // Right VID, wrong PID: a different Razer device must never match.
        assert!(!hwid_string_is_tartarus(r"\??\HID#VID_1532&PID_0000#..."));
        // Wrong VID entirely (some other vendor's keyboard/mouse).
        assert!(!hwid_string_is_tartarus(r"\??\HID#VID_046D&PID_C52B#..."));
    }
}
