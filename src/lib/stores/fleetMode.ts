// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Multi-vehicle ("fleet") feature gate — the backend's `--fleet` / `KITE_FLEET` launch flag
// (`fleet_mode.rs`; Dev-Docs active/GROUP_FLIGHTS.md §5e: no setting, a launch flag, default off in every
// build). Off: the single-vehicle UI — no link manager, Fleet tab, group commands, fleet map layers or
// per-vehicle STATUSTEXT tags. Loaded once at startup (`loadFleetMode`); the Debug Monitor can switch it
// for the running session (`setFleetMode`, debug mode only).

import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export const fleetEnabled = writable<boolean>(false);

/** Read the launch flag from the backend (startup). */
export async function loadFleetMode(): Promise<void> {
  try {
    fleetEnabled.set(await invoke<boolean>('is_fleet_mode'));
  } catch (e) {
    console.warn('[fleet] is_fleet_mode failed:', e);
  }
}

/** Debug Monitor: switch the gate for this session (the backend refuses outside debug mode). */
export async function setFleetMode(on: boolean): Promise<void> {
  await invoke('set_fleet_enabled', { on });
  fleetEnabled.set(on);
}
