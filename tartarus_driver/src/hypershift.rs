// Phase 3 (Purpose.md §6②), redesigned in v1.0.5: the physical "Hyper
// Response" thumb button.
//
// The button is invisible on every HID collection we can open — in its
// onboard binding it just sends a plain Alt keypress through the OS-reserved
// boot keyboard collection (Interface 0), which hidapi cannot read on
// Windows. So we detect it with a WH_KEYBOARD_LL hook on VK_MENU here (a
// fallback for when the Interception driver isn't installed — see dpad.rs's
// handle_interception_keyboard for the normal, device-aware detection path),
// and suppress the Alt event so it never reaches other applications
// ("トリガーキー自身のキーコードはOSに送信せずブロックする").
//
// v1.0.5 made what happens on each press/release edge configurable
// (config.toml's [hypershift]) instead of hardcoded to "toggle Layer1 while
// held": see on_trigger_edge below, the single place both this hook and
// dpad.rs's Interception path route every edge through.
//   - mode = "layer_switch" (default): switch_style = "momentary" (default)
//     behaves exactly as this project always has (held -> Layer1, released ->
//     Default); switch_style = "toggle" instead cycles through layer_count
//     (2 or 3) layers, advancing on every press.
//   - mode = "modifier_key": the button is just a plain key (default Alt,
//     config.toml's hypershift.modifier_key) sent on press/release, with no
//     layer switching at all.

use crate::config::{HypershiftMode, SwitchStyle};
use crate::{eprintln, println};
use std::sync::atomic::{AtomicU8, Ordering};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LMENU, VK_MENU, VK_RMENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, HC_ACTION,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// The active Hyper Shift layer (0 = Default, 1 = Layer1, 2 = Layer2), always
// < main.rs::MAX_LAYERS. Only meaningful/updated when [hypershift] mode =
// "layer_switch" (see on_trigger_edge); stays 0 forever in "modifier_key"
// mode. A static AtomicU8 (not Arc/mpsc) because the hook procedure below is
// a plain `extern "system"` fn that cannot capture state, and the
// single-threaded analog-read loop in main.rs's run_driver only needs a
// snapshot per iteration.
pub static CURRENT_LAYER: AtomicU8 = AtomicU8::new(0);

// Routes one physical Hyper Response press (`pressed = true`) or release
// (`false`) edge through the configured mode/switch_style. The single place
// both detection paths (this file's hook fallback, and dpad.rs's normal
// Interception-based device-aware detection) funnel every edge through, so
// the mode dispatch logic exists exactly once.
pub fn on_trigger_edge(pressed: bool) {
    on_trigger_edge_with(pressed, &crate::cfg().hypershift);
}

// The actual state machine, taking its config by reference instead of
// reading the process-wide `cfg()` OnceLock directly — this is what makes it
// unit-testable with an arbitrary HypershiftConfig (momentary/toggle/
// modifier_key, any layer_count) without needing to fight the fact that
// CONFIG can only ever be initialized once per test binary. `pub(crate)`
// (not private) specifically so emulate.rs's test module can drive it
// directly — see the doc comment on its test for why those assertions live
// there instead of here (both touch the CURRENT_LAYER static above, which
// must only ever be exercised by one test function crate-wide).
pub(crate) fn on_trigger_edge_with(pressed: bool, hs: &crate::config::HypershiftConfig) {
    match hs.mode {
        HypershiftMode::ModifierKey => {
            // Pure passthrough: no layer change, ever. For the default
            // modifier_key (LALT), this is byte-for-byte what already
            // happens today in layer_switch/momentary mode's press path
            // (physical Alt suppressed at the hook/Interception level,
            // synthetic Alt sent here) — just without the layer switch.
            crate::send_key(hs.modifier_key, !pressed);
            println!(
                "[hypershift] Hyper Response {} -> sent {} (modifier_key mode)",
                if pressed { "DOWN" } else { "UP  " },
                crate::vkname::vk_to_name(hs.modifier_key)
            );
        }
        HypershiftMode::LayerSwitch => match hs.switch_style {
            SwitchStyle::Momentary => {
                let layer = if pressed { 1 } else { 0 };
                CURRENT_LAYER.store(layer, Ordering::SeqCst);
                println!(
                    "[hypershift] Hyper Response {} -> layer {} (momentary)",
                    if pressed { "DOWN" } else { "UP  " },
                    layer
                );
            }
            SwitchStyle::Toggle => {
                // Release is inert; only a press advances the cycle.
                if pressed {
                    let n = hs.layer_count.max(2);
                    let next = (CURRENT_LAYER.load(Ordering::SeqCst) + 1) % n;
                    CURRENT_LAYER.store(next, Ordering::SeqCst);
                    println!("[hypershift] Hyper Response press -> layer {next} (toggle, {n} layers)");
                }
            }
        },
    }
}

// Low-level keyboard hook procedure. Runs on the hook thread's message pump.
// Only Alt (VK_MENU / VK_LMENU / VK_RMENU) is intercepted; every other key is
// passed through untouched via CallNextHookEx. Returning a non-zero LRESULT
// without calling CallNextHookEx swallows the Alt event system-wide — this
// hook is only ever active when Interception (which CAN tell the Tartarus's
// Alt apart from a real keyboard's) isn't available, so blocking every
// keyboard's Alt here is a known, documented limitation of this fallback
// path only (see dpad.rs's fall_back_to_hook_based_hypershift).
unsafe extern "system" fn hypershift_hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = event.vkCode as u16;
        if vk == VK_MENU.0 || vk == VK_LMENU.0 || vk == VK_RMENU.0 {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => on_trigger_edge(true),
                WM_KEYUP | WM_SYSKEYUP => on_trigger_edge(false),
                _ => {}
            }
            return LRESULT(1); // block the trigger key from reaching the OS
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

// Installs the WH_KEYBOARD_LL hook on a dedicated thread with its own message
// pump (SetWindowsHookExW requires a live GetMessage loop on the installing
// thread). This is a separate thread from the analog HID read loop, which is
// fine: it only touches Win32 hook APIs, never a HidDevice handle, so the
// documented "HID reads silently return nothing from a non-opening thread"
// restriction does not apply here. The thread runs for the process lifetime;
// Windows removes the hook automatically when the process exits.
pub fn spawn_hypershift_hook_thread() {
    std::thread::spawn(|| unsafe {
        let hinstance = GetModuleHandleW(None)
            .map(|module| HINSTANCE(module.0))
            .unwrap_or_default();
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hypershift_hook_proc), hinstance, 0) {
            Ok(_hook) => println!(
                "Hypershift hook installed: Alt (Hyper Response button) is now blocked from other \
                 apps and routed through on_trigger_edge (see config.toml's [hypershift])."
            ),
            Err(e) => {
                eprintln!("WARNING: failed to install WH_KEYBOARD_LL hook: {e} (Hypershift disabled)");
                return;
            }
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
