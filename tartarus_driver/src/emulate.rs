// `emulate` subcommand: a hardware-free debug harness for the analog-key
// hysteresis + keymap + SendInput + Hypershift-layer-switch logic
// (`process_key_depths`/`force_keyup_on_hypershift_release` in main.rs).
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
    println!("  hyper        toggle Hypershift, as if the Hyper Response button were pressed");
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
            let now_active = !crate::hypershift::HYPERSHIFT_ACTIVE.load(Ordering::SeqCst);
            crate::hypershift::HYPERSHIFT_ACTIVE.store(now_active, Ordering::SeqCst);
            println!(
                "[emulate] Hyper Response {} -> Hypershift {}",
                if now_active { "pressed" } else { "released" },
                if now_active { "ACTIVE" } else { "inactive" }
            );
            if !now_active {
                crate::force_keyup_on_hypershift_release(pressed_vk, start);
            }
            return true;
        }
        _ => {}
    }

    let Some(idx) = parse_key_index(first) else {
        println!("[emulate] unrecognized command: \"{line}\" (type \"help\" for the list)");
        return true;
    };
    let hypershift = crate::hypershift::HYPERSHIFT_ACTIVE.load(Ordering::SeqCst);

    // Every branch below is "set key idx's depth, then run it through the
    // same processing the real driver loop uses" — a tap is just that pair
    // called twice (255 then 0).
    let mut set_depth = |v: u8| {
        depths[idx] = v;
        crate::process_key_depths(depths, hypershift, pressed_vk, start);
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
    // depth/hyper) in one test, sequentially, rather than splitting into
    // several independent #[test]s: HYPERSHIFT_ACTIVE and CONFIG are
    // process-wide statics shared with the rest of the crate, and cargo test
    // runs tests in parallel by default, so multiple tests touching the same
    // shared mutable state could race against each other. Nothing else in
    // the crate's test suite touches either static, so this one is safe.
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

        // "hyper" toggles HYPERSHIFT_ACTIVE and force-releases held keys on
        // the release edge (Purpose.md §6② step 3).
        crate::hypershift::HYPERSHIFT_ACTIVE.store(false, Ordering::SeqCst);
        assert!(handle_command("hyper", &mut depths, &mut pressed_vk, start));
        assert!(crate::hypershift::HYPERSHIFT_ACTIVE.load(Ordering::SeqCst));
        assert!(handle_command("5 down", &mut depths, &mut pressed_vk, start));
        assert!(pressed_vk[4].is_some());
        assert!(handle_command("hyper", &mut depths, &mut pressed_vk, start));
        assert!(!crate::hypershift::HYPERSHIFT_ACTIVE.load(Ordering::SeqCst));
        assert!(pressed_vk[4].is_none());

        // "quit"/"exit" signal the emulator to stop.
        assert!(!handle_command("quit", &mut depths, &mut pressed_vk, start));
        assert!(!handle_command("exit", &mut depths, &mut pressed_vk, start));
    }
}
