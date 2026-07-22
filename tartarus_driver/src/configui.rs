// `configui` subcommand: a local-only HTTP server (127.0.0.1 only, no
// external network exposure) serving a single-page key-remap editor. Reads
// the current config.toml (or built-in defaults if none exists) on every GET
// so the page always reflects what's actually on disk, and validates +
// overwrites config.toml wholesale on save. This is a separate, short-lived
// process invocation (`cargo run --release -- configui`) from the normal
// analog-read driver loop — the two are never expected to run at the same
// time, and there is no live hot-reload: after saving, the user restarts the
// normal `tartarus_driver` run for the new mapping to take effect.

use crate::config::{ConfigPayload, DriverConfig};
use crate::vkname::all_key_names;
use crate::{eprintln, println, NUM_KEYS};
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server};

const PORT: u16 = 7878;
const ASSET_HTML: &str = include_str!("../assets/configui.html");

// The real payload is at most a few hundred bytes (30ish short key names as
// JSON); 64 KiB is generous headroom while still bounding memory use no
// matter what a caller claims via Content-Length or how much it actually
// sends (see read_body_capped, which caps the READ itself, not just the
// declared length).
const MAX_BODY_BYTES: u64 = 64 * 1024;

// Local-only defense in depth against a malicious webpage abusing the
// browser to POST to this server (CSRF) or DNS-rebinding a hostname to
// 127.0.0.1 to bypass same-origin checks: this server has no auth (it's a
// single-user local tool), so these are the only checks standing between
// "some other tab in your browser" and an unwanted config.toml write. A
// non-browser client (curl, a script) sends neither header, which is
// deliberately still allowed — this is not trying to authenticate the
// caller, only to block the browser-CSRF shape of attack.
fn allowed_host_or_origin(value: &str) -> bool {
    let v = value.trim().trim_end_matches('/');
    v == format!("127.0.0.1:{PORT}")
        || v == format!("localhost:{PORT}")
        || v == format!("http://127.0.0.1:{PORT}")
        || v == format!("http://localhost:{PORT}")
}

fn find_header<'a>(request: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

// Rejects requests whose Host or Origin header (when present) names
// something other than this server's own address — see the module doc
// comment on allowed_host_or_origin.
fn request_origin_is_trusted(request: &tiny_http::Request) -> bool {
    if let Some(host) = find_header(request, "Host") && !allowed_host_or_origin(host) {
        return false;
    }
    if let Some(origin) = find_header(request, "Origin") && !allowed_host_or_origin(origin) {
        return false;
    }
    true
}

// Reads at most MAX_BODY_BYTES+1 bytes regardless of what Content-Length
// claims, so a lying or absent Content-Length can never cause unbounded
// allocation — the cap is enforced on the actual read, not the declared
// size. Returns Err if the body is (or claims to be) too large.
fn read_body_capped(request: &mut tiny_http::Request) -> Result<String, String> {
    let mut buf = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("failed to read request body: {e}"))?;
    if buf.len() as u64 > MAX_BODY_BYTES {
        return Err(format!("request body exceeds {MAX_BODY_BYTES} byte limit"));
    }
    String::from_utf8(buf).map_err(|e| format!("request body is not valid UTF-8: {e}"))
}

// Reads the request body via read_body_capped, or sends the 400 error
// response itself (consuming `request`, since tiny_http::Request::respond
// takes it by value) and returns Err(()). Shared by /api/config and
// /api/language's POST handlers, which otherwise duplicated this exact
// reject-and-respond block for a body that's too large or not valid UTF-8.
// Takes `request` by value and hands it back alongside the body on success
// so the caller can keep using it to send its own eventual response.
fn read_body_or_reject(mut request: tiny_http::Request) -> Result<(String, tiny_http::Request), ()> {
    match read_body_capped(&mut request) {
        Ok(b) => Ok((b, request)),
        Err(e) => {
            respond_ignore_error(
                request.respond(json_response(format!("{{\"ok\":false,\"error\":{e:?}}}"), 400)),
            );
            Err(())
        }
    }
}

// Shared invalid-JSON response for both POST handlers below (identical
// `{"ok":false,"error":"invalid JSON: ..."}` / 400 shape in each).
fn respond_invalid_json(request: tiny_http::Request, e: impl std::fmt::Display) -> std::io::Result<()> {
    request.respond(json_response(
        format!("{{\"ok\":false,\"error\":{:?}}}", format!("invalid JSON: {e}")),
        400,
    ))
}

// Responds after a `ConfigPayload::validate_and_save()` call: `{"ok":true}`
// (plus an optional one-line log, since /api/config logs a save
// confirmation but /api/language doesn't) on success, or the same
// `{"ok":false,"error":...}` / 400 shape (plus an `eprintln!` tagged with
// `error_log_label`) on failure. Shared by both POST handlers below, which
// otherwise duplicated this exact shape apart from their log wording.
fn respond_after_save(
    request: tiny_http::Request,
    result: Result<(), String>,
    success_log: Option<&str>,
    error_log_label: &str,
) -> std::io::Result<()> {
    match result {
        Ok(()) => {
            if let Some(msg) = success_log {
                println!("{msg}");
            }
            request.respond(json_response("{\"ok\":true}".to_string(), 200))
        }
        Err(msg) => {
            eprintln!("configui: {error_log_label}: {msg}");
            request.respond(json_response(format!("{{\"ok\":false,\"error\":{msg:?}}}"), 400))
        }
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header name/value is always valid ASCII")
}

fn json_response(body: String, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
}

// ===========================================================================
// Live actuation calibration (GET/POST /api/calibration/*)
// ===========================================================================
//
// A live view of raw analog depth per key, to help pick sensible t_on/t_off
// values instead of guessing blindly. Runs as a dedicated thread, separate
// from — and possibly concurrent with — the main `tartarus_driver` process's
// own analog-read loop: Windows HID input reports are broadcast to every
// open reader, so a second handle opened here should see the same data
// without disturbing the first (unlike SendInput/writes, which would
// conflict, this is read-only). Only starts reading when the browser asks
// for it (POST .../start), not on every configui launch, since it's an
// active HID poll loop that has no reason to run otherwise.

static CALIBRATION_ACTIVE: AtomicBool = AtomicBool::new(false);
static CALIBRATION_DEPTHS: Mutex<[u8; NUM_KEYS]> = Mutex::new([0u8; NUM_KEYS]);

// Safety net in case the browser tab is closed without clicking "stop":
// the thread gives up on its own after this long rather than polling HID
// forever in the background.
const CALIBRATION_MAX_DURATION: Duration = Duration::from_secs(10 * 60);

#[derive(Serialize)]
struct CalibrationLive {
    active: bool,
    depths: Vec<u8>,
}

// Body of POST /api/language — a lightweight sibling to /api/config so
// switching the page's display language doesn't require resubmitting (or
// re-validating) the entire keymap/lighting/etc. form, and doesn't clobber
// any of the user's still-unsaved edits in the browser: it reads the
// currently-saved config.toml fresh, overwrites just the language field, and
// writes it back through the same validated save path as the main form.
#[derive(Deserialize)]
struct LanguagePayload {
    language: String,
}

// Uses main.rs's shared analog_device_infos() filter, but never calls
// std::process::exit on failure — that's fine for the normal driver's
// fail-fast CLI startup, but this runs inside the long-lived configui web
// server, where "Tartarus not plugged in" must fail soft (log a warning,
// leave the server itself running) rather than take the whole page down.
fn try_open_analog_devices(api: &hidapi::HidApi) -> Vec<(i32, hidapi::HidDevice)> {
    crate::analog_device_infos(api)
        .iter()
        .filter_map(|info| {
            let device = info.open_device(api).ok()?;
            let _ = device.set_blocking_mode(false);
            Some((info.interface_number(), device))
        })
        .collect()
}

fn run_calibration_thread() {
    let start = Instant::now();

    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[calibration] WARNING: hidapi init failed: {e}");
            CALIBRATION_ACTIVE.store(false, Ordering::SeqCst);
            return;
        }
    };

    // Best-effort device-mode-3 unlock, in case the main driver isn't
    // already running and nothing else has sent it this session. Harmless
    // to resend if it has (idempotent, confirmed on real hardware).
    if let Some(ctrl) = crate::open_razer_control_device(&api) {
        let cmd = crate::build_razer_cmd(0x01, 0x00, 0x04, &[0x03, 0x00]);
        let _ = ctrl.send_feature_report(&cmd);
    }

    let devices = try_open_analog_devices(&api);
    if devices.is_empty() {
        eprintln!("[calibration] WARNING: Tartarus Pro not found, or no interfaces could be opened.");
        CALIBRATION_ACTIVE.store(false, Ordering::SeqCst);
        return;
    }

    println!("[calibration] Started — press keys on the Tartarus Pro to see live depth values.");
    let mut buf = [0u8; 64];
    while CALIBRATION_ACTIVE.load(Ordering::SeqCst) && start.elapsed() < CALIBRATION_MAX_DURATION {
        for (_interface, device) in &devices {
            if let Ok(len) = device.read(&mut buf)
                && len > NUM_KEYS
                && buf[0] == crate::ANALOG_REPORT_ID
                && let Ok(mut depths) = CALIBRATION_DEPTHS.lock()
            {
                depths.copy_from_slice(&buf[1..1 + NUM_KEYS]);
            }
        }
        std::thread::sleep(Duration::from_micros(500));
    }
    CALIBRATION_ACTIVE.store(false, Ordering::SeqCst);
    println!("[calibration] Stopped.");
}

pub fn run_configui_server() {
    let server = match Server::http(("127.0.0.1", PORT)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "configui: 127.0.0.1:{PORT} でローカルサーバーを起動できませんでした: {e} \
                 (他のプロセスがこのポートを使用中の可能性があります)"
            );
            return;
        }
    };
    println!("configui: ブラウザで http://127.0.0.1:{PORT}/ を開いてください (終了はCtrl+C)");

    for request in server.incoming_requests() {
        handle_request(request);
    }
}

fn handle_request(request: tiny_http::Request) {
    if !request_origin_is_trusted(&request) {
        eprintln!(
            "configui: 信頼できないHost/Originからのリクエストを拒否しました \
             (Host={:?}, Origin={:?})",
            find_header(&request, "Host"),
            find_header(&request, "Origin")
        );
        respond_ignore_error(
            request.respond(Response::from_string("Forbidden: untrusted Host/Origin").with_status_code(403)),
        );
        return;
    }

    let method = request.method().clone();
    let url = request.url().to_string();

    let result = match (method, url.as_str()) {
        (Method::Get, "/") => request.respond(
            Response::from_string(ASSET_HTML)
                .with_header(header("Content-Type", "text/html; charset=utf-8")),
        ),
        (Method::Get, "/api/key-options") => {
            let json = serde_json::to_string(&all_key_names()).unwrap_or_else(|_| "[]".to_string());
            request.respond(json_response(json, 200))
        }
        (Method::Post, "/api/calibration/start") => {
            if CALIBRATION_ACTIVE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                std::thread::spawn(run_calibration_thread);
            }
            request.respond(json_response("{\"ok\":true}".to_string(), 200))
        }
        (Method::Post, "/api/calibration/stop") => {
            CALIBRATION_ACTIVE.store(false, Ordering::SeqCst);
            request.respond(json_response("{\"ok\":true}".to_string(), 200))
        }
        (Method::Get, "/api/calibration/live") => {
            let depths = CALIBRATION_DEPTHS.lock().map(|d| d.to_vec()).unwrap_or_else(|_| vec![0; NUM_KEYS]);
            let live = CalibrationLive { active: CALIBRATION_ACTIVE.load(Ordering::SeqCst), depths };
            let json = serde_json::to_string(&live).unwrap_or_else(|_| "{\"active\":false,\"depths\":[]}".to_string());
            request.respond(json_response(json, 200))
        }
        (Method::Get, "/api/config") => {
            let cfg: DriverConfig = crate::config::load();
            let payload = ConfigPayload::from_driver_config(&cfg);
            match serde_json::to_string(&payload) {
                Ok(json) => request.respond(json_response(json, 200)),
                Err(e) => request.respond(json_response(format!("{{\"error\":{:?}}}", e.to_string()), 500)),
            }
        }
        (Method::Post, "/api/config") => {
            let Ok((body, request)) = read_body_or_reject(request) else { return };
            match serde_json::from_str::<ConfigPayload>(&body) {
                Ok(payload) => respond_after_save(
                    request,
                    payload.validate_and_save(),
                    Some(
                        "configui: config.toml を更新しました。tartarus_driver を再起動すると新しい割り当てが反映されます。",
                    ),
                    "保存エラー",
                ),
                Err(e) => respond_invalid_json(request, e),
            }
        }
        (Method::Post, "/api/language") => {
            let Ok((body, request)) = read_body_or_reject(request) else { return };
            match serde_json::from_str::<LanguagePayload>(&body) {
                Ok(lang) => {
                    let cfg: DriverConfig = crate::config::load();
                    let mut payload = ConfigPayload::from_driver_config(&cfg);
                    payload.language = lang.language;
                    respond_after_save(request, payload.validate_and_save(), None, "言語設定の保存エラー")
                }
                Err(e) => respond_invalid_json(request, e),
            }
        }
        _ => request.respond(Response::from_string("Not Found").with_status_code(404)),
    };
    respond_ignore_error(result);
}

// The HTTP client disconnecting mid-response is not something this local
// single-user tool needs to handle specially; log and move on to the next
// request rather than crashing the server loop.
fn respond_ignore_error(result: std::io::Result<()>) {
    if let Err(e) = result {
        eprintln!("configui: レスポンス送信に失敗しました(クライアント切断など): {e}");
    }
}
