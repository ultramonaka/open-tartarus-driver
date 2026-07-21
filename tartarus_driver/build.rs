// interception-sys statically links an import table entry for
// `interception.dll` (the Interception driver's userland library, bundled
// inside the interception-sys crate as an import lib; the actual DLL file
// at runtime comes from the Interception kernel driver's own installer —
// see README.md "既知の制約" for install steps, NOT from this crate).
//
// Without delay-loading, Windows resolves ALL statically-linked DLL imports
// before this EXE's main() ever runs. If interception.dll is not present
// anywhere on the DLL search path (true on any machine where a human has
// not yet run the Interception installer, including this project's own
// dev/build machine), the OS loader would refuse to start the process at
// all — which would take down Phase 1/2/3 (analog keys, hysteresis,
// Hypershift) along with the D-pad/wheel/middle-click remap, even though
// only the latter actually depends on Interception. That violates this
// subsystem's fail-open design (see the module doc comment above
// run_interception_thread in src/main.rs): everything else must keep
// working when Interception is unavailable.
//
// /DELAYLOAD defers resolving interception.dll until the first actual call
// into it (inside run_interception_thread, on its own background thread),
// at which point our own Rust code — not the OS process loader — is what
// observes and reports the failure.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/DELAYLOAD:interception.dll");
        println!("cargo:rustc-link-arg=delayimp.lib");
    }
}
