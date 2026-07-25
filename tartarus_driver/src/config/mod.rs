// Phase 4 (docs/DESIGN.md roadmap): loads key remap assignments from
// config.toml, falling back to the built-in placeholder keymaps (unchanged
// from pre-Phase-4 behavior) whenever the file is absent, unparsable, or an
// individual entry names an unrecognized key. This mirrors the project's
// established fail-open philosophy for the D-pad/Interception subsystem: a
// mistake in config.toml should never crash the driver or take down keys
// that WERE configured correctly, only fall back to a safe default for the
// specific entry that's wrong.
//
// Two separate representations exist on purpose, for two different
// consumers with different failure semantics — split into their own
// submodules since each is a substantial, mostly self-contained chunk:
//   - `load` (RawConfig etc.): used unattended at driver startup. Every field
//     is optional and every bad entry just falls back with a warning;
//     nothing here can abort startup.
//   - `payload` (ConfigPayload): used by the `configui` web page's save
//     button, a request a human is actively watching in the browser. Every
//     field is required and the first invalid name aborts the whole save
//     with a specific error message, so the user gets immediate, precise
//     feedback instead of a silently-partial write.
// The types below (DriverConfig and everything it's built from) are shared
// by both and so live here, at the top of the module tree; `load`/`payload`
// are child modules and can see private items defined here (including
// build_effect/parse_color/parse_direction, which both need).

mod load;
mod payload;

pub use load::{load, try_reload};
pub use payload::ConfigPayload;

use crate::lighting::{self, Color, Effect, LayerIndicatorConfig, LightingConfig, WaveDirection};
use crate::{MAX_LAYERS, NUM_KEYS};
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_LMENU};

// config.toml's path is resolved at runtime relative to the running binary
// — see main.rs's `app_root()`/`config_path()` for why this isn't a
// compile-time constant.

// Built-in defaults, used whenever config.toml doesn't specify a given
// entry. These are exactly the placeholder TEST_KEYMAP / LAYER1_TEST_KEYMAP /
// D-pad-wheel-middle-click test keys the driver shipped with before Phase 4,
// so a machine with no config.toml behaves identically to before.
const DEFAULT_ANALOG: [VIRTUAL_KEY; NUM_KEYS] = crate::TEST_KEYMAP;
const DEFAULT_LAYER1: [VIRTUAL_KEY; NUM_KEYS] = crate::LAYER1_TEST_KEYMAP;
const DEFAULT_LAYER2: [VIRTUAL_KEY; NUM_KEYS] = crate::LAYER2_TEST_KEYMAP;

// index 0 = Default, 1 = Layer1, 2 = Layer2 (main.rs::MAX_LAYERS). [keys.layer2]
// is always parsed/stored regardless of [hypershift] mode/switch_style/
// layer_count — only reachable at runtime when switch_style="toggle" and
// layer_count=3 (see hypershift.rs), but harmless to keep configured
// otherwise, same fail-open philosophy as everything else in this module.
pub struct AnalogKeymap {
    pub layers: [[VIRTUAL_KEY; NUM_KEYS]; MAX_LAYERS],
}

// v1.0.5: what the physical "Hyper Response" thumb button does. LayerSwitch
// is the only behavior this project had before v1.0.5 (with SwitchStyle
// always effectively Momentary) — kept as the default so an existing
// config.toml with no [hypershift] section sees zero behavior change.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HypershiftMode {
    LayerSwitch,
    ModifierKey,
}

impl HypershiftMode {
    fn as_str(&self) -> &'static str {
        match self {
            HypershiftMode::LayerSwitch => "layer_switch",
            HypershiftMode::ModifierKey => "modifier_key",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "layer_switch" => Some(HypershiftMode::LayerSwitch),
            "modifier_key" => Some(HypershiftMode::ModifierKey),
            _ => None,
        }
    }
}

// Only meaningful when mode == LayerSwitch. Momentary always behaves as
// exactly 2 layers (held -> Layer1, released -> Default) regardless of
// layer_count; Toggle cycles through layer_count layers, advancing only on
// button press (see hypershift::on_trigger_edge).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SwitchStyle {
    Momentary,
    Toggle,
}

impl SwitchStyle {
    fn as_str(&self) -> &'static str {
        match self {
            SwitchStyle::Momentary => "momentary",
            SwitchStyle::Toggle => "toggle",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "momentary" => Some(SwitchStyle::Momentary),
            "toggle" => Some(SwitchStyle::Toggle),
            _ => None,
        }
    }
}

pub struct HypershiftConfig {
    pub mode: HypershiftMode,
    pub switch_style: SwitchStyle,
    // Only meaningful for (LayerSwitch, Toggle); always validated to 2 or 3.
    pub layer_count: u8,
    // Only meaningful for ModifierKey mode: the key sent on press/release in
    // place of the button's own (suppressed) physical Alt keycode.
    pub modifier_key: VIRTUAL_KEY,
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

// Hysteresis thresholds (docs/DESIGN.md §6.1) on the raw 0-255 analog depth
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
    pub hypershift: HypershiftConfig,
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
    // `configui`'s own display language ("en" | "ja"), not read by the driver
    // itself at all — persisted here purely so the browser page remembers
    // the user's choice across restarts instead of resetting every time.
    pub configui: ConfiguiSettings,
}

pub struct ConfiguiSettings {
    pub language: String,
}

impl Default for ConfiguiSettings {
    fn default() -> Self {
        ConfiguiSettings { language: "en".to_string() }
    }
}

impl DriverConfig {
    pub fn defaults() -> Self {
        DriverConfig {
            analog: AnalogKeymap {
                layers: [DEFAULT_ANALOG, DEFAULT_LAYER1, DEFAULT_LAYER2],
            },
            hypershift: HypershiftConfig {
                mode: HypershiftMode::LayerSwitch,
                switch_style: SwitchStyle::Momentary,
                layer_count: 2,
                modifier_key: VK_LMENU,
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
            configui: ConfiguiSettings::default(),
        }
    }
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

/// Builds an Effect from its string/numeric parts. Shared by load's
/// RawLighting handling and payload's validate()/save path so the two
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_pre_phase4_placeholder_keymaps() {
        let cfg = DriverConfig::defaults();
        assert_eq!(cfg.analog.layers[0], crate::TEST_KEYMAP);
        assert_eq!(cfg.analog.layers[1], crate::LAYER1_TEST_KEYMAP);
        assert_eq!(cfg.analog.layers[2], crate::LAYER2_TEST_KEYMAP);
        assert_eq!(cfg.dpad.left, crate::dpad::DPAD_ARROW_TEST_KEYMAP_LEFT);
    }
}
