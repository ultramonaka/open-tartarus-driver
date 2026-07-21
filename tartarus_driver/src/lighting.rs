// LED lighting control (Razer "extended matrix" protocol, command_class
// 0x0f) over the same Interface 2 ("Razer Control Device") razer_report
// channel already used for the device-mode-3 analog-streaming unlock.
// Protocol details are from a research pass over OpenRazer's driver source
// (driver/razerchromacommon.c, razerkbd_driver.c, PR #2336/#2710) — see
// tasks/research/2026-07-20_tartarus_pro_rgb_lighting_protocol.md for the
// full write-up and citations. NOT yet confirmed against real hardware by
// us directly (unlike everything else in this driver) — this is the first
// use of command_class 0x0f in this codebase.
//
// Two things this file deliberately does NOT implement, both flagged as
// future work in the research doc:
//   - Per-key custom RGB frames (command_id 0x03): the Tartarus Pro exposes
//     its 20-ish key LEDs as a 1x21 linear matrix, but which physical key is
//     which column index is unconfirmed and needs empirical calibration
//     (light one column at a time and watch which key lights up) — the same
//     kind of hands-on verification the D-pad scancode mapping needed.
//   - Starlight and breathing-random effects: cheap to add later (same
//     command shape as what's here), just not exposed yet to keep the
//     first configui lighting panel small.
//
// Tartarus-Pro-specific protocol quirk (per the research): lighting
// commands use transaction_id 0x1f, NOT the 0x01 the existing device-mode
// unlock command uses — those are two different, independently-confirmed
// constants for two different command classes, not a typo.

use crate::{eprintln, println};

pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub enum WaveDirection {
    Left,
    Right,
}

pub enum Effect {
    Off,
    Static(Color),
    Breathing(Color),
    Spectrum,
    Wave(WaveDirection),
    Reactive { speed: u8, color: Color },
}

pub struct LightingConfig {
    pub effect: Effect,
    pub brightness: Option<u8>,
}

const LIGHTING_TXN: u8 = 0x1f;
const CLASS_MATRIX: u8 = 0x0f;
const CMD_SET_EFFECT: u8 = 0x02;
const CMD_SET_BRIGHTNESS: u8 = 0x04;
// VARSTORE (persistent) + BACKLIGHT_LED, per razercommon.h — every "set
// effect" / "set brightness" command starts with this same two-byte prefix.
const ARG_VARSTORE_BACKLIGHT: [u8; 2] = [0x01, 0x05];

fn effect_args(effect: &Effect) -> Vec<u8> {
    let mut args = ARG_VARSTORE_BACKLIGHT.to_vec();
    match effect {
        Effect::Off => args.extend([0x00, 0x00, 0x00, 0x00]),
        Effect::Static(c) => args.extend([0x01, 0x00, 0x00, 0x01, c.r, c.g, c.b]),
        Effect::Breathing(c) => args.extend([0x02, 0x01, 0x00, 0x01, c.r, c.g, c.b]),
        Effect::Spectrum => args.extend([0x03, 0x00, 0x00, 0x00]),
        Effect::Wave(dir) => {
            let direction_byte = match dir {
                WaveDirection::Left => 0x01,
                WaveDirection::Right => 0x02,
            };
            args.extend([0x04, direction_byte, 0x28, 0x00]);
        }
        Effect::Reactive { speed, color } => {
            args.extend([0x05, 0x00, *speed, 0x01, color.r, color.g, color.b]);
        }
    }
    args
}

fn effect_name(effect: &Effect) -> &'static str {
    match effect {
        Effect::Off => "off",
        Effect::Static(_) => "static",
        Effect::Breathing(_) => "breathing",
        Effect::Spectrum => "spectrum",
        Effect::Wave(_) => "wave",
        Effect::Reactive { .. } => "reactive",
    }
}

/// Sends the configured effect (and brightness, if set) to Interface 2.
/// Called once at driver startup, right alongside the device-mode-3 unlock
/// command, using the same already-open control-device handle. Failures are
/// logged but non-fatal — a lighting command failing has no bearing on the
/// analog keys, D-pad, or Hypershift, all of which keep working normally.
pub fn apply(ctrl: &hidapi::HidDevice, lighting: &LightingConfig) {
    let args = effect_args(&lighting.effect);
    let cmd = crate::build_razer_cmd(LIGHTING_TXN, CLASS_MATRIX, CMD_SET_EFFECT, &args);
    match ctrl.send_feature_report(&cmd) {
        Ok(()) => println!("Lighting: effect set to \"{}\".", effect_name(&lighting.effect)),
        Err(e) => eprintln!("WARNING: failed to send lighting effect command: {e}"),
    }

    if let Some(brightness) = lighting.brightness {
        let args = [ARG_VARSTORE_BACKLIGHT[0], ARG_VARSTORE_BACKLIGHT[1], brightness];
        let cmd = crate::build_razer_cmd(LIGHTING_TXN, CLASS_MATRIX, CMD_SET_BRIGHTNESS, &args);
        match ctrl.send_feature_report(&cmd) {
            Ok(()) => println!("Lighting: brightness set to {brightness}."),
            Err(e) => eprintln!("WARNING: failed to send lighting brightness command: {e}"),
        }
    }
}

// ===========================================================================
// Layer indicator: profile LEDs (command_class 0x03, "standard LED")
// ===========================================================================
//
// The Tartarus Pro has 3 separate fixed-color indicator LEDs on its side
// strip (red/green/blue), physically and protocol-wise independent of the
// main 1x21 key matrix — a completely different command_class (0x03, not
// the matrix's 0x0f), so toggling one never disrupts whatever main effect
// (static/spectrum/breathing/etc.) is currently running. Used here to give
// Hypershift a hardware status light: on while Layer1 is active, off for
// Default. Research: tasks/research/2026-07-21_tartarus_pro_profile_indicator_led_protocol.md.
//
// OpenRazer itself has never implemented these specifically for the
// Tartarus Pro (its own merged PR left them as "will be added later"), so
// unlike the matrix effects, this protocol is NOT independently confirmed
// for this exact product — only for sibling devices (Tartarus Chroma/V2,
// Orbweaver). Two things in particular need real-hardware confirmation:
//   - transaction_id: sibling devices use 0xFF for this command family
//     (distinct from the matrix effects' Tartarus-Pro-specific 0x1f) — try
//     0xFF first; if the LED never lights, 0x1f is the fallback to try.
//   - Whether the LED IDs/command shape below are unchanged on this device
//     at all (plausible but unverified).
const LAYER_INDICATOR_TXN: u8 = 0x1f;
const CLASS_STANDARD_LED: u8 = 0x03;
const CMD_SET_LED_STATE: u8 = 0x00;
// NOSTORE (not VARSTORE): this toggles on every Hypershift press/release, so
// deliberately volatile — writing to the device's persistent storage that
// often would be pointless wear with no benefit (the LED only needs to
// reflect live state, never survive a power cycle on its own).
const ARG_NOSTORE: u8 = 0x00;

pub enum ProfileLedColor {
    Red,
    Green,
    Blue,
}

impl ProfileLedColor {
    fn led_id(&self) -> u8 {
        match self {
            ProfileLedColor::Red => 0x0C,
            ProfileLedColor::Green => 0x0D,
            ProfileLedColor::Blue => 0x0E,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ProfileLedColor::Red => "red",
            ProfileLedColor::Green => "green",
            ProfileLedColor::Blue => "blue",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "red" => Some(ProfileLedColor::Red),
            "green" => Some(ProfileLedColor::Green),
            "blue" => Some(ProfileLedColor::Blue),
            _ => None,
        }
    }
}

pub struct LayerIndicatorConfig {
    pub color: ProfileLedColor,
}

/// Sends a set-LED-state command for one profile indicator LED. Called from
/// the main analog loop on every Hypershift active/inactive transition (see
/// run_driver in main.rs), using the same already-open Interface 2 handle as
/// the startup unlock/effect commands. Failures are logged but non-fatal —
/// exactly like the main lighting effect, this has no bearing on analog
/// keys, D-pad, or Hypershift itself continuing to work correctly.
pub fn set_layer_indicator(ctrl: &hidapi::HidDevice, color: &ProfileLedColor, on: bool) {
    let args = [ARG_NOSTORE, color.led_id(), on as u8];
    let cmd = crate::build_razer_cmd(LAYER_INDICATOR_TXN, CLASS_STANDARD_LED, CMD_SET_LED_STATE, &args);
    match ctrl.send_feature_report(&cmd) {
        Ok(()) => println!(
            "Layer indicator: {} LED {}.",
            color.name(),
            if on { "on (Layer1 active)" } else { "off (Default layer)" }
        ),
        Err(e) => eprintln!("WARNING: failed to send layer indicator LED command: {e}"),
    }
}

/// Parses a 6-hex-digit "RRGGBB" string (case-insensitive, no leading '#').
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color { r, g, b })
}

pub fn color_to_hex(c: &Color) -> String {
    format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_color_round_trips() {
        let c = parse_hex_color("Ff6A00").unwrap();
        assert_eq!((c.r, c.g, c.b), (0xff, 0x6a, 0x00));
        assert_eq!(color_to_hex(&c), "FF6A00");
    }

    #[test]
    fn hex_color_rejects_malformed_input() {
        assert!(parse_hex_color("FF6A0").is_none()); // too short
        assert!(parse_hex_color("FF6A00FF").is_none()); // too long
        assert!(parse_hex_color("GGGGGG").is_none()); // not hex
    }

    #[test]
    fn profile_led_color_name_round_trips() {
        for name in ["red", "green", "blue"] {
            let color = ProfileLedColor::from_name(name).unwrap();
            assert_eq!(color.name(), name);
        }
        assert!(ProfileLedColor::from_name("purple").is_none());
    }

    #[test]
    fn effect_args_match_researched_data_sizes() {
        // data_size in the wire format is just args.len() (build_razer_cmd
        // sets it from the slice length), so this doubles as a check that
        // each effect produces the byte count the research doc recorded.
        assert_eq!(effect_args(&Effect::Off).len(), 6);
        assert_eq!(effect_args(&Effect::Static(Color { r: 1, g: 2, b: 3 })).len(), 9);
        assert_eq!(effect_args(&Effect::Breathing(Color { r: 1, g: 2, b: 3 })).len(), 9);
        assert_eq!(effect_args(&Effect::Spectrum).len(), 6);
        assert_eq!(effect_args(&Effect::Wave(WaveDirection::Left)).len(), 6);
        assert_eq!(
            effect_args(&Effect::Reactive { speed: 2, color: Color { r: 1, g: 2, b: 3 } }).len(),
            9
        );
    }
}
