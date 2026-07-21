// Phase 4 (Purpose.md roadmap): loads key remap assignments from
// config.toml, falling back to the built-in placeholder keymaps (unchanged
// from pre-Phase-4 behavior) whenever the file is absent, unparsable, or an
// individual entry names an unrecognized key. This mirrors the project's
// established fail-open philosophy for the D-pad/Interception subsystem: a
// mistake in config.toml should never crash the driver or take down keys
// that WERE configured correctly, only fall back to a safe default for the
// specific entry that's wrong.
//
// Two separate representations exist on purpose, for two different
// consumers with different failure semantics:
//   - `load()` / RawConfig: used unattended at driver startup. Every field is
//     optional and every bad entry just falls back with a warning; nothing
//     here can abort startup.
//   - `ConfigPayload`: used by the `configui` web page's save button, a
//     request a human is actively watching in the browser. Every field is
//     required and the first invalid name aborts the whole save with a
//     specific error message, so the user gets immediate, precise feedback
//     instead of a silently-partial write.

use crate::lighting::{self, Color, Effect, LayerIndicatorConfig, LightingConfig, ProfileLedColor, WaveDirection};
use crate::vkname::{vk_from_name, vk_to_name};
use crate::{eprintln, println, NUM_KEYS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

// Same compile-time-resolved-path trick as main.rs's LOG_PATH: always lands
// at the repo root regardless of the process's current directory.
pub const CONFIG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../config.toml");

// Built-in defaults, used whenever config.toml doesn't specify a given
// entry. These are exactly the placeholder TEST_KEYMAP / LAYER1_TEST_KEYMAP /
// D-pad-wheel-middle-click test keys the driver shipped with before Phase 4,
// so a machine with no config.toml behaves identically to before.
const DEFAULT_ANALOG: [VIRTUAL_KEY; NUM_KEYS] = crate::TEST_KEYMAP;
const DEFAULT_LAYER1: [VIRTUAL_KEY; NUM_KEYS] = crate::LAYER1_TEST_KEYMAP;

pub struct AnalogKeymap {
    pub default: [VIRTUAL_KEY; NUM_KEYS],
    pub layer1: [VIRTUAL_KEY; NUM_KEYS],
}

pub struct DpadKeymap {
    pub left: VIRTUAL_KEY,
    pub up: VIRTUAL_KEY,
    pub right: VIRTUAL_KEY,
    pub down: VIRTUAL_KEY,
    pub wheel_up: VIRTUAL_KEY,
    pub wheel_down: VIRTUAL_KEY,
    pub middle_click: VIRTUAL_KEY,
}

// Hysteresis thresholds (Purpose.md §6.1) on the raw 0-255 analog depth
// scale. t_off MUST be strictly less than t_on for the hysteresis to behave
// sensibly (a key that's "down" at depth > t_on and "up" at depth < t_off);
// load()/ConfigPayload::validate() both enforce this and reject/fallback
// otherwise rather than accepting a combination that can't work.
pub struct Actuation {
    pub t_on: u8,
    pub t_off: u8,
    // Some((t_on, t_off)) overrides the global pair above for that one key;
    // None means "use the global pair". Index i corresponds to key(i+1).
    pub per_key: [Option<(u8, u8)>; NUM_KEYS],
}

impl Actuation {
    /// The (t_on, t_off) pair the analog loop should actually use for key
    /// index i (0-based) — its per-key override if it has one, else the
    /// global pair.
    pub fn for_key(&self, i: usize) -> (u8, u8) {
        self.per_key[i].unwrap_or((self.t_on, self.t_off))
    }
}

pub struct DriverConfig {
    pub analog: AnalogKeymap,
    pub dpad: DpadKeymap,
    pub actuation: Actuation,
    // None (the default, when config.toml has no [lighting] section) means
    // "don't touch the device's LEDs at all" — unlike the keymap/actuation
    // fields, lighting has no forced built-in default, since there's no
    // reason to overwrite whatever color/effect the device (or Synapse,
    // historically) last set unless the user opts in.
    pub lighting: Option<LightingConfig>,
    // None (the default) means "don't touch the profile indicator LEDs" —
    // same opt-in philosophy as `lighting` above. Some(cfg) means the main
    // analog loop turns cfg.color's indicator LED on while Hypershift/Layer1
    // is active and off otherwise (see run_driver in main.rs).
    pub layer_indicator: Option<LayerIndicatorConfig>,
}

impl DriverConfig {
    pub fn defaults() -> Self {
        DriverConfig {
            analog: AnalogKeymap {
                default: DEFAULT_ANALOG,
                layer1: DEFAULT_LAYER1,
            },
            dpad: DpadKeymap {
                left: crate::dpad::DPAD_ARROW_TEST_KEYMAP_LEFT,
                up: crate::dpad::DPAD_ARROW_TEST_KEYMAP_UP,
                right: crate::dpad::DPAD_ARROW_TEST_KEYMAP_RIGHT,
                down: crate::dpad::DPAD_ARROW_TEST_KEYMAP_DOWN,
                wheel_up: crate::dpad::WHEEL_UP_TEST_KEY,
                wheel_down: crate::dpad::WHEEL_DOWN_TEST_KEY,
                middle_click: crate::dpad::MIDDLE_CLICK_TEST_KEY,
            },
            actuation: Actuation {
                t_on: crate::T_ON,
                t_off: crate::T_OFF,
                per_key: [None; NUM_KEYS],
            },
            lighting: None,
            layer_indicator: None,
        }
    }
}

#[derive(Deserialize, Default)]
struct RawKeys {
    default: Option<HashMap<String, String>>,
    layer1: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct RawDpad {
    left: Option<String>,
    up: Option<String>,
    right: Option<String>,
    down: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawWheel {
    up: Option<String>,
    down: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawMiddleClick {
    key: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawPerKeyActuation {
    t_on: Option<u8>,
    t_off: Option<u8>,
}

#[derive(Deserialize, Default)]
struct RawActuation {
    t_on: Option<u8>,
    t_off: Option<u8>,
    per_key: Option<HashMap<String, RawPerKeyActuation>>,
}

#[derive(Deserialize, Default)]
struct RawLighting {
    effect: Option<String>,
    color: Option<String>,
    brightness: Option<u8>,
    wave_direction: Option<String>,
    reactive_speed: Option<u8>,
}

#[derive(Deserialize, Default)]
struct RawLayerIndicator {
    enabled: Option<bool>,
    color: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    keys: Option<RawKeys>,
    dpad: Option<RawDpad>,
    wheel: Option<RawWheel>,
    middle_click: Option<RawMiddleClick>,
    actuation: Option<RawActuation>,
    lighting: Option<RawLighting>,
    layer_indicator: Option<RawLayerIndicator>,
}

fn parse_color(s: &str) -> Result<Color, String> {
    lighting::parse_hex_color(s).ok_or_else(|| format!("\"{s}\" は RRGGBB形式の16進数カラーコードではありません"))
}

fn parse_direction(s: &str) -> Result<WaveDirection, String> {
    match s {
        "left" => Ok(WaveDirection::Left),
        "right" => Ok(WaveDirection::Right),
        other => Err(format!("wave_direction は \"left\" または \"right\" である必要があります(値: \"{other}\")")),
    }
}

/// Builds an Effect from its string/numeric parts. Shared by load()'s
/// RawLighting handling and ConfigPayload's validate()/save path so the two
/// can never disagree about what's valid. "none" is handled by callers
/// before this (it means "no Effect at all", not a variant of one).
fn build_effect(effect_name: &str, color: &str, wave_direction: &str, reactive_speed: u8) -> Result<Effect, String> {
    match effect_name {
        "off" => Ok(Effect::Off),
        "static" => Ok(Effect::Static(parse_color(color)?)),
        "breathing" => Ok(Effect::Breathing(parse_color(color)?)),
        "spectrum" => Ok(Effect::Spectrum),
        "wave" => Ok(Effect::Wave(parse_direction(wave_direction)?)),
        "reactive" => {
            if !(1..=4).contains(&reactive_speed) {
                return Err(format!("reactive_speed は1〜4である必要があります(値: {reactive_speed})"));
            }
            Ok(Effect::Reactive { speed: reactive_speed, color: parse_color(color)? })
        }
        other => Err(format!("\"{other}\" は認識できないライティングエフェクト名です")),
    }
}

fn apply_analog_map(target: &mut [VIRTUAL_KEY; NUM_KEYS], provided: &HashMap<String, String>, section: &str) {
    for (i, vk_slot) in target.iter_mut().enumerate() {
        let key_name = format!("key{:02}", i + 1);
        let Some(name) = provided.get(&key_name) else {
            continue;
        };
        match vk_from_name(name) {
            Some(vk) => *vk_slot = vk,
            None => eprintln!(
                "WARNING: config.toml [{section}] {key_name} = \"{name}\" is not a recognized \
                 key name; keeping the built-in default for this key."
            ),
        }
    }
}

fn apply_single(target: &mut VIRTUAL_KEY, provided: &Option<String>, field: &str) {
    let Some(name) = provided else { return };
    match vk_from_name(name) {
        Some(vk) => *target = vk,
        None => eprintln!(
            "WARNING: config.toml {field} = \"{name}\" is not a recognized key name; keeping \
             the built-in default."
        ),
    }
}

// Applies [actuation], if present, as a pair (not per-field) because t_off
// must stay strictly less than t_on for the hysteresis to make sense — a
// half-applied update (only one of the two overridden) could silently
// produce a broken combination that neither the file nor the built-in
// defaults intended.
fn apply_actuation(target: &mut Actuation, provided: &Option<RawActuation>) {
    let Some(raw) = provided else { return };
    let t_on = raw.t_on.unwrap_or(target.t_on);
    let t_off = raw.t_off.unwrap_or(target.t_off);
    if t_off >= t_on {
        eprintln!(
            "WARNING: config.toml [actuation] t_off ({t_off}) must be less than t_on ({t_on}); \
             keeping the built-in defaults for both (t_on={}, t_off={}).",
            crate::T_ON,
            crate::T_OFF
        );
    } else {
        target.t_on = t_on;
        target.t_off = t_off;
    }
    apply_per_key_actuation(target, &raw.per_key);
}

// Applies [actuation.per_key.keyNN] overrides, if present. Same pair-not-
// per-field reasoning as apply_actuation: a key with only one of t_on/t_off
// overridden falls back to the (already-resolved) global pair entirely
// rather than mixing one overridden value with the global default for the
// other, which could easily produce a t_off >= t_on combination by accident.
fn apply_per_key_actuation(target: &mut Actuation, provided: &Option<HashMap<String, RawPerKeyActuation>>) {
    let Some(map) = provided else { return };
    for i in 0..NUM_KEYS {
        let key_name = format!("key{:02}", i + 1);
        let Some(raw) = map.get(&key_name) else {
            continue;
        };
        let t_on = raw.t_on.unwrap_or(target.t_on);
        let t_off = raw.t_off.unwrap_or(target.t_off);
        if t_off >= t_on {
            eprintln!(
                "WARNING: config.toml [actuation.per_key] {key_name}: t_off ({t_off}) must be \
                 less than t_on ({t_on}); this key will use the global actuation values instead."
            );
            continue;
        }
        target.per_key[i] = Some((t_on, t_off));
    }
}

// Applies [lighting], if present. Unlike the keymap sections, there is no
// per-field fallback here: an effect + its parameters are a cohesive unit
// (e.g. "reactive" without a valid color doesn't degrade gracefully into
// some other effect), so any problem leaves cfg.lighting at its default
// (None — "don't touch the LEDs") rather than applying a partial/guessed
// effect. effect = "none" (or the section/field being entirely absent) both
// mean the same thing: no lighting management this run.
fn apply_lighting(target: &mut Option<LightingConfig>, provided: &Option<RawLighting>) {
    let Some(raw) = provided else { return };
    let Some(name) = &raw.effect else { return };
    if name == "none" {
        return;
    }
    let color = raw.color.as_deref().unwrap_or("FFFFFF");
    let wave_direction = raw.wave_direction.as_deref().unwrap_or("left");
    let reactive_speed = raw.reactive_speed.unwrap_or(2);
    match build_effect(name, color, wave_direction, reactive_speed) {
        Ok(effect) => *target = Some(LightingConfig { effect, brightness: raw.brightness }),
        Err(msg) => eprintln!(
            "WARNING: config.toml [lighting] could not be applied ({msg}); the device's LEDs \
             will not be changed this run."
        ),
    }
}

// Applies [layer_indicator], if present. Same all-or-nothing reasoning as
// apply_lighting: `enabled = true` with no (or an invalid) color can't
// degrade to "half enabled", so any problem leaves cfg.layer_indicator at
// None ("don't touch the profile LEDs").
fn apply_layer_indicator(target: &mut Option<LayerIndicatorConfig>, provided: &Option<RawLayerIndicator>) {
    let Some(raw) = provided else { return };
    if raw.enabled != Some(true) {
        return;
    }
    let color_name = raw.color.as_deref().unwrap_or("green");
    match ProfileLedColor::from_name(color_name) {
        Some(color) => *target = Some(LayerIndicatorConfig { color }),
        None => eprintln!(
            "WARNING: config.toml [layer_indicator] color = \"{color_name}\" is not recognized \
             (expected \"red\", \"green\", or \"blue\"); the layer indicator LED will not be used \
             this run."
        ),
    }
}

/// Loads config.toml if present, falling back to built-in placeholder
/// defaults for anything absent, unparsable, or individually invalid. Prints
/// one status line either way (via the crate's file-logging println!) so
/// `tasks/run.log` always shows which source was used.
pub fn load() -> DriverConfig {
    let mut cfg = DriverConfig::defaults();

    let text = match std::fs::read_to_string(CONFIG_PATH) {
        Ok(t) => t,
        Err(_) => {
            println!(
                "No config.toml found at {CONFIG_PATH} — using built-in placeholder keymap. \
                 Run `cargo run --release -- configui` to create one."
            );
            return cfg;
        }
    };

    let raw: RawConfig = match toml::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "WARNING: config.toml could not be parsed ({e}) — using built-in placeholder \
                 keymap entirely until this is fixed."
            );
            return cfg;
        }
    };

    if let Some(keys) = &raw.keys {
        if let Some(m) = &keys.default {
            apply_analog_map(&mut cfg.analog.default, m, "keys.default");
        }
        if let Some(m) = &keys.layer1 {
            apply_analog_map(&mut cfg.analog.layer1, m, "keys.layer1");
        }
    }
    if let Some(dpad) = &raw.dpad {
        apply_single(&mut cfg.dpad.left, &dpad.left, "dpad.left");
        apply_single(&mut cfg.dpad.up, &dpad.up, "dpad.up");
        apply_single(&mut cfg.dpad.right, &dpad.right, "dpad.right");
        apply_single(&mut cfg.dpad.down, &dpad.down, "dpad.down");
    }
    if let Some(wheel) = &raw.wheel {
        apply_single(&mut cfg.dpad.wheel_up, &wheel.up, "wheel.up");
        apply_single(&mut cfg.dpad.wheel_down, &wheel.down, "wheel.down");
    }
    if let Some(mc) = &raw.middle_click {
        apply_single(&mut cfg.dpad.middle_click, &mc.key, "middle_click.key");
    }
    apply_actuation(&mut cfg.actuation, &raw.actuation);
    apply_lighting(&mut cfg.lighting, &raw.lighting);
    apply_layer_indicator(&mut cfg.layer_indicator, &raw.layer_indicator);

    println!("Loaded config.toml from {CONFIG_PATH}.");
    cfg
}

// ===========================================================================
// JSON representation for the `configui` web page (GET/POST /api/config)
// ===========================================================================

#[derive(Serialize, Deserialize)]
pub struct ConfigPayload {
    pub keys_default: Vec<String>, // exactly NUM_KEYS entries, key01..key20 in order
    pub keys_layer1: Vec<String>,  // exactly NUM_KEYS entries, key01..key20 in order
    pub dpad_left: String,
    pub dpad_up: String,
    pub dpad_right: String,
    pub dpad_down: String,
    pub wheel_up: String,
    pub wheel_down: String,
    pub middle_click: String,
    pub t_on: u8,
    pub t_off: u8,
    // Per-key overrides, exactly NUM_KEYS entries each, key01..key20 in
    // order. Both None = "use the global t_on/t_off for this key"; both
    // Some = an override; exactly one Some is rejected by validate() as an
    // incomplete pair.
    pub per_key_t_on: Vec<Option<u8>>,
    pub per_key_t_off: Vec<Option<u8>>,
    pub lighting_effect: String, // "none" | "off" | "static" | "breathing" | "spectrum" | "wave" | "reactive"
    pub lighting_color: String,  // "RRGGBB" hex, relevant for static/breathing/reactive
    pub lighting_brightness: u8,
    pub lighting_wave_direction: String, // "left" | "right", relevant for wave
    pub lighting_reactive_speed: u8,     // 1-4, relevant for reactive
    pub layer_indicator_enabled: bool,
    pub layer_indicator_color: String, // "red" | "green" | "blue"
}

// Decomposes an Option<LightingConfig> into the flat string/number fields
// ConfigPayload needs. Fields irrelevant to the current effect (e.g. color
// when the effect is "spectrum") are filled with harmless placeholders
// purely so the web page has *something* to prefill those inputs with.
fn lighting_to_payload_fields(lighting: &Option<LightingConfig>) -> (String, String, u8, String, u8) {
    const PLACEHOLDER_COLOR: &str = "FFFFFF";
    const PLACEHOLDER_DIRECTION: &str = "left";
    const PLACEHOLDER_SPEED: u8 = 2;
    let brightness = lighting.as_ref().and_then(|l| l.brightness).unwrap_or(255);
    let Some(l) = lighting else {
        return ("none".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED);
    };
    match &l.effect {
        Effect::Off => ("off".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Static(c) => ("static".to_string(), lighting::color_to_hex(c), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Breathing(c) => ("breathing".to_string(), lighting::color_to_hex(c), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Spectrum => ("spectrum".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Wave(dir) => {
            let dir_str = match dir { WaveDirection::Left => "left", WaveDirection::Right => "right" };
            ("wave".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, dir_str.to_string(), PLACEHOLDER_SPEED)
        }
        Effect::Reactive { speed, color } => ("reactive".to_string(), lighting::color_to_hex(color), brightness, PLACEHOLDER_DIRECTION.to_string(), *speed),
    }
}

impl ConfigPayload {
    pub fn from_driver_config(cfg: &DriverConfig) -> Self {
        let (lighting_effect, lighting_color, lighting_brightness, lighting_wave_direction, lighting_reactive_speed) =
            lighting_to_payload_fields(&cfg.lighting);
        ConfigPayload {
            keys_default: cfg.analog.default.iter().map(|vk| vk_to_name(*vk)).collect(),
            keys_layer1: cfg.analog.layer1.iter().map(|vk| vk_to_name(*vk)).collect(),
            dpad_left: vk_to_name(cfg.dpad.left),
            dpad_up: vk_to_name(cfg.dpad.up),
            dpad_right: vk_to_name(cfg.dpad.right),
            dpad_down: vk_to_name(cfg.dpad.down),
            wheel_up: vk_to_name(cfg.dpad.wheel_up),
            wheel_down: vk_to_name(cfg.dpad.wheel_down),
            middle_click: vk_to_name(cfg.dpad.middle_click),
            t_on: cfg.actuation.t_on,
            t_off: cfg.actuation.t_off,
            per_key_t_on: cfg.actuation.per_key.iter().map(|p| p.map(|(on, _)| on)).collect(),
            per_key_t_off: cfg.actuation.per_key.iter().map(|p| p.map(|(_, off)| off)).collect(),
            lighting_effect,
            lighting_color,
            lighting_brightness,
            lighting_wave_direction,
            lighting_reactive_speed,
            layer_indicator_enabled: cfg.layer_indicator.is_some(),
            layer_indicator_color: cfg
                .layer_indicator
                .as_ref()
                .map(|l| l.color.name().to_string())
                .unwrap_or_else(|| "green".to_string()),
        }
    }

    /// Validates every name, failing loud on the FIRST bad one (unlike
    /// load()'s fail-open-per-field: this backs an interactive save the user
    /// is watching in the browser, so surfacing the mistake immediately
    /// beats silently keeping an old value). Pure — no disk I/O — so it's
    /// safe to call from tests.
    pub fn validate(&self) -> Result<(), String> {
        if self.keys_default.len() != NUM_KEYS {
            return Err(format!("keys_default には{NUM_KEYS}件必要です"));
        }
        if self.keys_layer1.len() != NUM_KEYS {
            return Err(format!("keys_layer1 には{NUM_KEYS}件必要です"));
        }

        let check = |label: String, name: &str| -> Result<(), String> {
            if vk_from_name(name).is_some() {
                Ok(())
            } else {
                Err(format!("{label}: \"{name}\" は認識できないキー名です"))
            }
        };
        for (i, name) in self.keys_default.iter().enumerate() {
            check(format!("keys.default.key{:02}", i + 1), name)?;
        }
        for (i, name) in self.keys_layer1.iter().enumerate() {
            check(format!("keys.layer1.key{:02}", i + 1), name)?;
        }
        check("dpad.left".into(), &self.dpad_left)?;
        check("dpad.up".into(), &self.dpad_up)?;
        check("dpad.right".into(), &self.dpad_right)?;
        check("dpad.down".into(), &self.dpad_down)?;
        check("wheel.up".into(), &self.wheel_up)?;
        check("wheel.down".into(), &self.wheel_down)?;
        check("middle_click.key".into(), &self.middle_click)?;
        if self.t_off >= self.t_on {
            return Err(format!(
                "actuation.t_off ({}) は actuation.t_on ({}) より小さくする必要があります",
                self.t_off, self.t_on
            ));
        }
        if self.per_key_t_on.len() != NUM_KEYS || self.per_key_t_off.len() != NUM_KEYS {
            return Err(format!("per_key_t_on/per_key_t_off には{NUM_KEYS}件必要です"));
        }
        for i in 0..NUM_KEYS {
            let label = format!("actuation.per_key.key{:02}", i + 1);
            match (self.per_key_t_on[i], self.per_key_t_off[i]) {
                (None, None) => {}
                (Some(on), Some(off)) if off < on => {}
                (Some(on), Some(off)) => {
                    return Err(format!("{label}: t_off ({off}) は t_on ({on}) より小さくする必要があります"));
                }
                _ => return Err(format!("{label}: t_on と t_off は両方指定するか、両方とも空にしてください")),
            }
        }
        if self.lighting_effect != "none" {
            build_effect(
                &self.lighting_effect,
                &self.lighting_color,
                &self.lighting_wave_direction,
                self.lighting_reactive_speed,
            )
            .map_err(|e| format!("lighting: {e}"))?;
        }
        if self.layer_indicator_enabled && ProfileLedColor::from_name(&self.layer_indicator_color).is_none() {
            return Err(format!(
                "layer_indicator.color: \"{}\" は認識できない色です(red/green/blueのいずれか)",
                self.layer_indicator_color
            ));
        }
        Ok(())
    }

    /// validate()s, then overwrites config.toml wholesale on success.
    pub fn validate_and_save(&self) -> Result<(), String> {
        self.validate()?;
        std::fs::write(CONFIG_PATH, self.to_toml_string())
            .map_err(|e| format!("config.tomlへの書き込みに失敗しました: {e}"))
    }

    fn to_toml_string(&self) -> String {
        let mut s = String::from("# tartarus_driver key remap config — generated by `configui`\n\n[keys.default]\n");
        for (i, name) in self.keys_default.iter().enumerate() {
            s.push_str(&format!("key{:02} = \"{name}\"\n", i + 1));
        }
        s.push_str("\n[keys.layer1]\n");
        for (i, name) in self.keys_layer1.iter().enumerate() {
            s.push_str(&format!("key{:02} = \"{name}\"\n", i + 1));
        }
        s.push_str(&format!(
            "\n[dpad]\nleft = \"{}\"\nup = \"{}\"\nright = \"{}\"\ndown = \"{}\"\n",
            self.dpad_left, self.dpad_up, self.dpad_right, self.dpad_down
        ));
        s.push_str(&format!(
            "\n[wheel]\nup = \"{}\"\ndown = \"{}\"\n",
            self.wheel_up, self.wheel_down
        ));
        s.push_str(&format!("\n[middle_click]\nkey = \"{}\"\n", self.middle_click));
        s.push_str(&format!("\n[actuation]\nt_on = {}\nt_off = {}\n", self.t_on, self.t_off));
        let overrides: Vec<(usize, u8, u8)> = (0..NUM_KEYS)
            .filter_map(|i| match (self.per_key_t_on[i], self.per_key_t_off[i]) {
                (Some(on), Some(off)) => Some((i, on, off)),
                _ => None,
            })
            .collect();
        if !overrides.is_empty() {
            s.push_str("\n[actuation.per_key]\n");
            for (i, on, off) in overrides {
                s.push_str(&format!("key{:02} = {{ t_on = {on}, t_off = {off} }}\n", i + 1));
            }
        }
        s.push_str(&format!(
            "\n[lighting]\neffect = \"{}\"\ncolor = \"{}\"\nbrightness = {}\nwave_direction = \"{}\"\nreactive_speed = {}\n",
            self.lighting_effect, self.lighting_color, self.lighting_brightness, self.lighting_wave_direction, self.lighting_reactive_speed
        ));
        s.push_str(&format!(
            "\n[layer_indicator]\nenabled = {}\ncolor = \"{}\"\n",
            self.layer_indicator_enabled, self.layer_indicator_color
        ));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_pre_phase4_placeholder_keymaps() {
        let cfg = DriverConfig::defaults();
        assert_eq!(cfg.analog.default, crate::TEST_KEYMAP);
        assert_eq!(cfg.analog.layer1, crate::LAYER1_TEST_KEYMAP);
        assert_eq!(cfg.dpad.left, crate::dpad::DPAD_ARROW_TEST_KEYMAP_LEFT);
    }

    #[test]
    fn payload_round_trips_through_defaults() {
        let cfg = DriverConfig::defaults();
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.keys_default.len(), NUM_KEYS);
        assert_eq!(payload.keys_default[0], "1"); // TEST_KEYMAP key01 -> '1'
        assert_eq!(payload.dpad_left, "K");
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn invalid_name_is_rejected_with_a_specific_message() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.dpad_left = "NOT_A_KEY".to_string();
        let err = payload.validate().unwrap_err();
        assert!(err.contains("dpad.left"));
    }

    #[test]
    fn actuation_defaults_and_payload_round_trip() {
        let cfg = DriverConfig::defaults();
        assert_eq!(cfg.actuation.t_on, crate::T_ON);
        assert_eq!(cfg.actuation.t_off, crate::T_OFF);
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.t_on, crate::T_ON);
        assert_eq!(payload.t_off, crate::T_OFF);
    }

    #[test]
    fn t_off_must_be_less_than_t_on() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.t_on = 50;
        payload.t_off = 50;
        let err = payload.validate().unwrap_err();
        assert!(err.contains("actuation"));

        payload.t_off = 49;
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn apply_actuation_rejects_invalid_pair_and_keeps_defaults() {
        let mut actuation = Actuation { t_on: crate::T_ON, t_off: crate::T_OFF, per_key: [None; NUM_KEYS] };
        let bad = Some(RawActuation { t_on: Some(50), t_off: Some(50), per_key: None });
        apply_actuation(&mut actuation, &bad);
        assert_eq!(actuation.t_on, crate::T_ON);
        assert_eq!(actuation.t_off, crate::T_OFF);

        let good = Some(RawActuation { t_on: Some(120), t_off: Some(60), per_key: None });
        apply_actuation(&mut actuation, &good);
        assert_eq!(actuation.t_on, 120);
        assert_eq!(actuation.t_off, 60);
    }

    #[test]
    fn per_key_actuation_falls_back_to_global_when_unset_or_invalid() {
        let mut actuation = Actuation { t_on: 100, t_off: 80, per_key: [None; NUM_KEYS] };

        // No override -> global pair.
        assert_eq!(actuation.for_key(4), (100, 80));

        let mut overrides = HashMap::new();
        overrides.insert("key05".to_string(), RawPerKeyActuation { t_on: Some(60), t_off: Some(30) });
        overrides.insert("key12".to_string(), RawPerKeyActuation { t_on: Some(50), t_off: Some(50) }); // invalid pair
        apply_per_key_actuation(&mut actuation, &Some(overrides));

        assert_eq!(actuation.for_key(4), (60, 30)); // key05, index 4
        assert_eq!(actuation.for_key(11), (100, 80)); // key12 rejected -> falls back to global
        assert_eq!(actuation.for_key(0), (100, 80)); // untouched key -> global
    }

    #[test]
    fn payload_per_key_pair_must_be_complete_or_absent() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.per_key_t_on[4] = Some(60);
        // t_off left None: an incomplete pair.
        let err = payload.validate().unwrap_err();
        assert!(err.contains("key05"));

        payload.per_key_t_off[4] = Some(30);
        assert!(payload.validate().is_ok());

        payload.per_key_t_off[4] = Some(90); // now off > on
        assert!(payload.validate().is_err());
    }

    #[test]
    fn to_toml_string_only_writes_overridden_keys() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.per_key_t_on[4] = Some(60);
        payload.per_key_t_off[4] = Some(30);
        let toml_text = payload.to_toml_string();
        assert!(toml_text.contains("[actuation.per_key]"));
        assert!(toml_text.contains("key05 = { t_on = 60, t_off = 30 }"));
        assert!(!toml_text.contains("key01 = { t_on"));
    }

    #[test]
    fn lighting_defaults_to_none_and_round_trips_as_the_none_sentinel() {
        let cfg = DriverConfig::defaults();
        assert!(cfg.lighting.is_none());
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.lighting_effect, "none");
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn lighting_static_requires_a_valid_color() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.lighting_effect = "static".to_string();
        payload.lighting_color = "NOTHEX".to_string();
        let err = payload.validate().unwrap_err();
        assert!(err.contains("lighting"));

        payload.lighting_color = "FF6A00".to_string();
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn lighting_reactive_speed_must_be_in_range() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.lighting_effect = "reactive".to_string();
        payload.lighting_color = "00FF00".to_string();
        payload.lighting_reactive_speed = 9;
        assert!(payload.validate().is_err());
        payload.lighting_reactive_speed = 4;
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn apply_lighting_leaves_none_on_bad_effect_and_applies_a_good_one() {
        let mut lighting: Option<LightingConfig> = None;
        let bad = Some(RawLighting {
            effect: Some("not_a_real_effect".to_string()),
            ..Default::default()
        });
        apply_lighting(&mut lighting, &bad);
        assert!(lighting.is_none());

        let good = Some(RawLighting {
            effect: Some("static".to_string()),
            color: Some("112233".to_string()),
            brightness: Some(200),
            ..Default::default()
        });
        apply_lighting(&mut lighting, &good);
        let applied = lighting.expect("static effect with a valid color should apply");
        assert!(matches!(applied.effect, Effect::Static(_)));
        assert_eq!(applied.brightness, Some(200));
    }

    #[test]
    fn layer_indicator_defaults_to_disabled_and_round_trips() {
        let cfg = DriverConfig::defaults();
        assert!(cfg.layer_indicator.is_none());
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert!(!payload.layer_indicator_enabled);
        assert_eq!(payload.layer_indicator_color, "green");
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn apply_layer_indicator_rejects_unknown_color_and_applies_a_good_one() {
        let mut indicator: Option<LayerIndicatorConfig> = None;
        let bad = Some(RawLayerIndicator { enabled: Some(true), color: Some("purple".to_string()) });
        apply_layer_indicator(&mut indicator, &bad);
        assert!(indicator.is_none());

        let good = Some(RawLayerIndicator { enabled: Some(true), color: Some("blue".to_string()) });
        apply_layer_indicator(&mut indicator, &good);
        assert!(matches!(indicator.unwrap().color, ProfileLedColor::Blue));

        // enabled = false (or absent) must never turn the indicator on, even
        // with an otherwise-valid color.
        let mut indicator2: Option<LayerIndicatorConfig> = None;
        let disabled = Some(RawLayerIndicator { enabled: Some(false), color: Some("red".to_string()) });
        apply_layer_indicator(&mut indicator2, &disabled);
        assert!(indicator2.is_none());
    }

    #[test]
    fn payload_rejects_unknown_layer_indicator_color_only_when_enabled() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.layer_indicator_color = "purple".to_string();
        // Disabled: an invalid color is harmless (never sent), so this must
        // still validate cleanly.
        assert!(payload.validate().is_ok());

        payload.layer_indicator_enabled = true;
        assert!(payload.validate().is_err());

        payload.layer_indicator_color = "red".to_string();
        assert!(payload.validate().is_ok());
    }
}
