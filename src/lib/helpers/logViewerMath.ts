// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Log Viewer — dataset types (mirrors flightlog::viewer::ViewerDataset, serde camelCase)
// and the pure math the viewer needs: time lookups, axis ticks, SVG path building and
// formatting. Everything here is side-effect free and unit-tested by
// scripts/log-viewer-math.test.ts. The viewer is a pure replay state — nothing here (or in
// the component) touches the live telemetry stores.

export interface ViewerField {
  name: string;
  unit: string | null;
  timesS: number[];
  values: number[];
}

export interface ViewerType {
  name: string;
  baseName: string;
  instance: number | null;
  count: number;
  fields: ViewerField[];
}

export interface ViewerTrackPoint {
  tS: number;
  lat: number;
  lon: number;
  altM: number | null;
  modeId: string | null;
}

export interface ViewerModeSegment {
  startS: number;
  endS: number;
  modeNum: number;
  modeId: string;
}

export interface ViewerDataset {
  sourceFormat: 'dataflash' | 'tlog';
  fileName: string;
  fileSizeBytes: number;
  vehicleVariant: string | null;
  durationS: number;
  types: ViewerType[];
  track: ViewerTrackPoint[];
  modes: ViewerModeSegment[];
  summary: {
    messageCount: number;
    gpsPointCount: number;
    maxAltM: number | null;
    distanceM: number | null;
  };
  truncated: boolean;
}

/** Stable selection key for one field of one message type. */
export function fieldKey(typeName: string, fieldName: string): string {
  return `${typeName}::${fieldName}`;
}

/** Largest index i with times[i] <= t, or -1 when t is before the first sample. */
export function indexAtTime(times: number[], t: number): number {
  let lo = 0;
  let hi = times.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] <= t) {
      ans = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return ans;
}

/** Index of the sample closest to t (for the map marker — position should track the
 *  playhead, not lag to the previous GPS tick). -1 for an empty series. */
export function nearestIndex(times: number[], t: number): number {
  if (times.length === 0) return -1;
  const i = indexAtTime(times, t);
  if (i < 0) return 0;
  if (i >= times.length - 1) return times.length - 1;
  return t - times[i] <= times[i + 1] - t ? i : i + 1;
}

/** Step-hold value of a series at time t (last sample at or before t); null before the
 *  first sample or for an empty series. Log data is sampled, not continuous — no
 *  interpolation is invented between samples. */
export function valueAtTime(times: number[], values: number[], t: number): number | null {
  const i = indexAtTime(times, t);
  return i < 0 ? null : values[i];
}

/** The mode segment in force at t, or null when the log has no mode data there. */
export function modeAtTime(segments: ViewerModeSegment[], t: number): ViewerModeSegment | null {
  let current: ViewerModeSegment | null = null;
  for (const seg of segments) {
    if (seg.startS <= t) current = seg;
    else break;
  }
  return current;
}

/** [min, max] of a series, expanded when degenerate (flat line) so a chart can draw it. */
export function seriesRange(values: number[]): [number, number] {
  if (values.length === 0) return [0, 1];
  let min = Infinity;
  let max = -Infinity;
  for (const v of values) {
    if (v < min) min = v;
    if (v > max) max = v;
  }
  if (min === max) {
    const pad = Math.abs(min) > 0 ? Math.abs(min) * 0.05 : 1;
    return [min - pad, max + pad];
  }
  const pad = (max - min) * 0.05;
  return [min - pad, max + pad];
}

/** "Nice" axis ticks (1/2/5 × 10^n steps) covering [min, max] with about `count` ticks. */
export function niceTicks(min: number, max: number, count = 4): number[] {
  if (!(max > min) || count < 1) return [min];
  const span = max - min;
  const rawStep = span / count;
  const mag = Math.pow(10, Math.floor(Math.log10(rawStep)));
  const norm = rawStep / mag;
  const step = (norm >= 7.5 ? 10 : norm >= 3.5 ? 5 : norm >= 1.5 ? 2 : 1) * mag;
  const ticks: number[] = [];
  const start = Math.ceil(min / step) * step;
  for (let v = start; v <= max + 1e-9 && ticks.length < 12; v += step) {
    ticks.push(Math.abs(v) < step * 1e-9 ? 0 : v);
  }
  return ticks.length > 0 ? ticks : [min, max];
}

/** Build an SVG polyline path for a series inside a plot box. Each series is scaled to
 *  its own [vMin, vMax] (multi-unit overlay — see the viewer's scale hint). */
export function buildLinePath(
  times: number[],
  values: number[],
  plot: { x: number; y: number; w: number; h: number },
  tMax: number,
  vMin: number,
  vMax: number,
): string {
  const n = Math.min(times.length, values.length);
  if (n === 0 || tMax <= 0 || vMax <= vMin) return '';
  let d = '';
  for (let i = 0; i < n; i++) {
    const x = plot.x + (times[i] / tMax) * plot.w;
    const y = plot.y + plot.h - ((values[i] - vMin) / (vMax - vMin)) * plot.h;
    d += `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
  }
  return d;
}

/** Seconds → "m:ss" under an hour, "h:mm:ss" above. Negative/NaN clamp to "0:00". */
export function formatDuration(totalS: number): string {
  if (!Number.isFinite(totalS) || totalS < 0) return '0:00';
  const s = Math.floor(totalS);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const ss = String(sec).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** Compact numeric readout: magnitude-aware decimals, em dash for missing. */
export function formatValue(v: number | null | undefined): string {
  if (v === null || v === undefined || !Number.isFinite(v)) return '—';
  const a = Math.abs(v);
  if (a !== 0 && (a >= 10000 || a < 0.01)) return v.toExponential(2);
  if (a >= 100) return v.toFixed(1);
  if (a >= 1) return v.toFixed(2);
  return v.toFixed(3);
}

/** Metres → "850 m" / "1.23 km"; null → em dash. */
export function formatDistance(m: number | null | undefined): string {
  if (m === null || m === undefined || !Number.isFinite(m)) return '—';
  return m >= 1000 ? `${(m / 1000).toFixed(2)} km` : `${m.toFixed(0)} m`;
}

/** Bytes → human size ("412 B", "3.4 MB"). */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
