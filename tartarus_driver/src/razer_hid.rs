// HID device discovery and the Razer control-protocol command builder:
// finding/opening the Tartarus Pro's analog-data interface and its
// "Razer Control Device" control channel, and building the 91-byte
// razer_report feature-report payloads sent over the latter (device-mode
// unlock, lighting commands — see lighting.rs). Split out of main.rs so the
// entry point / driver loop isn't tangled up with HID/protocol plumbing.

use crate::eprintln;
use hidapi::HidApi;

const VID: u16 = 0x1532;
const PID: u16 = 0x0244;

// Interface 1 / endpoint 0x82 emits this report ID for the 20 analog keys.
// Reverse-engineered via USBPcap capture on 2026-07-18 (docs/reference/logs/capture.pcap):
// byte[0] = report ID, byte[1..=20] = one 0-255 depth value per physical key.
// Confirmed 2026-07-18 (docs/reference/logs/keymap_log.txt): byte offset N == the number
// printed on keycap N (identity mapping, no permutation).
pub const ANALOG_REPORT_ID: u8 = 0x06;

// Filters hidapi's device list down to the Tartarus Pro's collections that
// are actually readable: matching VID/PID, minus the two boot collections
// (Usage Page 0x01, Usage 0x02 "Mouse" / 0x06 "Keyboard") that Windows' HID
// class driver claims exclusively (ReadFile on them always fails with
// ACCESS_DENIED). Shared by open_analog_devices below (which exits the
// process if this comes back empty — fine for the driver's fail-fast CLI
// startup) and configui's try_open_analog_devices (which must fail soft
// instead, since it runs inside the long-lived config web server).
pub fn analog_device_infos(api: &HidApi) -> Vec<hidapi::DeviceInfo> {
    api.device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
        .filter(|d| !(d.usage_page() == 0x0001 && (d.usage() == 0x0002 || d.usage() == 0x0006)))
        .cloned()
        .collect()
}

pub(crate) fn open_analog_devices(api: &HidApi) -> Vec<(i32, hidapi::HidDevice)> {
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

pub fn open_razer_control_device(api: &HidApi) -> Option<hidapi::HidDevice> {
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
pub fn build_razer_cmd(txn: u8, class: u8, cmd: u8, args: &[u8]) -> [u8; 91] {
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
