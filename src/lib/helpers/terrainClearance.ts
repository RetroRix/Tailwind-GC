// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// "Hold clearance" terrain rewrite for the mission panel (clean-room).
//
// Pure math over already-sampled terrain: given each navigation waypoint's
// stored altitude + altitude semantics, the terrain samples along its
// incoming leg, and a pilot-set minimum clearance, compute the waypoints
// whose altitude must be RAISED so the leg's highest terrain point keeps at
// least that clearance — converting the required AMSL altitude back into
// each waypoint's own altitude semantics so its mode/frame is preserved.
//
// Altitude semantics (as used by both mission models in this app):
//   'relative' — stored value is height above the home/launch reference
//                (INAV alt_mode REL, ArduPilot frame GLOBAL_RELATIVE_ALT).
//   'amsl'     — stored value is absolute altitude ≈ MSL
//                (INAV alt_mode AMSL, ArduPilot frame GLOBAL).
//   'agl'      — stored value IS the height above terrain, resolved
//                continuously by the FC / at export (INAV alt_mode AGL,
//                ArduPilot frame GLOBAL_TERRAIN_ALT). Holding clearance for
//                such a waypoint means the stored value itself must be at
//                least the clearance.
//
// No-data is never invented: a waypoint whose incoming leg has any missing
// terrain sample is skipped (and reported), never rewritten from partial
// data.

/** Altitude semantics of a stored waypoint altitude value. */
export type AltSemantics = 'relative' | 'amsl' | 'agl';

/** A navigation waypoint, normalised across the INAV / ArduPilot models. */
export interface ClearanceWp {
  /** Index into the firmware mission array (the store edit target). */
  srcIndex: number;
  /** Display number (1-based, as shown in the mission panel). */
  number: number;
  lat: number; // degrees
  lon: number; // degrees
  /** Stored altitude in metres (INAV cm already converted). */
  altM: number;
  mode: AltSemantics;
}

/** One terrain sample along a leg. `elevM === null` = no data (ocean / no tile). */
export interface LegSample {
  /** Distance from the leg start, metres. */
  distM: number;
  elevM: number | null;
}

/** Terrain sampled along the leg that ENDS at a waypoint (its incoming leg). */
export interface IncomingLeg {
  lengthM: number;
  samples: LegSample[];
}

export interface RewriteChange {
  srcIndex: number;
  number: number;
  mode: AltSemantics;
  /** Stored altitude before / after, metres (in the waypoint's own semantics). */
  oldAltM: number;
  newAltM: number;
  /** Resolved AMSL altitude after the rewrite, metres. */
  newAltMsl: number;
  /** Highest terrain elevation on the incoming leg, metres (null for AGL mode). */
  maxTerrainM: number | null;
}

export type SkipReason = 'noData' | 'noRef';

export interface RewriteSkip {
  srcIndex: number;
  number: number;
  reason: SkipReason;
}

export interface RewriteResult {
  changes: RewriteChange[];
  skipped: RewriteSkip[];
  /** Each waypoint's currently planned altitude resolved to AMSL (null = unresolvable). */
  currentMsl: (number | null)[];
}

const EARTH_R = 6371000;
const EPS = 0.005; // m — below this, altitudes count as equal (no rewrite)

/** Great-circle distance between two lat/lon points (degrees), metres. */
export function haversineM(aLat: number, aLon: number, bLat: number, bLon: number): number {
  const toRad = Math.PI / 180;
  const dLat = (bLat - aLat) * toRad;
  const dLon = (bLon - aLon) * toRad;
  const la1 = aLat * toRad;
  const bLatRad = bLat * toRad;
  const h =
    Math.sin(dLat / 2) ** 2 + Math.cos(la1) * Math.cos(bLatRad) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH_R * Math.asin(Math.min(1, Math.sqrt(h)));
}

/**
 * Highest terrain elevation on a leg + whether coverage is complete.
 * `covered` is false when ANY sample (including the endpoints) has no data —
 * a leg with a gap must not drive a rewrite.
 */
export function legMaxTerrain(leg: IncomingLeg): { maxM: number | null; covered: boolean } {
  let maxM: number | null = null;
  let covered = leg.samples.length > 0;
  for (const s of leg.samples) {
    if (s.elevM == null) {
      covered = false;
    } else if (maxM == null || s.elevM > maxM) {
      maxM = s.elevM;
    }
  }
  if (maxM == null) covered = false;
  return { maxM, covered };
}

/**
 * Resolve a waypoint's planned altitude to AMSL.
 * `refElevM` = terrain elevation at the home/launch reference (for 'relative');
 * `groundM` = terrain elevation at the waypoint itself (for 'agl').
 * Returns null when the needed reference is unavailable.
 */
export function resolveAltMsl(
  wp: ClearanceWp,
  refElevM: number | null,
  groundM: number | null,
): number | null {
  if (wp.mode === 'amsl') return wp.altM;
  if (wp.mode === 'relative') return refElevM == null ? null : refElevM + wp.altM;
  return groundM == null ? null : groundM + wp.altM; // agl
}

/** Round UP to centimetre precision so the stored value truly holds clearance. */
function ceilCm(v: number): number {
  return Math.ceil((v - 1e-9) * 100) / 100;
}

/**
 * Compute the raise-only "hold clearance" rewrite.
 *
 * `legs[i]` is the incoming leg of `wps[i]`; `grounds[i]` the terrain
 * elevation at `wps[i]` itself. `refElevM` is the terrain elevation at the
 * home/launch reference used to resolve 'relative' altitudes (the caller
 * falls back to the first waypoint's terrain when home is unknown, and
 * reports that in the UI).
 *
 * Only waypoints that VIOLATE the clearance are raised; none is ever
 * lowered. Waypoints on legs without full terrain coverage are skipped.
 */
export function computeHoldClearance(
  wps: ClearanceWp[],
  legs: IncomingLeg[],
  grounds: (number | null)[],
  refElevM: number | null,
  clearanceM: number,
): RewriteResult {
  const changes: RewriteChange[] = [];
  const skipped: RewriteSkip[] = [];
  const currentMsl: (number | null)[] = wps.map((wp, i) =>
    resolveAltMsl(wp, refElevM, grounds[i] ?? null),
  );

  for (let i = 0; i < wps.length; i++) {
    const wp = wps[i];
    const leg = legs[i];
    if (!leg) continue;
    const { maxM, covered } = legMaxTerrain(leg);
    const ground = grounds[i] ?? null;

    if (!covered || maxM == null) {
      skipped.push({ srcIndex: wp.srcIndex, number: wp.number, reason: 'noData' });
      continue;
    }
    if (wp.mode === 'relative' && refElevM == null) {
      skipped.push({ srcIndex: wp.srcIndex, number: wp.number, reason: 'noRef' });
      continue;
    }
    if (wp.mode === 'agl' && ground == null) {
      skipped.push({ srcIndex: wp.srcIndex, number: wp.number, reason: 'noData' });
      continue;
    }

    if (wp.mode === 'agl') {
      // The stored value IS the clearance above terrain (resolved
      // continuously), so holding clearance = the value itself ≥ clearance.
      if (wp.altM < clearanceM - EPS) {
        changes.push({
          srcIndex: wp.srcIndex,
          number: wp.number,
          mode: wp.mode,
          oldAltM: wp.altM,
          newAltM: ceilCm(clearanceM),
          newAltMsl: (ground as number) + clearanceM,
          maxTerrainM: null,
        });
      }
      continue;
    }

    // 'amsl' / 'relative': the planned altitude (resolved to AMSL) must reach
    // the highest terrain on the incoming leg + clearance.
    const current = currentMsl[i];
    if (current == null) {
      skipped.push({
        srcIndex: wp.srcIndex,
        number: wp.number,
        reason: wp.mode === 'relative' ? 'noRef' : 'noData',
      });
      continue;
    }
    const requiredMsl = maxM + clearanceM;
    if (requiredMsl > current + EPS) {
      const newStored = wp.mode === 'amsl' ? requiredMsl : requiredMsl - (refElevM as number);
      const newAltM = ceilCm(newStored);
      if (newAltM > wp.altM + EPS) {
        changes.push({
          srcIndex: wp.srcIndex,
          number: wp.number,
          mode: wp.mode,
          oldAltM: wp.altM,
          newAltM,
          newAltMsl: wp.mode === 'amsl' ? newAltM : (refElevM as number) + newAltM,
          maxTerrainM: maxM,
        });
      }
    }
  }

  return { changes, skipped, currentMsl };
}
