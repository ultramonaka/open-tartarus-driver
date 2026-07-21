// System tray icon for the `tray` subcommand. Runs on its own thread with
// its own hidden window and message loop — the same "auxiliary thread, own
// message pump" pattern already used for the Hypershift hook thread
// (spawn_hypershift_hook_thread), just with a real (invisible) window
// instead of a message-only one, because Shell_NotifyIconW needs an HWND
// that can receive the taskbar callback message and TrackPopupMenu needs a
// window to anchor the popup to.
//
// Two menu items: "設定を開く" opens the configui web page (assumed already
// running — see run_tray_mode in main.rs, which starts it on its own thread
// before this one) in the default browser; "終了" sets the same
// SHUTDOWN_REQUESTED flag the console Ctrl handler uses, so the main
// analog-read loop exits through its normal cleanup path (force-releasing
// any held key) exactly the same way either shutdown trigger would.

use crate::{eprintln, println, SHUTDOWN_REQUESTED};
use std::sync::atomic::Ordering;
use windows::core::w;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, IDI_APPLICATION, MF_STRING, MSG,
    SW_SHOWNORMAL, TPM_LEFTALIGN, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP,
    WM_RBUTTONUP, WNDCLASSEXW, WS_OVERLAPPED,
};

const WM_TRAYICON: u32 = WM_APP + 1;
const IDM_OPEN_SETTINGS: usize = 1;
const IDM_QUIT: usize = 2;
const TRAY_ICON_ID: u32 = 1;
const CLASS_NAME: PCWSTR = w!("TartarusDriverTrayWindowClass");

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            // Pre-NIM_SETVERSION(4) behavior (which we never opt into): lParam
            // is simply the mouse event code, not packed with anything else.
            let event = lparam.0 as u32;
            if event == WM_LBUTTONUP || event == WM_RBUTTONUP {
                show_context_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                IDM_OPEN_SETTINGS => open_settings_page(),
                IDM_QUIT => {
                    println!("[tray] \"終了\"が選択されました。シャットダウンします。");
                    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let nid = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: TRAY_ICON_ID,
                ..Default::default()
            };
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN_SETTINGS, w!("設定を開く (configui)"));
        let _ = AppendMenuW(menu, MF_STRING, IDM_QUIT, w!("終了"));
        let mut point = Default::default();
        let _ = GetCursorPos(&mut point);
        // Undocumented-but-required Win32 gotcha for tray-icon popup menus:
        // without bringing the (invisible) window to the foreground first,
        // TrackPopupMenu's menu can fail to close when the user clicks
        // elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_LEFTALIGN, point.x, point.y, 0, hwnd, None);
    }
}

fn open_settings_page() {
    unsafe {
        // Fire-and-forget: ShellExecuteW's return value here is an HINSTANCE
        // (legacy ABI quirk), not worth inspecting — worst case the browser
        // just doesn't open, which the user notices immediately.
        let _ = ShellExecuteW(None, w!("open"), w!("http://127.0.0.1:7878/"), None, None, SW_SHOWNORMAL);
    }
}

pub fn spawn_tray_icon_thread() {
    std::thread::spawn(|| unsafe {
        let hinstance = GetModuleHandleW(None)
            .map(|m| HINSTANCE(m.0))
            .unwrap_or_default();

        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            eprintln!("[tray] WARNING: failed to register tray window class; tray icon disabled.");
            return;
        }

        let hwnd = match CreateWindowExW(
            Default::default(),
            CLASS_NAME,
            w!("Tartarus Driver"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[tray] WARNING: failed to create tray window: {e}; tray icon disabled.");
                return;
            }
        };

        let icon: HICON = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon,
            ..Default::default()
        };
        let tip: Vec<u16> = "Tartarus Driver\0".encode_utf16().collect();
        for (dst, src) in nid.szTip.iter_mut().zip(tip.iter()) {
            *dst = *src;
        }

        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            eprintln!("[tray] WARNING: failed to add tray icon.");
        } else {
            println!("[tray] トレイアイコンを表示しました。右クリックでメニュー(設定を開く / 終了)。");
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
