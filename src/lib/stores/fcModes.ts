// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// The connected FC's own mode list (MAVLink AVAILABLE_MODES, ArduPilot 4.7+). The backend reads it
// for the link's autopilot and emits `available-modes` once a list is complete (again after the FC's
// mode set changed); a vehicle that never answers simply leaves this null and the panel keeps
// Tailwind's static table.
//
// Tailwind is single-vehicle (upstream keeps one list per vehicle), so there is exactly one list:
// the connected FC's. It is cleared on disconnect.

import { writable } from 'svelte/store';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { FcMode } from '$lib/helpers/mavModes';

/** The connected FC's reported mode list, or null when it has none (yet / ever). */
export const fcModes = writable<readonly FcMode[] | null>(null);

/** Forget the list (disconnect). */
export function clearFcModes(): void {
  fcModes.set(null);
}

let unlistener: UnlistenFn | null = null;
let starting: Promise<void> | null = null;

/** Subscribe to the backend's `available-modes` event. Idempotent, kept for the app's lifetime;
 *  call before connecting so a list completed during the connect burst is not missed. */
export function startFcModeListeners(): Promise<void> {
  if (unlistener) return Promise.resolve();
  if (!starting) {
    starting = (async () => {
      unlistener = await listen<{ modes: FcMode[] }>('available-modes', (e) => {
        const modes = e.payload?.modes;
        if (!Array.isArray(modes) || modes.length === 0) return;
        fcModes.set(modes);
      });
    })().finally(() => { starting = null; });
  }
  return starting;
}
