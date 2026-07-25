// Unattended parsing path: config.toml -> DriverConfig, used at driver
// startup and by v1.0.6's hot-reload. Every field is optional and every bad
// entry just falls back to a built-in default with a warning — see the
// module doc comment in config/mod.rs for how this differs from payload.rs's
// stricter, interactive-save semantics.

use super::{build_effect, Actuation, ConfiguiSettings, DriverConfig, HypershiftConfig, HypershiftMode, SwitchStyle};
use crate::lighting::{LayerIndicatorConfig, LightingConfig, ProfileLedColor};
use crate::vkname::vk_from_name;
use crate::{eprintln, println, NUM_KEYS};
use serde::Deserialize;
use std::collections::HashMap;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

#[derive(Deserialize, Default)]
struct RawKeys {
    default: Option<HashMap<String, String>>,
    layer1: Option<HashMap<String, String>>,
    layer2: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct RawHypershift {
    mode: Option<String>,
    switch_style: Option<String>,
    layer_count: Option<u8>,
    modifier_key: Option<String>,
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
struct RawConfigui {
    language: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawConfig {
    keys: Option<RawKeys>,
    hypershift: Option<RawHypershift>,
    dpad: Option<RawDpad>,
    wheel: Option<RawWheel>,
    middle_click: Option<RawMiddleClick>,
    actuation: Option<RawActuation>,
    lighting: Option<RawLighting>,
    layer_indicator: Option<RawLayerIndicator>,
    configui: Option<RawConfigui>,
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

// Applies [hypershift], if present. Each field is validated and falls back
// independently to its own built-in default with a warning on a bad value —
// unlike apply_actuation/apply_lighting, these four fields don't interact
// with each other (mode/switch_style/layer_count/modifier_key are each
// independently meaningful), so there's no cross-field pair to protect the
// way t_on/t_off or a lighting effect+params are.
fn apply_hypershift(target: &mut HypershiftConfig, provided: &Option<RawHypershift>) {
    let Some(raw) = provided else { return };
    if let Some(mode) = &raw.mode {
        match HypershiftMode::from_str(mode) {
            Some(m) => target.mode = m,
            None => eprintln!(
                "WARNING: config.toml [hypershift] mode = \"{mode}\" is not \"layer_switch\" or \
                 \"modifier_key\"; keeping the default (\"layer_switch\")."
            ),
        }
    }
    if let Some(style) = &raw.switch_style {
        match SwitchStyle::from_str(style) {
            Some(s) => target.switch_style = s,
            None => eprintln!(
                "WARNING: config.toml [hypershift] switch_style = \"{style}\" is not \"momentary\" \
                 or \"toggle\"; keeping the default (\"momentary\")."
            ),
        }
    }
    if let Some(n) = raw.layer_count {
        if n == 2 || n == 3 {
            target.layer_count = n;
        } else {
            eprintln!(
                "WARNING: config.toml [hypershift] layer_count = {n} must be 2 or 3; keeping the \
                 default (2)."
            );
        }
    }
    if let Some(name) = &raw.modifier_key {
        match vk_from_name(name) {
            Some(vk) => target.modifier_key = vk,
            None => eprintln!(
                "WARNING: config.toml [hypershift] modifier_key = \"{name}\" is not a recognized \
                 key name; keeping the default (\"LALT\")."
            ),
        }
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

// Applies [configui], if present. Purely a UI preference (never read by the
// driver's own analog loop), so an invalid value just falls back to the
// default ("en") with a warning rather than affecting anything functional.
fn apply_configui(target: &mut ConfiguiSettings, provided: &Option<RawConfigui>) {
    let Some(raw) = provided else { return };
    let Some(lang) = &raw.language else { return };
    if lang == "en" || lang == "ja" {
        target.language = lang.clone();
    } else {
        eprintln!(
            "WARNING: config.toml [configui] language = \"{lang}\" is not \"en\" or \"ja\"; \
             keeping the default (\"en\")."
        );
    }
}

/// Loads config.toml if present, falling back to built-in placeholder
/// defaults for anything missing entirely (no file, or a file that doesn't
/// parse as TOML at all) or individually invalid (a specific bad field).
/// Prints one status line either way so `logs/run.log` always shows which
/// source was used. Startup-only: unlike `try_reload()`, "no usable file at
/// all" is an expected, harmless state here — a fresh install/first run
/// always looks like this via the built-in placeholder keymap.
pub fn load() -> DriverConfig {
    try_reload().unwrap_or_else(DriverConfig::defaults)
}

/// v1.0.6: the same parsing/fail-open logic `load()` uses, but returns
/// `None` — instead of silently falling back to hardcoded placeholder
/// defaults — when config.toml is missing or fails to parse as TOML at all.
/// This distinction matters specifically for hot-reloading an
/// already-running driver (see `run_driver`'s ~1s mtime-poll in main.rs): a
/// transient bad state (mid-write, a typo) must never blow away a working
/// custom config just because a reload happened to land on it — the caller
/// keeps whatever config it already had and simply retries on the next
/// check, self-healing once the file is fixed. An individual bad field
/// within an otherwise-valid file still fails open per-field exactly like
/// `load()` always has (see the `apply_*` functions above) — only "no
/// coherent file to parse at all" returns `None` here.
pub fn try_reload() -> Option<DriverConfig> {
    try_reload_from(&crate::config_path())
}

// The actual implementation, taking its path as a parameter instead of
// calling crate::config_path() directly — this is what makes it
// unit-testable against a throwaway temp file (missing-file / unparseable /
// partially-invalid cases) without ever touching the real, user-editable
// config.toml that crate::config_path() always resolves to.
fn try_reload_from(path: &std::path::Path) -> Option<DriverConfig> {
    let mut cfg = DriverConfig::defaults();

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => {
            println!(
                "No config.toml found at {} — using built-in placeholder keymap. \
                 Run `cargo run --release -- configui` to create one.",
                path.display()
            );
            return None;
        }
    };

    let raw: RawConfig = match toml::from_str(&text) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "WARNING: config.toml could not be parsed ({e}) — using built-in placeholder \
                 keymap entirely until this is fixed."
            );
            return None;
        }
    };

    if let Some(keys) = &raw.keys {
        if let Some(m) = &keys.default {
            apply_analog_map(&mut cfg.analog.layers[0], m, "keys.default");
        }
        if let Some(m) = &keys.layer1 {
            apply_analog_map(&mut cfg.analog.layers[1], m, "keys.layer1");
        }
        if let Some(m) = &keys.layer2 {
            apply_analog_map(&mut cfg.analog.layers[2], m, "keys.layer2");
        }
    }
    apply_hypershift(&mut cfg.hypershift, &raw.hypershift);
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
    apply_configui(&mut cfg.configui, &raw.configui);

    println!("Loaded config.toml from {}.", path.display());
    Some(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_LMENU;

    // try_reload_from() (not try_reload()/load(), which always resolve
    // crate::config_path() — the real, user-editable config.toml) is what
    // makes these three safe: each test uses its own throwaway file under
    // the OS temp dir, so there's no risk of a test corrupting the actual
    // config.toml a developer running `cargo test` locally might have.
    #[test]
    fn try_reload_from_missing_file_returns_none() {
        let path = std::env::temp_dir().join("tartarus_test_try_reload_missing.toml");
        let _ = std::fs::remove_file(&path); // ensure it doesn't exist
        assert!(try_reload_from(&path).is_none());
    }

    #[test]
    fn try_reload_from_malformed_toml_returns_none() {
        let path = std::env::temp_dir().join("tartarus_test_try_reload_malformed.toml");
        std::fs::write(&path, "this is not { valid toml [[[").unwrap();
        assert!(try_reload_from(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn try_reload_from_partially_invalid_toml_fails_open_per_field() {
        let path = std::env::temp_dir().join("tartarus_test_try_reload_partial.toml");
        // Valid TOML, but keys.default.key01 names an unrecognized key —
        // must fail open for that one field only (same as load()'s
        // existing per-field behavior), NOT return None for the whole file.
        std::fs::write(&path, "[keys.default]\nkey01 = \"NOT_A_REAL_KEY\"\n").unwrap();
        let cfg = try_reload_from(&path).expect("parseable TOML should still produce Some");
        assert_eq!(cfg.analog.layers[0][0], crate::TEST_KEYMAP[0]); // fell back to default for key01
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn apply_hypershift_rejects_bad_values_and_applies_good_ones() {
        let mut hs = HypershiftConfig {
            mode: HypershiftMode::LayerSwitch,
            switch_style: SwitchStyle::Momentary,
            layer_count: 2,
            modifier_key: VK_LMENU,
        };
        let bad = Some(RawHypershift {
            mode: Some("not_a_mode".to_string()),
            switch_style: Some("not_a_style".to_string()),
            layer_count: Some(5),
            modifier_key: Some("NOT_A_KEY".to_string()),
        });
        apply_hypershift(&mut hs, &bad);
        assert!(matches!(hs.mode, HypershiftMode::LayerSwitch));
        assert!(matches!(hs.switch_style, SwitchStyle::Momentary));
        assert_eq!(hs.layer_count, 2);
        assert_eq!(hs.modifier_key, VK_LMENU);

        let good = Some(RawHypershift {
            mode: Some("modifier_key".to_string()),
            switch_style: Some("toggle".to_string()),
            layer_count: Some(3),
            modifier_key: Some("LCTRL".to_string()),
        });
        apply_hypershift(&mut hs, &good);
        assert!(matches!(hs.mode, HypershiftMode::ModifierKey));
        assert!(matches!(hs.switch_style, SwitchStyle::Toggle));
        assert_eq!(hs.layer_count, 3);
        assert_eq!(hs.modifier_key, vk_from_name("LCTRL").unwrap());
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
        assert!(matches!(applied.effect, crate::lighting::Effect::Static(_)));
        assert_eq!(applied.brightness, Some(200));
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
    fn apply_configui_rejects_unknown_language_and_applies_a_good_one() {
        let mut settings = ConfiguiSettings::default();
        let bad = Some(RawConfigui { language: Some("fr".to_string()) });
        apply_configui(&mut settings, &bad);
        assert_eq!(settings.language, "en"); // unchanged, falls back to default

        let good = Some(RawConfigui { language: Some("ja".to_string()) });
        apply_configui(&mut settings, &good);
        assert_eq!(settings.language, "ja");
    }
}
