// `emulate` subcommand: a hardware-free debug harness for the analog-key
// hysteresis + keymap + SendInput + Hyper Shift layer-switch logic
// (`process_key_depths`/`force_keyup_on_layer_change` in main.rs).
// Lets that logic — and config.toml's keymap/actuation settings — be
// exercised and watched end-to-end from a terminal without the physical
// Tartarus Pro plugged in at all. Purely additive: never touches HID,
// Interception, or the Razer control device (no unlock command, no
// lighting, no D-pad), so it can't need or interfere with any of that.
//
// Not a tick-based loop: real hardware resends every key's depth on every
// report even when nothing changed, but process_key_depths() only reacts to
// a value actually crossing t_on/t_off (see its `pressed_vk[i].is_none()`
// guard), so unchanged depths are inert. That means this can just react to
// one command at a time rather than re-polling a shared array continuously
// — simpler, and behaviorally identical for what this tool is for.

use crate::vkname::vk_to_name;
use crate::{config, eprintln, println, NUM_KEYS};
use std::io::BufRead;
use std::sync::atomic::Ordering;
use std::time::Instant;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

fn print_help() {
    println!("Commands (Enter to run, Ctrl+C or \"quit\" to exit):");
    println!("  <N>          tap key N (1-{NUM_KEYS}): DOWN then UP");
    println!("  <N> down     hold key N down until \"<N> up\"");
    println!("  <N> up       release key N");
    println!("  <N> <0-255>  set key N's raw depth directly (test exact t_on/t_off thresholds)");
    println!("  hyper        tap the Hyper Response button: press then release");
    println!("  hyper down   hold the Hyper Response button down until \"hyper up\"");
    println!("  hyper up     release the Hyper Response button");
    println!("  help         show this again");
    println!("  quit         exit");
}

fn parse_key_index(s: &str) -> Option<usize> {
    let n: usize = s.parse().ok()?;
    (1..=NUM_KEYS).contains(&n).then(|| n - 1)
}

fn report_key_state(idx: usize, pressed_vk: &[Option<VIRTUAL_KEY>; NUM_KEYS]) {
    match pressed_vk[idx] {
        Some(vk) => println!("[emulate] key{:02} is now DOWN -> sends \"{}\"", idx + 1, vk_to_name(vk)),
        None => println!("[emulate] key{:02} is now up", idx + 1),
    }
}

// One Hyper Response press/release edge, mirroring what run_driver's loop
// does every iteration (main.rs): route the edge through
// hypershift::on_trigger_edge, then — ONLY if that moved the active layer
// back to Default (from any other layer) — force-release every key still
// logically held (force_keyup_on_layer_change), exactly like the real driver
// loop. Not on every transition: a key already held when Hyper Shift engages
// (Default -> Layer1) is meant to keep sending whatever it was pressed with
// for the rest of that hold — see force_keyup_on_layer_change's call site in
// main.rs for why. Detailed logging of what the edge actually did (layer
// changed vs. modifier key sent) comes from on_trigger_edge itself
// (hypershift.rs), not duplicated here.
fn fire_hyper_edge(pressed: bool, pressed_vk: &mut [Option<VIRTUAL_KEY>; NUM_KEYS], start: Instant) {
    let before = crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst);
    crate::hypershift::on_trigger_edge(pressed);
    let after = crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst);
    if before != 0 && after == 0 {
        crate::force_keyup_on_layer_change(pressed_vk, start);
    }
}

// Runs one line of input. Returns false when the emulator should exit.
fn handle_command(
    line: &str,
    depths: &mut [u8; NUM_KEYS],
    pressed_vk: &mut [Option<VIRTUAL_KEY>; NUM_KEYS],
    start: Instant,
) -> bool {
    let line = line.trim();
    let mut parts = line.split_whitespace();
    let Some(first) = parts.next() else { return true };

    match first {
        "quit" | "exit" => return false,
        "help" => {
            print_help();
            return true;
        }
        "hyper" => {
            match parts.next() {
                None => {
                    fire_hyper_edge(true, pressed_vk, start);
                    fire_hyper_edge(false, pressed_vk, start);
                }
                Some("down") => fire_hyper_edge(true, pressed_vk, start),
                Some("up") => fire_hyper_edge(false, pressed_vk, start),
                Some(other) => println!(
                    "[emulate] unrecognized \"hyper {other}\" (expected \"down\", \"up\", or nothing for a tap)"
                ),
            }
            return true;
        }
        _ => {}
    }

    let Some(idx) = parse_key_index(first) else {
        println!("[emulate] unrecognized command: \"{line}\" (type \"help\" for the list)");
        return true;
    };
    let layer = crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst) as usize;

    // Every branch below is "set key idx's depth, then run it through the
    // same processing the real driver loop uses" — a tap is just that pair
    // called twice (255 then 0).
    let mut set_depth = |v: u8| {
        depths[idx] = v;
        crate::process_key_depths(depths, layer, pressed_vk, start);
    };
    match parts.next() {
        None => {
            set_depth(255);
            set_depth(0);
        }
        Some("down") => set_depth(255),
        Some("up") => set_depth(0),
        Some(other) => match other.parse::<u16>() {
            Ok(v) if v <= 255 => set_depth(v as u8),
            _ => {
                println!("[emulate] depth must be 0-255 (got \"{other}\")");
                return true;
            }
        },
    }
    report_key_state(idx, pressed_vk);
    true
}

pub fn run_emulator() {
    crate::CONFIG.set(config::load()).ok();

    println!("tartarus_driver emulate mode — no HID device, no Interception, no hardware needed.");
    print_help();

    let start = Instant::now();
    let mut depths = [0u8; NUM_KEYS];
    let mut pressed_vk: [Option<VIRTUAL_KEY>; NUM_KEYS] = [None; NUM_KEYS];

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[emulate] stdin read error: {e}");
                break;
            }
        };
        if !handle_command(&line, &mut depths, &mut pressed_vk, start) {
            break;
        }
    }

    for slot in pressed_vk.iter_mut() {
        if let Some(vk) = slot.take() {
            crate::send_key(vk, true);
        }
    }
    println!("[emulate] Done.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_index_accepts_1_to_num_keys_only() {
        assert_eq!(parse_key_index("1"), Some(0));
        assert_eq!(parse_key_index("20"), Some(19));
        assert_eq!(parse_key_index("0"), None);
        assert_eq!(parse_key_index("21"), None);
        assert_eq!(parse_key_index("abc"), None);
    }

    // Exercises handle_command's stateful commands (tap/down/up/explicit
    // depth/hyper) AND hypershift::on_trigger_edge_with's toggle/
    // modifier_key behavior in one test, sequentially, rather than splitting
    // into several independent #[test]s: CONFIG and hypershift::CURRENT_LAYER
    // are process-wide statics shared with the rest of the crate, and cargo
    // test runs tests in parallel by default, so multiple tests touching the
    // same shared mutable state could race against each other. Nothing else
    // in the crate's test suite touches either static, so this one test
    // owning all of it is safe. (on_trigger_edge_with itself takes its
    // HypershiftConfig by parameter — see hypershift.rs — specifically so
    // the toggle/modifier_key assertions below don't need a second,
    // differently-configured CONFIG, which the OnceLock couldn't provide
    // anyway once handle_command's own calls have already initialized it
    // with DriverConfig::defaults().)
    #[test]
    fn handle_command_dispatches_tap_hold_depth_and_hyper() {
        crate::CONFIG.get_or_init(crate::config::DriverConfig::defaults);
        let start = Instant::now();
        let mut depths = [0u8; NUM_KEYS];
        let mut pressed_vk: [Option<VIRTUAL_KEY>; NUM_KEYS] = [None; NUM_KEYS];

        // Unrecognized/empty input doesn't panic and keeps the loop running.
        assert!(handle_command("bogus", &mut depths, &mut pressed_vk, start));
        assert!(handle_command("", &mut depths, &mut pressed_vk, start));

        // A bare tap presses then releases within the same call.
        assert!(handle_command("5", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_none());

        // "down" holds; "up" releases it.
        assert!(handle_command("5 down", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_some());
        assert!(handle_command("5 up", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_none());

        // An explicit depth crossing t_on presses; below t_off releases.
        assert!(handle_command("5 250", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_some());
        assert!(handle_command("5 0", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_none());

        // Out-of-range depth is rejected without touching state.
        assert!(handle_command("5 300", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_none());

        // "hyper down"/"hyper up" drive CURRENT_LAYER via the default config
        // (layer_switch + momentary), and force-release held keys on the
        // change back to Default only (Purpose.md §6② step 3).
        crate::hypershift::CURRENT_LAYER.store(0, Ordering::SeqCst);
        assert!(handle_command("hyper down", &mut depths, &mut pressed_vk, start));
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 1);
        assert!(handle_command("5 down", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_some());
        assert!(handle_command("hyper up", &mut depths, &mut pressed_vk, start));
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 0);
        assert!(pressed_vk[4].is_none());

        // Regression check: a key already held under Default when Hyper
        // Shift engages must NOT be force-released on that press edge — it
        // keeps sending its Default-layer VK for the rest of the hold
        // (TEST_KEYMAP key05 -> '5'), only getting force-released when the
        // layer returns to Default. An earlier v1.0.5 build force-released
        // on every transition (including this one), which sent both the
        // Default AND Layer1 key back-to-back the instant Hyper Shift
        // engaged (reported as "1 and 6 both get typed" against a real
        // config where key01 was "1"/layer1 "6").
        crate::hypershift::CURRENT_LAYER.store(0, Ordering::SeqCst);
        assert!(handle_command("5 down", &mut depths, &mut pressed_vk, start));
        let vk_while_default = pressed_vk[4];
        assert!(vk_while_default.is_some());
        assert!(handle_command("hyper down", &mut depths, &mut pressed_vk, start));
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 1);
        // Still holding the SAME vk it was pressed with (Default's), not
        // force-released and not re-pressed under Layer1's mapping.
        assert_eq!(pressed_vk[4], vk_while_default);
        assert!(handle_command("hyper up", &mut depths, &mut pressed_vk, start));
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 0);
        assert!(pressed_vk[4].is_none()); // NOW force-released, on the return to Default
        assert!(handle_command("5 up", &mut depths, &mut pressed_vk, start)); // physical release, no-op

        // A bare "hyper" tap presses then releases: momentary ends back on
        // Default (0).
        assert!(handle_command("hyper", &mut depths, &mut pressed_vk, start));
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 0);

        // Toggle mode, 3 layers: each press-edge advances one step,
        // wrapping around; release is inert.
        crate::hypershift::CURRENT_LAYER.store(0, Ordering::SeqCst);
        let toggle3 = crate::config::HypershiftConfig {
            mode: crate::config::HypershiftMode::LayerSwitch,
            switch_style: crate::config::SwitchStyle::Toggle,
            layer_count: 3,
            modifier_key: crate::vkname::vk_from_name("LALT").unwrap(),
        };
        for expected in [1u8, 2, 0, 1] {
            crate::hypershift::on_trigger_edge_with(true, &toggle3);
            assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), expected);
            crate::hypershift::on_trigger_edge_with(false, &toggle3); // release is inert
            assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), expected);
        }

        // modifier_key mode: CURRENT_LAYER never leaves 0 regardless of
        // press/release (it's not a layer trigger at all in this mode).
        crate::hypershift::CURRENT_LAYER.store(0, Ordering::SeqCst);
        let modifier = crate::config::HypershiftConfig {
            mode: crate::config::HypershiftMode::ModifierKey,
            switch_style: crate::config::SwitchStyle::Momentary,
            layer_count: 2,
            modifier_key: crate::vkname::vk_from_name("LALT").unwrap(),
        };
        crate::hypershift::on_trigger_edge_with(true, &modifier);
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 0);
        crate::hypershift::on_trigger_edge_with(false, &modifier);
        assert_eq!(crate::hypershift::CURRENT_LAYER.load(Ordering::SeqCst), 0);

        // Leave CURRENT_LAYER at a known value (0) for any test run after
        // this one in the same process.
        crate::hypershift::CURRENT_LAYER.store(0, Ordering::SeqCst);

        // "quit"/"exit" signal the emulator to stop.
        assert!(!handle_command("quit", &mut depths, &mut pressed_vk, start));
        assert!(!handle_command("exit", &mut depths, &mut pressed_vk, start));
    }
}
