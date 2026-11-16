// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Rick Quast

// QGroundControl `.params` file interop (rivals batch, 2026-11-16), clean-room from the format
// as QGC writes it: comment lines start with '#'; each data line is
//   <vehicle-id>\t<component-id>\t<NAME>\t<value>\t<MAV_PARAM_TYPE>
// Fields are tab-separated in files QGC writes; the parser accepts any whitespace run so files
// from other tools (space-separated) load too.

import type { FcParam } from '$lib/stores/parameters';

/** Serialize a parameter set as a QGC `.params` file. Ids are written as 1/1 — Tailwind flies a
 *  single vehicle, and loaders match on the name anyway. */
export function serializeQgcParams(params: FcParam[]): string {
  const lines = [
    '# QGroundControl Parameter File',
    '# Vehicle-Id Component-Id Name Value Type',
  ];
  for (const p of params) {
    const value = p.paramType >= 9 ? formatReal(p.value) : formatInt(p.value);
    lines.push(`1\t1\t${p.name}\t${value}\t${p.paramType}`);
  }
  return lines.join('\n') + '\n';
}

function formatInt(v: number): string {
  return String(Math.round(v));
}

function formatReal(v: number): string {
  // Enough digits to round-trip an f32 without trailing noise.
  return String(Number(v.toPrecision(9)));
}

export interface QgcParseResult {
  params: FcParam[];
  /** Lines that looked like data but could not be parsed (reported, never silently dropped). */
  badLines: number;
}

/** Parse a `.params` file's text. Unknown/extra trailing fields are ignored; a missing type
 *  column defaults to real32 (9), matching what most tools assume. */
export function parseQgcParams(text: string): QgcParseResult {
  const params: FcParam[] = [];
  let badLines = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const fields = line.split(/\s+/);
    if (fields.length < 4) { badLines++; continue; }
    const name = fields[2];
    const value = Number(fields[3]);
    const paramType = fields.length >= 5 ? Number(fields[4]) : 9;
    if (!name || !Number.isFinite(value) || !Number.isFinite(paramType)) { badLines++; continue; }
    params.push({ name, value, paramType });
  }
  return { params, badLines };
}
