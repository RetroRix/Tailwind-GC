// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

//! Multi-vehicle ("fleet") feature gate — a launch flag, modelled on `debug_mode`.
//!
//! Off, Kite is the single-vehicle GCS: a second system id on a link never becomes a vehicle (no
//! discovery, no extra recorder, no fleet markers or STATUSTEXT tags), the group-flight coordinator
//! never forms a group, and the frontend hides the link manager, Fleet tab and group commands
//! (Dev-Docs active/GROUP_FLIGHTS.md §5e, Marc 2026-10-02: no setting, a launch flag).
//!
//! Default: OFF in every build, debug builds included. Set once at startup from the `--fleet` CLI
//! argument or the `KITE_FLEET` environment variable (any value — so `npx tauri dev` and the portable
//! build can use it too). Mobile has no command line: compiling with `KITE_FLEET` set bakes it in
//! (`KITE_FLEET=1 tauri android build`), like `KITE_DEBUG_UI` for debug mode. The Debug Monitor can
//! flip it at runtime in a debug session (`set_fleet_enabled`).
//!
//! The flag is one shared `AtomicBool`, handed to the group coordinator as an `Arc` (`handle`) so its
//! tests can give each coordinator a flag of its own.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

static ENABLED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(option_env!("KITE_FLEET").is_some())));

/// Switch the fleet features on or off (startup flag; the Debug Monitor's runtime toggle).
pub fn set(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// True when the multi-vehicle features are active. A single relaxed atomic load.
#[inline]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The shared flag itself (the group coordinator reads it on every arm).
pub fn handle() -> Arc<AtomicBool> {
    ENABLED.clone()
}

/// True when the launch asks for the fleet features: `--fleet` on the command line, or the runtime
/// environment variable `KITE_FLEET` set.
pub fn flag_present() -> bool {
    std::env::args().any(|a| a == "--fleet") || std::env::var_os("KITE_FLEET").is_some()
}
