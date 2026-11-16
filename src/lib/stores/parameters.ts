// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Rick Quast

// The connected FC's parameter list, as downloaded for the Parameters panel (rivals batch,
// 2026-11-16). `null` = not downloaded yet this session. The list is a snapshot: it is not
// refreshed live, and it is cleared on disconnect so a stale set is never written to a
// different vehicle.

import { writable } from 'svelte/store';

export interface FcParam {
  name: string;
  value: number;
  /** MAVLink MAV_PARAM_TYPE (1 = int8 … 9 = real32, 10 = real64). */
  paramType: number;
}

export const fcParams = writable<FcParam[] | null>(null);
/** False when the last download hit its deadline with indices missing. */
export const fcParamsComplete = writable(true);

export function clearFcParams(): void {
  fcParams.set(null);
  fcParamsComplete.set(true);
}
