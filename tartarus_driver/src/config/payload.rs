// Interactive-save path: the `configui` web page's GET/POST /api/config
// shape. Unlike load.rs's fail-open-per-field semantics, every field here is
// required and the first invalid name aborts the whole save with a specific
// error message — see the module doc comment in config/mod.rs.

use super::{build_effect, DriverConfig};
use crate::lighting::{Effect, LightingConfig, ProfileLedColor, WaveDirection};
use crate::vkname::{vk_from_name, vk_to_name};
use crate::NUM_KEYS;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ConfigPayload {
    pub keys_default: Vec<String>, // exactly NUM_KEYS entries, key01..key20 in order
    pub keys_layer1: Vec<String>,  // exactly NUM_KEYS entries, key01..key20 in order
    pub keys_layer2: Vec<String>,  // exactly NUM_KEYS entries, key01..key20 in order
    pub hypershift_mode: String,          // "layer_switch" | "modifier_key"
    pub hypershift_switch_style: String,  // "momentary" | "toggle"
    pub hypershift_layer_count: u8,       // 2 | 3
    pub hypershift_modifier_key: String,
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
    pub language: String,              // "en" | "ja" — configui's own display language
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
        Effect::Static(c) => ("static".to_string(), crate::lighting::color_to_hex(c), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Breathing(c) => ("breathing".to_string(), crate::lighting::color_to_hex(c), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Spectrum => ("spectrum".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, PLACEHOLDER_DIRECTION.to_string(), PLACEHOLDER_SPEED),
        Effect::Wave(dir) => {
            let dir_str = match dir { WaveDirection::Left => "left", WaveDirection::Right => "right" };
            ("wave".to_string(), PLACEHOLDER_COLOR.to_string(), brightness, dir_str.to_string(), PLACEHOLDER_SPEED)
        }
        Effect::Reactive { speed, color } => ("reactive".to_string(), crate::lighting::color_to_hex(color), brightness, PLACEHOLDER_DIRECTION.to_string(), *speed),
    }
}

impl ConfigPayload {
    pub fn from_driver_config(cfg: &DriverConfig) -> Self {
        let (lighting_effect, lighting_color, lighting_brightness, lighting_wave_direction, lighting_reactive_speed) =
            lighting_to_payload_fields(&cfg.lighting);
        ConfigPayload {
            keys_default: cfg.analog.layers[0].iter().map(|vk| vk_to_name(*vk)).collect(),
            keys_layer1: cfg.analog.layers[1].iter().map(|vk| vk_to_name(*vk)).collect(),
            keys_layer2: cfg.analog.layers[2].iter().map(|vk| vk_to_name(*vk)).collect(),
            hypershift_mode: cfg.hypershift.mode.as_str().to_string(),
            hypershift_switch_style: cfg.hypershift.switch_style.as_str().to_string(),
            hypershift_layer_count: cfg.hypershift.layer_count,
            hypershift_modifier_key: vk_to_name(cfg.hypershift.modifier_key),
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
            language: cfg.configui.language.clone(),
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
        if self.keys_layer2.len() != NUM_KEYS {
            return Err(format!("keys_layer2 には{NUM_KEYS}件必要です"));
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
        for (i, name) in self.keys_layer2.iter().enumerate() {
            check(format!("keys.layer2.key{:02}", i + 1), name)?;
        }
        if self.hypershift_mode != "layer_switch" && self.hypershift_mode != "modifier_key" {
            return Err(format!(
                "hypershift.mode: \"{}\" は \"layer_switch\" または \"modifier_key\" である必要があります",
                self.hypershift_mode
            ));
        }
        if self.hypershift_switch_style != "momentary" && self.hypershift_switch_style != "toggle" {
            return Err(format!(
                "hypershift.switch_style: \"{}\" は \"momentary\" または \"toggle\" である必要があります",
                self.hypershift_switch_style
            ));
        }
        if self.hypershift_layer_count != 2 && self.hypershift_layer_count != 3 {
            return Err(format!(
                "hypershift.layer_count: {} は2または3である必要があります",
                self.hypershift_layer_count
            ));
        }
        check("hypershift.modifier_key".into(), &self.hypershift_modifier_key)?;
        for (label, name) in [
            ("dpad.left", &self.dpad_left),
            ("dpad.up", &self.dpad_up),
            ("dpad.right", &self.dpad_right),
            ("dpad.down", &self.dpad_down),
            ("wheel.up", &self.wheel_up),
            ("wheel.down", &self.wheel_down),
            ("middle_click.key", &self.middle_click),
        ] {
            check(label.into(), name)?;
        }
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
        if self.language != "en" && self.language != "ja" {
            return Err(format!(
                "configui.language: \"{}\" は \"en\" または \"ja\" である必要があります",
                self.language
            ));
        }
        Ok(())
    }

    /// validate()s, then overwrites config.toml wholesale on success.
    pub fn validate_and_save(&self) -> Result<(), String> {
        self.validate()?;
        std::fs::write(crate::config_path(), self.to_toml_string())
            .map_err(|e| format!("config.tomlへの書き込みに失敗しました: {e}"))
    }

    fn to_toml_string(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::from("# tartarus_driver key remap config — generated by `configui`\n\n[keys.default]\n");
        for (i, name) in self.keys_default.iter().enumerate() {
            writeln!(s, "key{:02} = \"{name}\"", i + 1).unwrap();
        }
        s.push_str("\n[keys.layer1]\n");
        for (i, name) in self.keys_layer1.iter().enumerate() {
            writeln!(s, "key{:02} = \"{name}\"", i + 1).unwrap();
        }
        s.push_str("\n[keys.layer2]\n");
        for (i, name) in self.keys_layer2.iter().enumerate() {
            writeln!(s, "key{:02} = \"{name}\"", i + 1).unwrap();
        }
        write!(
            s,
            "\n[hypershift]\nmode = \"{}\"\nswitch_style = \"{}\"\nlayer_count = {}\nmodifier_key = \"{}\"\n",
            self.hypershift_mode, self.hypershift_switch_style, self.hypershift_layer_count, self.hypershift_modifier_key
        )
        .unwrap();
        write!(
            s,
            "\n[dpad]\nleft = \"{}\"\nup = \"{}\"\nright = \"{}\"\ndown = \"{}\"\n",
            self.dpad_left, self.dpad_up, self.dpad_right, self.dpad_down
        )
        .unwrap();
        write!(s, "\n[wheel]\nup = \"{}\"\ndown = \"{}\"\n", self.wheel_up, self.wheel_down).unwrap();
        write!(s, "\n[middle_click]\nkey = \"{}\"\n", self.middle_click).unwrap();
        write!(s, "\n[actuation]\nt_on = {}\nt_off = {}\n", self.t_on, self.t_off).unwrap();
        let overrides: Vec<(usize, u8, u8)> = (0..NUM_KEYS)
            .filter_map(|i| match (self.per_key_t_on[i], self.per_key_t_off[i]) {
                (Some(on), Some(off)) => Some((i, on, off)),
                _ => None,
            })
            .collect();
        if !overrides.is_empty() {
            s.push_str("\n[actuation.per_key]\n");
            for (i, on, off) in overrides {
                writeln!(s, "key{:02} = {{ t_on = {on}, t_off = {off} }}", i + 1).unwrap();
            }
        }
        write!(
            s,
            "\n[lighting]\neffect = \"{}\"\ncolor = \"{}\"\nbrightness = {}\nwave_direction = \"{}\"\nreactive_speed = {}\n",
            self.lighting_effect, self.lighting_color, self.lighting_brightness, self.lighting_wave_direction, self.lighting_reactive_speed
        )
        .unwrap();
        write!(
            s,
            "\n[layer_indicator]\nenabled = {}\ncolor = \"{}\"\n",
            self.layer_indicator_enabled, self.layer_indicator_color
        )
        .unwrap();
        write!(s, "\n[configui]\nlanguage = \"{}\"\n", self.language).unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypershift_defaults_and_payload_round_trip() {
        let cfg = DriverConfig::defaults();
        assert!(matches!(cfg.hypershift.mode, crate::config::HypershiftMode::LayerSwitch));
        assert!(matches!(cfg.hypershift.switch_style, crate::config::SwitchStyle::Momentary));
        assert_eq!(cfg.hypershift.layer_count, 2);
        assert_eq!(cfg.hypershift.modifier_key, windows::Win32::UI::Input::KeyboardAndMouse::VK_LMENU);
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.hypershift_mode, "layer_switch");
        assert_eq!(payload.hypershift_switch_style, "momentary");
        assert_eq!(payload.hypershift_layer_count, 2);
        assert_eq!(payload.hypershift_modifier_key, "LALT");
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn payload_rejects_bad_hypershift_fields() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);

        payload.hypershift_mode = "bogus".to_string();
        assert!(payload.validate().is_err());
        payload.hypershift_mode = "modifier_key".to_string();
        assert!(payload.validate().is_ok());

        payload.hypershift_switch_style = "bogus".to_string();
        assert!(payload.validate().is_err());
        payload.hypershift_switch_style = "toggle".to_string();
        assert!(payload.validate().is_ok());

        payload.hypershift_layer_count = 4;
        assert!(payload.validate().is_err());
        payload.hypershift_layer_count = 3;
        assert!(payload.validate().is_ok());

        payload.hypershift_modifier_key = "NOT_A_KEY".to_string();
        assert!(payload.validate().is_err());
        payload.hypershift_modifier_key = "RALT".to_string();
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn keys_layer2_round_trips_and_is_validated() {
        let cfg = DriverConfig::defaults();
        assert_eq!(cfg.analog.layers[2], crate::LAYER2_TEST_KEYMAP);
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.keys_layer2.len(), NUM_KEYS);

        payload.keys_layer2[0] = "NOT_A_KEY".to_string();
        let err = payload.validate().unwrap_err();
        assert!(err.contains("keys.layer2.key01"));

        payload.keys_layer2 = payload.keys_layer2.iter().map(|_| "Z".to_string()).collect();
        let toml_text = payload.to_toml_string();
        assert!(toml_text.contains("[keys.layer2]"));
        assert!(toml_text.contains("key01 = \"Z\""));
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
    fn layer_indicator_defaults_to_disabled_and_round_trips() {
        let cfg = DriverConfig::defaults();
        assert!(cfg.layer_indicator.is_none());
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert!(!payload.layer_indicator_enabled);
        assert_eq!(payload.layer_indicator_color, "green");
        assert!(payload.validate().is_ok());
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

    #[test]
    fn configui_language_defaults_to_english_and_round_trips() {
        let cfg = DriverConfig::defaults();
        assert_eq!(cfg.configui.language, "en");
        let payload = ConfigPayload::from_driver_config(&cfg);
        assert_eq!(payload.language, "en");
        assert!(payload.validate().is_ok());
    }

    #[test]
    fn payload_rejects_unrecognized_language() {
        let cfg = DriverConfig::defaults();
        let mut payload = ConfigPayload::from_driver_config(&cfg);
        payload.language = "fr".to_string();
        assert!(payload.validate().is_err());

        payload.language = "ja".to_string();
        assert!(payload.validate().is_ok());
    }
}
