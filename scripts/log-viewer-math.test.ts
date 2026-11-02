// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Standalone unit checks for the Log Viewer's pure helpers (no test runner in the repo —
// run with: npx esbuild scripts/log-viewer-math.test.ts --bundle --platform=node
//   --format=esm --outfile=/tmp/log-viewer-math.test.mjs && node /tmp/log-viewer-math.test.mjs
// Exits non-zero on the first failed assertion group summary.)

import {
  indexAtTime,
  nearestIndex,
  valueAtTime,
  modeAtTime,
  seriesRange,
  niceTicks,
  buildLinePath,
  formatDuration,
  formatValue,
  formatDistance,
  type ViewerModeSegment,
} from '../src/lib/helpers/logViewerMath';

let failures = 0;
function check(name: string, cond: boolean, detail = '') {
  if (!cond) {
    failures++;
    console.error(`FAIL ${name} ${detail}`);
  } else {
    console.log(`ok   ${name}`);
  }
}
function approx(a: number, b: number, eps = 1e-9) {
  return Math.abs(a - b) <= eps;
}

// ── indexAtTime / valueAtTime (step-hold) ──
const times = [0, 1, 2, 4, 8];
const values = [10, 11, 12, 14, 18];
check('index before first', indexAtTime(times, -0.5) === -1);
check('index at first', indexAtTime(times, 0) === 0);
check('index between samples holds previous', indexAtTime(times, 3) === 2);
check('index at last', indexAtTime(times, 8) === 4);
check('index past end clamps to last', indexAtTime(times, 99) === 4);
check('value before first is null', valueAtTime(times, values, -1) === null);
check('value step-hold', valueAtTime(times, values, 3.5) === 12);
check('value at sample', valueAtTime(times, values, 4) === 14);
check('value of empty series is null', valueAtTime([], [], 1) === null);

// ── nearestIndex ──
check('nearest below midpoint', nearestIndex(times, 2.9) === 2);
check('nearest above midpoint', nearestIndex(times, 3.1) === 3);
check('nearest before first is first', nearestIndex(times, -5) === 0);
check('nearest past end is last', nearestIndex(times, 50) === 4);
check('nearest of empty is -1', nearestIndex([], 1) === -1);

// ── modeAtTime ──
const modes: ViewerModeSegment[] = [
  { startS: 0, endS: 5, modeNum: 5, modeId: 'loiter' },
  { startS: 5, endS: 10, modeNum: 11, modeId: 'rtl' },
];
check('mode at start', modeAtTime(modes, 0)?.modeId === 'loiter');
check('mode mid-segment', modeAtTime(modes, 4.9)?.modeId === 'loiter');
check('mode switches at boundary', modeAtTime(modes, 5)?.modeId === 'rtl');
check('mode past end is last segment', modeAtTime(modes, 42)?.modeId === 'rtl');
check('no segments → null', modeAtTime([], 1) === null);

// ── seriesRange ──
const [rMin, rMax] = seriesRange([2, 4, 6]);
check('range covers data with padding', rMin < 2 && rMax > 6, `${rMin}..${rMax}`);
const [fMin, fMax] = seriesRange([5, 5, 5]);
check('flat series range is non-degenerate', fMax > fMin);
check('empty series range is [0,1]', seriesRange([])[0] === 0 && seriesRange([])[1] === 1);

// ── niceTicks ──
const ticks = niceTicks(0, 100, 4);
check('ticks ascending + inside range', ticks.every((v, i) => v >= 0 && v <= 100 && (i === 0 || v > ticks[i - 1])), ticks.join(','));
check('ticks count sane', ticks.length >= 2 && ticks.length <= 8, `n=${ticks.length}`);
const t2 = niceTicks(-3.2, 7.9, 4);
check('ticks handle negative range', t2.length >= 2 && t2[0] >= -3.2 && t2[t2.length - 1] <= 7.9, t2.join(','));
check('degenerate range returns single tick', niceTicks(5, 5).length === 1);

// ── buildLinePath ──
const path = buildLinePath([0, 5, 10], [0, 50, 100], { x: 0, y: 0, w: 100, h: 100 }, 10, 0, 100);
check('path starts at bottom-left', path.startsWith('M0.0,100.0'), path);
check('path ends at top-right', path.endsWith('L100.0,0.0'), path);
check('path of empty series is empty', buildLinePath([], [], { x: 0, y: 0, w: 10, h: 10 }, 10, 0, 1) === '');
check('path with zero duration is empty', buildLinePath([0], [1], { x: 0, y: 0, w: 10, h: 10 }, 0, 0, 1) === '');

// ── formatting ──
check('duration m:ss', formatDuration(65) === '1:05');
check('duration h:mm:ss', formatDuration(3661) === '1:01:01');
check('duration clamps NaN', formatDuration(NaN) === '0:00');
check('value formatting', formatValue(3.14159) === '3.14' && formatValue(123.456) === '123.5' && formatValue(null) === '—');
check('distance formatting', formatDistance(850) === '850 m' && formatDistance(1234) === '1.23 km' && formatDistance(null) === '—');
check('approx helper sanity', approx(1.0000000001, 1.0));

if (failures > 0) {
  console.error(`\n${failures} check(s) FAILED`);
  process.exit(1);
}
console.log('\nAll log-viewer math checks passed.');
