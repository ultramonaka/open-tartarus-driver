// Human-readable key-name <-> VIRTUAL_KEY conversion, used by config.toml
// parsing and the `configui` web page. Deliberately a CLOSED vocabulary (not
// "any VK code the user types") so every name that round-trips through here
// either produces a real, intentional key or a caught error — never a
// silently wrong/unmapped virtual-key value. `all_key_names()` is the single
// source of truth for what configui.html's <select> options are allowed to
// contain (fetched from the running driver via GET /api/key-options rather
// than duplicated in the HTML, so the two can never drift apart).

use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT,
    VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
    VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP, VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_SPACE, VK_TAB, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};

// Named keys beyond plain digits/letters/F-keys. (name, VIRTUAL_KEY) pairs so
// vk_from_name and vk_to_name share one definition and cannot disagree.
const KEY_TABLE: &[(&str, VIRTUAL_KEY)] = &[
    ("LEFT", VK_LEFT),
    ("UP", VK_UP),
    ("RIGHT", VK_RIGHT),
    ("DOWN", VK_DOWN),
    ("SPACE", VK_SPACE),
    ("ENTER", VK_RETURN),
    ("TAB", VK_TAB),
    ("ESCAPE", VK_ESCAPE),
    ("BACKSPACE", VK_BACK),
    ("LSHIFT", VK_LSHIFT),
    ("RSHIFT", VK_RSHIFT),
    ("LCTRL", VK_LCONTROL),
    ("RCTRL", VK_RCONTROL),
    ("LALT", VK_LMENU),
    ("RALT", VK_RMENU),
    ("HOME", VK_HOME),
    ("END", VK_END),
    ("PAGEUP", VK_PRIOR),
    ("PAGEDOWN", VK_NEXT),
    ("INSERT", VK_INSERT),
    ("DELETE", VK_DELETE),
];

// v1.0.5: media/volume keys, kept in a separate table (not merged into
// KEY_TABLE) so configui's key picker can offer them as their own "Media
// Control" category (see key_names_grouped below) instead of mixed into the
// much longer basic list.
const MEDIA_KEY_TABLE: &[(&str, VIRTUAL_KEY)] = &[
    ("MEDIA_PLAY_PAUSE", VK_MEDIA_PLAY_PAUSE),
    ("MEDIA_STOP", VK_MEDIA_STOP),
    ("MEDIA_NEXT", VK_MEDIA_NEXT_TRACK),
    ("MEDIA_PREV", VK_MEDIA_PREV_TRACK),
    ("VOLUME_MUTE", VK_VOLUME_MUTE),
    ("VOLUME_DOWN", VK_VOLUME_DOWN),
    ("VOLUME_UP", VK_VOLUME_UP),
];

const NUM_F_KEYS: u16 = 24;

/// Parses a human-readable key name (case-insensitive) into a VIRTUAL_KEY.
/// Accepts: single digits "0".."9", single letters "A".."Z", "F1".."F24",
/// and the names in KEY_TABLE. Returns None for anything else.
pub fn vk_from_name(name: &str) -> Option<VIRTUAL_KEY> {
    let upper = name.trim().to_ascii_uppercase();

    if let Some(rest) = upper.strip_prefix('F')
        && let Ok(n) = rest.parse::<u16>()
        && (1..=NUM_F_KEYS).contains(&n)
    {
        return Some(VIRTUAL_KEY(VK_F1.0 + n - 1));
    }

    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        if c.is_ascii_digit() || c.is_ascii_uppercase() {
            return Some(VIRTUAL_KEY(c as u16));
        }
    }

    KEY_TABLE
        .iter()
        .chain(MEDIA_KEY_TABLE)
        .find(|(n, _)| *n == upper)
        .map(|(_, vk)| *vk)
}

/// Inverse of vk_from_name. Every VIRTUAL_KEY this crate ever assigns via
/// config (built-in defaults or a loaded config.toml, both always sourced
/// from vk_from_name) round-trips back to its original name; anything else
/// falls back to a "0xNN" string so the configui page can still display it
/// (as an unselectable option) instead of silently showing the wrong key.
pub fn vk_to_name(vk: VIRTUAL_KEY) -> String {
    let code = vk.0;

    if (VK_F1.0..VK_F1.0 + NUM_F_KEYS).contains(&code) {
        return format!("F{}", code - VK_F1.0 + 1);
    }
    if (b'0' as u16..=b'9' as u16).contains(&code) || (b'A' as u16..=b'Z' as u16).contains(&code) {
        return (code as u8 as char).to_string();
    }
    KEY_TABLE
        .iter()
        .chain(MEDIA_KEY_TABLE)
        .find(|(_, k)| k.0 == code)
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| format!("0x{code:02X}"))
}

/// Every name vk_from_name accepts, in display order. Used to populate
/// configui.html's dropdowns via GET /api/key-options so the web page's
/// vocabulary can never drift from what the driver actually accepts.
pub fn all_key_names() -> Vec<String> {
    let mut names: Vec<String> = ('0'..='9').map(String::from).collect();
    names.extend(('A'..='Z').map(String::from));
    names.extend((1..=NUM_F_KEYS).map(|n| format!("F{n}")));
    names.extend(KEY_TABLE.iter().map(|(n, _)| n.to_string()));
    names
}

/// (basic names, media names) — the same vocabulary as all_key_names(), just
/// split into the two categories configui.html's key picker offers. Used by
/// configui.rs's GET /api/key-options.
pub fn key_names_grouped() -> (Vec<String>, Vec<String>) {
    (all_key_names(), MEDIA_KEY_TABLE.iter().map(|(n, _)| n.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_and_letters_round_trip() {
        for c in '0'..='9' {
            let vk = vk_from_name(&c.to_string()).unwrap();
            assert_eq!(vk_to_name(vk), c.to_string());
        }
        for c in 'A'..='Z' {
            let vk = vk_from_name(&c.to_string()).unwrap();
            assert_eq!(vk_to_name(vk), c.to_string());
        }
    }

    #[test]
    fn function_keys_round_trip_and_are_bounded() {
        for n in 1..=24 {
            let name = format!("F{n}");
            let vk = vk_from_name(&name).unwrap();
            assert_eq!(vk_to_name(vk), name);
        }
        assert_eq!(vk_from_name("F0"), None);
        assert_eq!(vk_from_name("F25"), None);
    }

    #[test]
    fn named_specials_round_trip() {
        for (name, _) in KEY_TABLE {
            let vk = vk_from_name(name).unwrap();
            assert_eq!(vk_to_name(vk), *name);
        }
    }

    #[test]
    fn media_keys_round_trip() {
        for (name, _) in MEDIA_KEY_TABLE {
            let vk = vk_from_name(name).unwrap();
            assert_eq!(vk_to_name(vk), *name);
        }
    }

    #[test]
    fn key_names_grouped_matches_all_key_names_plus_media() {
        let (basic, media) = key_names_grouped();
        assert_eq!(basic, all_key_names());
        assert_eq!(media.len(), MEDIA_KEY_TABLE.len());
        for (name, _) in MEDIA_KEY_TABLE {
            assert!(media.contains(&name.to_string()));
            assert!(!basic.contains(&name.to_string()));
        }
    }

    #[test]
    fn case_insensitive_and_unknown_names_rejected() {
        assert_eq!(vk_from_name("left"), Some(VK_LEFT));
        assert_eq!(vk_from_name("f1"), vk_from_name("F1"));
        assert_eq!(vk_from_name("NOT_A_KEY"), None);
        assert_eq!(vk_from_name(""), None);
    }

    #[test]
    fn all_key_names_are_all_individually_valid() {
        let names = all_key_names();
        assert!(names.len() > 50);
        for name in &names {
            assert!(vk_from_name(name).is_some(), "{name} did not round-trip");
        }
    }
}
