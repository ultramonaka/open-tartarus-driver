// Phase 3 (Purpose.md §6②): temporary layer-shift trigger.
//
// The Tartarus Pro's "Hyper Response" thumb button is invisible on every HID
// collection we can open — in its onboard binding it just sends a plain Alt
// keypress through the OS-reserved boot keyboard collection (Interface 0),
// which hidapi cannot read on Windows. So we detect it with a WH_KEYBOARD_LL
// hook on VK_MENU instead, and suppress the Alt event so it never reaches
// other applications ("トリガーキー自身のキーコードはOSに送信せずブロックする").

use crate::{eprintln, println};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LMENU, VK_MENU, VK_RMENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, HC_ACTION,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

// A static AtomicBool (not Arc/mpsc) because the hook procedure is a plain
// `extern "system"` fn that cannot capture state, and the single-threaded
// analog-read loop in main.rs's run_driver only needs a snapshot per
// iteration.
pub static HYPERSHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);

// Low-level keyboard hook procedure. Runs on the hook thread's message pump.
// Only Alt (VK_MENU / VK_LMENU / VK_RMENU) is intercepted; every other key is
// passed through untouched via CallNextHookEx. Returning a non-zero LRESULT
// without calling CallNextHookEx swallows the Alt event system-wide, which is
// intended: with the driver running, ALL Alt presses mean "Hypershift held"
// because the Hyper Response button is the thing sending Alt.
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
                WM_KEYDOWN | WM_SYSKEYDOWN => HYPERSHIFT_ACTIVE.store(true, Ordering::SeqCst),
                WM_KEYUP | WM_SYSKEYUP => HYPERSHIFT_ACTIVE.store(false, Ordering::SeqCst),
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
                "Hypershift hook installed: Alt (Hyper Response button) now toggles Layer1 and is blocked from other apps."
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
