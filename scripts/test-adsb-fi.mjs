// SPDX-License-Identifier: GPL-3.0-or-later
// Sanity test for the adsb.fi internet traffic feed (radar/sources/adsb_online.rs).
//
// The Rust parser cannot run in this sandbox (no GTK for a cargo build), so this node test mirrors
// its semantics 1:1 against a RECORDED live response (scripts/fixtures/adsb-fi-v2.json, captured
// from https://opendata.adsb.fi/api/v2/lat/31.76/lon/-106.49/dist/25, trimmed to 3 aircraft) and
// checks: the v2 `aircraft[]` shape (NOT `ac[]` — that is the v3/readsb shape, also accepted),
// hex upper-casing for cross-source merge, ft→m / kt→m/s conversions, `seen` staleness backdating,
// the aggregator's dedupe-by-ICAO with FC-source-wins distinction, and the 45 s ADS-B TTL prune.
//
// Run: node scripts/test-adsb-fi.mjs   (exit 0 = pass)

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const fixture = JSON.parse(readFileSync(join(here, 'fixtures', 'adsb-fi-v2.json'), 'utf8'));

let failures = 0;
const check = (name, cond) => {
  console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}`);
  if (!cond) failures++;
};
const approx = (a, b, eps = 0.01) => Math.abs(a - b) <= eps;

// ── Reference port of parse_into (adsb_online.rs) ─────────────────────
const FT_TO_M = 0.3048;
const KT_TO_MS = 0.514444;
const FPM_TO_MS = 0.00508;

function parseInto(root, now) {
  const ac = root.ac ?? root.aircraft;
  if (!Array.isArray(ac)) return [];
  const out = [];
  for (const a of ac) {
    if (typeof a.lat !== 'number' || typeof a.lon !== 'number') continue;
    const hex = (a.hex ?? '').trim().toUpperCase();
    if (!hex) continue;
    const seenMs = typeof a.seen === 'number' ? Math.max(0, a.seen) * 1000 : 0;
    out.push({
      id: hex,
      system: 'adsb',
      sources: ['adsb_online'],
      callsign: typeof a.flight === 'string' && a.flight.trim() ? a.flight.trim() : null,
      lat: a.lat,
      lon: a.lon,
      altM: typeof a.alt_baro === 'number' ? a.alt_baro * FT_TO_M : null,
      headingDeg: typeof a.track === 'number' ? a.track : null,
      groundSpeedMs: typeof a.gs === 'number' ? a.gs * KT_TO_MS : null,
      verticalSpeedMs: typeof a.baro_rate === 'number' ? a.baro_rate * FPM_TO_MS : null,
      category: typeof a.category === 'string' ? a.category : null,
      lastSeenMs: now - seenMs,
    });
  }
  return out;
}

// ── Aggregator semantics (radar/mod.rs): merge by ICAO, sources accumulate ──
function merge(existing, incoming) {
  if (!existing) return incoming;
  const sources = [...existing.sources];
  for (const s of incoming.sources) if (!sources.includes(s)) sources.push(s);
  return { ...incoming, sources, callsign: incoming.callsign ?? existing.callsign };
}
const isOnlineOnly = (v) =>
  v.system === 'adsb' &&
  v.sources.includes('adsb_online') &&
  !v.sources.includes('adsb_receiver') &&
  !v.sources.includes('adsb_msp');
const TTL_ADSB_MS = 45_000;
const prune = (list, now) => list.filter((v) => now - v.lastSeenMs <= TTL_ADSB_MS);

// ── Fixture parse ─────────────────────────────────────────────────────
const NOW = 1_800_000_000_000;
check('fixture is the v2 shape (aircraft[], no ac[])', Array.isArray(fixture.aircraft) && !('ac' in fixture));
const vs = parseInto(fixture, NOW);
check('all 3 recorded aircraft parse', vs.length === 3);

const first = vs[0];
check('hex upper-cased for ICAO merge (a90c5a → A90C5A)', first.id === 'A90C5A');
check('callsign trimmed (N682TM)', first.callsign === 'N682TM');
check('alt_baro ft→m (15650 ft = 4770.12 m)', approx(first.altM, 15650 * FT_TO_M));
check('gs kt→m/s (248.9 kt)', approx(first.groundSpeedMs, 248.9 * KT_TO_MS));
check('track → heading (351.22°)', first.headingDeg === 351.22);
check('seen 0.1 s backdates the stamp by 100 ms', first.lastSeenMs === NOW - 100);
check('seen 1.9 s backdates the stamp by 1900 ms', vs[2].lastSeenMs === NOW - 1900);
check('category carried (A1, as recorded)', first.category === 'A1');

// ── Defensive cases (synthetic) ───────────────────────────────────────
const messy = parseInto(
  {
    ac: [
      { hex: 'abc123', flight: 'GND1', lat: 31.0, lon: -106.0, alt_baro: 'ground' },
      { hex: 'def456', flight: 'NOPOS' },
      { hex: '', lat: 31.0, lon: -106.0 },
    ],
  },
  NOW,
);
check('v3/readsb ac[] shape also parses', messy.length === 1 && messy[0].id === 'ABC123');
check('alt_baro "ground" → no altitude', messy[0].altM === null);
check('entries without position or hex are skipped', messy.length === 1);

// ── Dedupe: FC source wins the distinction ────────────────────────────
const fcTwin = { ...first, sources: ['adsb_receiver'], lat: first.lat + 0.001 };
const merged = merge(fcTwin, first);
check('same ICAO merges into one contact with both sources',
  JSON.stringify(merged.sources) === JSON.stringify(['adsb_receiver', 'adsb_online']));
check('FC-heard contact is NOT internet-only', !isOnlineOnly(merged));
check('online-only contact IS internet-only', isOnlineOnly(first));

// ── Staleness: 45 s ADS-B TTL (aggregator ttl_ms) ─────────────────────
const stale = { ...first, lastSeenMs: NOW - 46_000 };
const fresh = { ...first, lastSeenMs: NOW - 44_000 };
check('contact unseen for 46 s is pruned', prune([stale], NOW).length === 0);
check('contact unseen for 44 s survives', prune([fresh], NOW).length === 1);
check('a feed contact with seen=50 s would be pruned on arrival',
  prune(parseInto({ aircraft: [{ hex: 'aaaaaa', lat: 31, lon: -106, seen: 50 }] }, NOW), NOW).length === 0);

console.log(failures === 0 ? '\nAll adsb.fi parser/merge checks passed.' : `\n${failures} check(s) FAILED`);
process.exit(failures === 0 ? 0 : 1);
