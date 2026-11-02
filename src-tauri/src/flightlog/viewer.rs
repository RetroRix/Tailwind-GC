// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Flight-log viewer dataset builder (Log Viewer tab).
//
// Clean-room implementation against this app's own parsers — the *idea* (graph any logged
// field + map track + mode timeline, like ArduDeck / QGC / Mission Planner log browsers) is
// implemented on top of the existing DataFlash scanner (`ardupilot.rs`) and the app's MAVLink
// parser (`mavlink_proto::parser`). No rival code is copied.
//
//   - `.bin`  (ArduPilot DataFlash): GENERIC decode. Every FMT-registered message type is
//     exposed with its FMT column names; FMTU records add per-field units + multipliers.
//     Numeric fields become time series; string fields are not graphed and are omitted.
//   - `.tlog` (MAVLink telemetry):  CURATED decode. The common flight messages
//     (GLOBAL_POSITION_INT, GPS_RAW_INT, ATTITUDE, SYS_STATUS, VFR_HUD) become series, and
//     HEARTBEAT drives the mode timeline. Other message types are counted, not decoded.
//
// Size discipline (a Cube .bin can exceed 100 MB):
//   - Files over MAX_FILE_BYTES are rejected with a clear error instead of being truncated.
//   - Each field is stream-compacted while collecting (halved whenever it passes
//     FIELD_RAW_CAP points), then decimated to an adaptive per-field cap so the whole
//     dataset stays under TOTAL_POINT_BUDGET points for the JSON bridge. First and last
//     points are always preserved by the final decimation.
//   - Multi-instance messages (GPS I=0/I=1, BAT Instance=…, GPS vs GPS2) are kept as
//     separate series ("GPS", "GPS[1]", "GPS2") — instances are never merged silently.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use ::mavlink::ardupilotmega::{GpsFixType, MavAutopilot, MavMessage, MavType};

use super::ardupilot::{DataFlashScanner, ParsedMsg};
use crate::mavlink_proto::parser::MavParser;

// ─── Limits ──────────────────────────────────────────────────────────────────

/// Hard reject above this size: the whole file is read into memory and the result crosses
/// the Tauri JSON bridge, so an unbounded file could OOM the app. 512 MiB is far above any
/// real Cube log Rick flies; the error tells the user instead of silently truncating.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
/// Per-field cap after final decimation (lowered adaptively when a log has many fields).
const FIELD_POINT_CAP: usize = 4000;
/// Per-field raw collection cap; beyond it the buffer is halved in place (stream compaction).
const FIELD_RAW_CAP: usize = 8192;
/// Total points (all fields + track) the returned dataset may contain.
const TOTAL_POINT_BUDGET: usize = 400_000;
/// Raw points collected across all fields before collection stops (counts keep going).
const GLOBAL_RAW_BUDGET: usize = 2_000_000;
/// GPS track cap after decimation.
const TRACK_POINT_CAP: usize = 20_000;

// ─── Dataset (serialized to the frontend) ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerDataset {
    /// "dataflash" | "tlog"
    pub source_format: String,
    pub file_name: String,
    pub file_size_bytes: u64,
    /// Vehicle variant string used for the mode table ("ArduPlane", "ArduCopter", "PX4", …).
    pub vehicle_variant: Option<String>,
    /// Flight-log span in seconds (first → last timestamped message).
    pub duration_s: f64,
    pub types: Vec<ViewerType>,
    pub track: Vec<ViewerTrackPoint>,
    pub modes: Vec<ViewerModeSegment>,
    pub summary: ViewerSummary,
    /// True when any series/track was decimated or collection hit a budget cap.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerType {
    /// Display name; multi-instance types carry an "[instance]" suffix (e.g. "GPS[1]").
    pub name: String,
    /// Base FMT / MAVLink message name without the instance suffix.
    pub base_name: String,
    /// Instance label when the message format carries an instance/index field.
    pub instance: Option<i64>,
    /// Total messages of this type+instance in the file (before decimation).
    pub count: usize,
    pub fields: Vec<ViewerField>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerField {
    pub name: String,
    /// SI unit from FMTU (DataFlash) or the curated .tlog table; None when unknown.
    pub unit: Option<String>,
    /// Seconds relative to the first timestamped message in the file.
    pub times_s: Vec<f64>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerTrackPoint {
    pub t_s: f64,
    pub lat: f64,
    pub lon: f64,
    pub alt_m: Option<f64>,
    /// Canonical flight-mode id (frontend registry resolves label + colour).
    pub mode_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerModeSegment {
    pub start_s: f64,
    pub end_s: f64,
    pub mode_num: u32,
    /// Canonical flight-mode id from `flightmode::classify_mavlink`.
    pub mode_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerSummary {
    /// Total messages decoded (all types, before decimation).
    pub message_count: usize,
    /// GPS track points before decimation.
    pub gps_point_count: usize,
    pub max_alt_m: Option<f64>,
    /// Track length in metres (haversine sum over the raw, undecimated track).
    pub distance_m: Option<f64>,
}

// ─── Series collection (shared by both parsers) ──────────────────────────────

#[derive(Default)]
struct FieldAcc {
    name: String,
    /// FMT column index (DataFlash unit/multiplier lookup); unused for .tlog.
    col_index: usize,
    unit: Option<String>,
    /// Raw microseconds (DataFlash TimeUS / tlog container µs / boot µs); relativised later.
    times_us: Vec<f64>,
    values: Vec<f64>,
}

#[derive(Default)]
struct TypeAcc {
    base_name: String,
    instance: Option<i64>,
    count: usize,
    fields: Vec<FieldAcc>,
}

#[derive(Default)]
struct SeriesBuilder {
    types: Vec<TypeAcc>,
    index: HashMap<String, usize>,
    total_points: usize,
    truncated: bool,
}

impl SeriesBuilder {
    fn type_idx(&mut self, base_name: &str, instance: Option<i64>) -> usize {
        let key = match instance {
            Some(i) => format!("{base_name}\u{1}{i}"),
            None => base_name.to_string(),
        };
        if let Some(&idx) = self.index.get(&key) {
            return idx;
        }
        let idx = self.types.len();
        self.types.push(TypeAcc {
            base_name: base_name.to_string(),
            instance,
            count: 0,
            fields: Vec::new(),
        });
        self.index.insert(key, idx);
        idx
    }

    fn count(&mut self, base_name: &str, instance: Option<i64>) {
        let idx = self.type_idx(base_name, instance);
        self.types[idx].count += 1;
    }

    /// Append one numeric point. Non-finite values are dropped (a NaN would break the graph
    /// and JSON cannot represent it). Collection past GLOBAL_RAW_BUDGET stops silently
    /// collecting (message counts continue) and flags the dataset truncated.
    fn push(
        &mut self,
        type_idx: usize,
        col_index: usize,
        field_name: &str,
        unit: Option<String>,
        t_us: f64,
        value: f64,
    ) {
        if !value.is_finite() {
            return;
        }
        if self.total_points >= GLOBAL_RAW_BUDGET {
            self.truncated = true;
            return;
        }
        let ty = &mut self.types[type_idx];
        let fidx = match ty
            .fields
            .iter()
            .position(|f| f.col_index == col_index && f.name == field_name)
        {
            Some(i) => i,
            None => {
                ty.fields.push(FieldAcc {
                    name: field_name.to_string(),
                    col_index,
                    unit,
                    times_us: Vec::new(),
                    values: Vec::new(),
                });
                ty.fields.len() - 1
            }
        };
        let field = &mut ty.fields[fidx];
        field.times_us.push(t_us);
        field.values.push(value);
        self.total_points += 1;
        if field.times_us.len() > FIELD_RAW_CAP {
            compact_half(&mut field.times_us, &mut field.values);
            self.truncated = true;
        }
    }
}

/// Drop every second point (keeps index 0). Used for stream compaction of high-rate fields.
fn compact_half(times: &mut Vec<f64>, values: &mut Vec<f64>) {
    let mut i = 0usize;
    times.retain(|_| {
        let keep = i % 2 == 0;
        i += 1;
        keep
    });
    let mut j = 0usize;
    values.retain(|_| {
        let keep = j % 2 == 0;
        j += 1;
        keep
    });
}

/// Decimate parallel (times, values) vectors to at most `cap` points by stride, always
/// preserving the first and last point. Returns true when points were dropped.
pub fn decimate_pairs(times: &[f64], values: &[f64], cap: usize) -> (Vec<f64>, Vec<f64>, bool) {
    let n = times.len().min(values.len());
    if cap == 0 || n == 0 {
        return (Vec::new(), Vec::new(), n > 0);
    }
    if n <= cap {
        return (times[..n].to_vec(), values[..n].to_vec(), false);
    }
    let stride = (n as f64 / cap as f64).ceil() as usize;
    let mut out_t = Vec::with_capacity(cap + 1);
    let mut out_v = Vec::with_capacity(cap + 1);
    let mut i = 0usize;
    while i < n {
        out_t.push(times[i]);
        out_v.push(values[i]);
        i += stride;
    }
    if (n - 1) % stride != 0 {
        out_t.push(times[n - 1]);
        out_v.push(values[n - 1]);
    }
    (out_t, out_v, true)
}

/// Haversine distance in metres (same formula as the DataFlash importer's).
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().asin()
}

fn is_valid_gps_coord(lat: f64, lon: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-180.0..=180.0).contains(&lon)
        && !(lat == 0.0 && lon == 0.0)
}

// ─── Mode segments ───────────────────────────────────────────────────────────

/// Build mode segments from (t_s, mode_num) change events in file order. Consecutive
/// repeats merge; the last segment runs to `duration_s`.
fn build_mode_segments(
    events: &[(f64, u32)],
    duration_s: f64,
    variant: &str,
) -> Vec<ViewerModeSegment> {
    let mut segments: Vec<ViewerModeSegment> = Vec::new();
    for &(t_s, num) in events {
        if let Some(last) = segments.last_mut() {
            if last.mode_num == num {
                continue;
            }
            last.end_s = t_s;
        }
        segments.push(ViewerModeSegment {
            start_s: t_s,
            end_s: duration_s,
            mode_num: num,
            mode_id: crate::flightmode::classify_mavlink(num, variant).primary,
        });
    }
    segments
}

/// Canonical mode id in force at `t_s` (segments are start-ordered), if any.
fn mode_id_at(segments: &[ViewerModeSegment], t_s: f64) -> Option<String> {
    let mut current: Option<&ViewerModeSegment> = None;
    for seg in segments {
        if seg.start_s <= t_s {
            current = Some(seg);
        } else {
            break;
        }
    }
    current.map(|s| s.mode_id.clone())
}

// ─── Track assembly ──────────────────────────────────────────────────────────

struct TrackRaw {
    t_us: f64,
    lat: f64,
    lon: f64,
    alt_m: Option<f64>,
}

/// Relativise + decimate raw track points, stamp each with the mode in force, and compute
/// the summary stats over the RAW points (distance must not shrink with decimation).
fn finish_track(
    raw: Vec<TrackRaw>,
    t0_us: f64,
    segments: &[ViewerModeSegment],
    builder_truncated: bool,
) -> (Vec<ViewerTrackPoint>, Option<f64>, Option<f64>, bool) {
    let mut distance_m = 0.0f64;
    let mut max_alt: Option<f64> = None;
    let mut prev: Option<(f64, f64)> = None;
    for p in &raw {
        if let Some((plat, plon)) = prev {
            distance_m += haversine_m(plat, plon, p.lat, p.lon);
        }
        prev = Some((p.lat, p.lon));
        if let Some(a) = p.alt_m {
            max_alt = Some(max_alt.map_or(a, |c: f64| c.max(a)));
        }
    }

    // Decimate by index (stride over the point list, first + last preserved).
    let times: Vec<f64> = raw.iter().map(|p| (p.t_us - t0_us) / 1e6).collect();
    let indices: Vec<f64> = (0..raw.len()).map(|i| i as f64).collect();
    let (_, kept_idx, was_decimated) = decimate_pairs(&times, &indices, TRACK_POINT_CAP);
    let mut points = Vec::with_capacity(kept_idx.len());
    for ki in kept_idx {
        let p = &raw[ki as usize];
        let t_s = (p.t_us - t0_us) / 1e6;
        points.push(ViewerTrackPoint {
            t_s,
            lat: p.lat,
            lon: p.lon,
            alt_m: p.alt_m,
            mode_id: mode_id_at(segments, t_s),
        });
    }

    (
        points,
        if raw.is_empty() { None } else { max_alt },
        if distance_m > 0.0 {
            Some(distance_m)
        } else {
            None
        },
        was_decimated || builder_truncated,
    )
}

/// Finalise collected series: relativise times, apply an optional per-field scale
/// (DataFlash FMTU multipliers), decimate to the adaptive cap, and drop empty fields/types.
fn finish_types(
    builder: SeriesBuilder,
    t0_us: f64,
    scales: &HashMap<String, Vec<(Option<String>, f64)>>,
) -> (Vec<ViewerType>, bool) {
    let total_fields: usize = builder.types.iter().map(|t| t.fields.len()).sum();
    let field_cap = if total_fields == 0 {
        FIELD_POINT_CAP
    } else {
        (TOTAL_POINT_BUDGET / total_fields.max(1)).clamp(200, FIELD_POINT_CAP)
    };

    // Display names: suffix "[instance]" only when a base name has >1 instance group.
    let mut instance_groups: HashMap<String, usize> = HashMap::new();
    for ty in &builder.types {
        *instance_groups.entry(ty.base_name.clone()).or_insert(0) += 1;
    }

    let mut truncated = builder.truncated;
    let mut out: Vec<ViewerType> = Vec::new();
    for ty in builder.types {
        let mut fields: Vec<ViewerField> = Vec::new();
        for f in ty.fields {
            if f.times_us.is_empty() {
                continue;
            }
            let (unit, scale) = match scales
                .get(&ty.base_name)
                .and_then(|cols| cols.get(f.col_index))
            {
                Some((u, s)) => (u.clone(), *s),
                None => (f.unit.clone(), 1.0),
            };
            let rel_t: Vec<f64> = f.times_us.iter().map(|t| (t - t0_us) / 1e6).collect();
            let scaled: Vec<f64> = if scale != 1.0 {
                f.values.iter().map(|v| v * scale).collect()
            } else {
                f.values
            };
            let (times_s, values, dec) = decimate_pairs(&rel_t, &scaled, field_cap);
            truncated |= dec;
            fields.push(ViewerField {
                name: f.name,
                unit,
                times_s,
                values,
            });
        }
        let multi = instance_groups.get(&ty.base_name).copied().unwrap_or(1) > 1;
        // Instance 0 (the primary) keeps the plain name; other instances are suffixed so
        // the two are never confused — and never merged.
        let name = match (multi, ty.instance) {
            (true, Some(i)) if i != 0 => format!("{}[{}]", ty.base_name, i),
            _ => ty.base_name.clone(),
        };
        out.push(ViewerType {
            name,
            base_name: ty.base_name,
            instance: ty.instance,
            count: ty.count,
            fields,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    (out, truncated)
}

// ─── DataFlash (.bin) ────────────────────────────────────────────────────────

/// FMTU unit-id character → display unit. Conservative subset of the AP_Logger unit table
/// (the documented ones: 's' seconds, 'm' metres, 'd' degrees, 'B'-style multipliers below);
/// unknown ids yield None rather than a fabricated unit.
fn unit_name(id: char) -> Option<String> {
    Some(
        match id {
            's' => "s",
            'm' => "m",
            'd' => "deg",
            'r' => "rad",
            'A' => "A",
            'V' => "V",
            'W' => "W",
            'J' => "J",
            'C' => "°C",
            'g' => "g",
            'G' => "G",
            'P' => "Pa",
            '%' => "%",
            _ => return None,
        }
        .to_string(),
    )
}

/// FMTU multiplier-id character → scale factor. AP_Logger table: digits are powers of ten
/// ('2' = ×100), letters A–G are negative powers ('B' = ×0.01, 'F' = ×1e-6), '-'/'?' = ×1.
fn multiplier(id: char) -> f64 {
    match id {
        '1'..='9' => 10f64.powi(id as i32 - '0' as i32),
        'A'..='G' => 10f64.powi(-(id as i32 - 'A' as i32 + 1)),
        _ => 1.0,
    }
}

/// Whether the existing decoder already scales this format character ('c'/'C' = ×0.01,
/// 'e'/'E' = ×0.01, 'L' = ×1e-7 in `decode_field_value`). Applying the FMTU multiplier on
/// top would double-scale, so for those chars the FMTU multiplier is NOT applied (the unit
/// still is). Plain int/float formats get the multiplier as AP intends.
fn format_self_scales(fmt: char) -> bool {
    matches!(fmt, 'c' | 'C' | 'e' | 'E' | 'L')
}

/// The receiver a `GPS` message comes from is the primary one when `U` = 1; logs without
/// `U` fly on instance 0. Mirrors the importer's `is_primary_gps`.
fn is_primary_gps(msg: &ParsedMsg) -> bool {
    match msg.get_u64("U") {
        Some(used) => used == 1,
        None => msg.get_u64("I").unwrap_or(0) == 0,
    }
}

fn instance_of(msg: &ParsedMsg) -> Option<i64> {
    for key in ["I", "Instance", "Inst"] {
        if let Some(v) = msg.get_f64(key) {
            if v.is_finite() && v.abs() < 1000.0 {
                return Some(v as i64);
            }
        }
    }
    None
}

fn msg_time_us(msg: &ParsedMsg) -> Option<f64> {
    if let Some(us) = msg.get_f64("TimeUS") {
        return Some(us);
    }
    msg.get_f64("TimeMS").map(|ms| ms * 1000.0)
}

pub fn parse_dataflash_viewer(
    data: &[u8],
    file_name: &str,
    file_size: u64,
) -> Result<ViewerDataset, String> {
    let mut scanner = DataFlashScanner::new(data);
    let mut builder = SeriesBuilder::default();
    // FMTU per FMT type-id: (unit-id chars, multiplier-id chars).
    let mut fmtu: HashMap<u8, (Vec<char>, Vec<char>)> = HashMap::new();
    let mut track_primary: Vec<TrackRaw> = Vec::new();
    let mut track_secondary: Vec<TrackRaw> = Vec::new();
    let mut mode_events_raw: Vec<(f64, u32)> = Vec::new();
    let mut variant: Option<String> = None;
    let mut message_count = 0usize;
    let mut t_min: Option<f64> = None;
    let mut t_max: Option<f64> = None;

    while let Some(msg) = scanner.next_message() {
        message_count += 1;
        let t_us = msg_time_us(&msg);
        if let Some(t) = t_us {
            t_min = Some(t_min.map_or(t, |c: f64| c.min(t)));
            t_max = Some(t_max.map_or(t, |c: f64| c.max(t)));
        }

        match msg.type_name.as_str() {
            "FMT" => continue, // schema records are not a graphed series
            "FMTU" => {
                if let (Some(fmt_type), Some(units), Some(mults)) = (
                    msg.get_u64("FmtType"),
                    msg.get_str("UnitIds"),
                    msg.get_str("MultIds"),
                ) {
                    fmtu.insert(
                        fmt_type as u8,
                        (units.chars().collect(), mults.chars().collect()),
                    );
                }
                continue;
            }
            "MSG" => {
                if let Some(text) = msg.get_str("Message") {
                    for prefix in [
                        "ArduPlane",
                        "ArduCopter",
                        "ArduRover",
                        "ArduSub",
                        "ArduBlimp",
                    ] {
                        if text.starts_with(prefix) {
                            variant = Some(prefix.to_string());
                            break;
                        }
                    }
                }
                continue;
            }
            _ => {}
        }

        let instance = instance_of(&msg);
        builder.count(&msg.type_name, instance);
        let tidx = builder.type_idx(&msg.type_name, instance);

        if let Some(t) = t_us {
            for (col_index, (name, value)) in msg.fields.iter().enumerate() {
                if name == "TimeUS" || name == "TimeMS" {
                    continue;
                }
                if let Some(v) = value.as_f64() {
                    builder.push(tidx, col_index, name, None, t, v);
                }
                // String fields (DFValue::Str) have no numeric series — skipped by design.
            }
        }

        // ── GPS track (primary receiver; GPS2 kept apart as a fallback) ──
        if (msg.type_name == "GPS" || msg.type_name == "GPS2") && t_us.is_some() {
            let lat = msg.get_f64("Lat");
            let lon = msg.get_f64("Lng").or_else(|| msg.get_f64("Lon"));
            let fix_ok = msg.get_f64("Status").map(|s| s >= 3.0).unwrap_or(true);
            if let (Some(lat), Some(lon)) = (lat, lon) {
                if fix_ok && is_valid_gps_coord(lat, lon) {
                    let point = TrackRaw {
                        t_us: t_us.unwrap_or(0.0),
                        lat,
                        lon,
                        alt_m: msg.get_f64("Alt"),
                    };
                    if msg.type_name == "GPS" {
                        if is_primary_gps(&msg) {
                            track_primary.push(point);
                        }
                    } else {
                        track_secondary.push(point);
                    }
                }
            }
        }

        // ── Mode timeline ──
        if msg.type_name == "MODE" {
            if let (Some(t), Some(num)) =
                (t_us, msg.get_f64("ModeNum").or_else(|| msg.get_f64("Mode")))
            {
                mode_events_raw.push((t, num as u32));
            }
        }
    }

    if message_count == 0 || scanner.type_registry.is_empty() {
        return Err(
            "No DataFlash messages could be decoded — the file is empty, corrupt, or not an ArduPilot .bin log."
                .to_string(),
        );
    }

    let variant_str = variant.clone().unwrap_or_else(|| "ArduCopter".to_string());
    let t0 = t_min.unwrap_or(0.0);
    let duration_s = match (t_min, t_max) {
        (Some(a), Some(b)) => (b - a) / 1e6,
        _ => 0.0,
    };

    // Per-type FMTU scales: base name → per-column (unit, multiplier).
    let mut scales: HashMap<String, Vec<(Option<String>, f64)>> = HashMap::new();
    for def in scanner.type_registry.values() {
        if let Some((units, mults)) = fmtu.get(&def.type_id) {
            let cols: Vec<(Option<String>, f64)> = def
                .format
                .chars()
                .enumerate()
                .map(|(i, fc)| {
                    let unit = units.get(i).copied().and_then(unit_name);
                    let mult = if format_self_scales(fc) {
                        1.0
                    } else {
                        mults.get(i).copied().map(multiplier).unwrap_or(1.0)
                    };
                    (unit, mult)
                })
                .collect();
            scales.insert(def.name.clone(), cols);
        }
    }

    let mode_events: Vec<(f64, u32)> = mode_events_raw
        .iter()
        .map(|&(t, n)| ((t - t0) / 1e6, n))
        .collect();
    let modes = build_mode_segments(&mode_events, duration_s, &variant_str);

    let track_raw = if !track_primary.is_empty() {
        track_primary
    } else {
        track_secondary
    };
    let gps_point_count = track_raw.len();
    let (track, max_alt_m, distance_m, track_trunc) =
        finish_track(track_raw, t0, &modes, builder.truncated);
    let (types, types_trunc) = finish_types(builder, t0, &scales);

    if types.iter().all(|t| t.fields.is_empty()) && track.is_empty() {
        return Err(
            "The log decoded but contains no numeric time series and no GPS track to display."
                .to_string(),
        );
    }

    Ok(ViewerDataset {
        source_format: "dataflash".to_string(),
        file_name: file_name.to_string(),
        file_size_bytes: file_size,
        vehicle_variant: variant,
        duration_s,
        types,
        track,
        modes,
        summary: ViewerSummary {
            message_count,
            gps_point_count,
            max_alt_m,
            distance_m,
        },
        truncated: track_trunc || types_trunc,
    })
}

// ─── MAVLink .tlog ───────────────────────────────────────────────────────────

/// One curated .tlog series field: (field name, unit, value) — value None = not present /
/// unknown in this message (e.g. SYS_STATUS current = -1), so no point is recorded.
type TlogField = (&'static str, Option<&'static str>, Option<f64>);

fn push_tlog_fields(builder: &mut SeriesBuilder, type_name: &str, t_us: f64, fields: &[TlogField]) {
    builder.count(type_name, None);
    let tidx = builder.type_idx(type_name, None);
    for (i, (name, unit, value)) in fields.iter().enumerate() {
        if let Some(v) = value {
            builder.push(tidx, i, name, unit.map(|u| u.to_string()), t_us, *v);
        }
    }
}

fn valid_lat_lon(lat_e7: i32, lon_e7: i32) -> Option<(f64, f64)> {
    let lat = lat_e7 as f64 / 1e7;
    let lon = lon_e7 as f64 / 1e7;
    if is_valid_gps_coord(lat, lon) {
        Some((lat, lon))
    } else {
        None
    }
}

/// Decode the curated .tlog message set from a decoded frame stream.
/// `frames` = (timestamp µs on the viewer's time base, message).
fn build_tlog_dataset(
    frames: Vec<(f64, MavMessage)>,
    file_name: &str,
    file_size: u64,
) -> Result<ViewerDataset, String> {
    if frames.is_empty() {
        return Err(
            "No MAVLink frames could be decoded — the file is empty, corrupt, or not a .tlog telemetry log."
                .to_string(),
        );
    }

    let mut builder = SeriesBuilder::default();
    let mut track_gpi: Vec<TrackRaw> = Vec::new();
    let mut track_gps: Vec<TrackRaw> = Vec::new();
    let mut mode_events_raw: Vec<(f64, u32)> = Vec::new();
    let mut variant: Option<String> = None;
    let mut message_count = 0usize;
    let mut t_min: Option<f64> = None;
    let mut t_max: Option<f64> = None;

    // HEARTBEATs reaching this function are already locked to the autopilot sender by
    // `lock_autopilot_heartbeats` in the stream parsers below.
    for (t_us, msg) in &frames {
        message_count += 1;
        t_min = Some(t_min.map_or(*t_us, |c: f64| c.min(*t_us)));
        t_max = Some(t_max.map_or(*t_us, |c: f64| c.max(*t_us)));

        match msg {
            MavMessage::HEARTBEAT(hb) => {
                if hb.mavtype != MavType::MAV_TYPE_GCS {
                    let (v, _) = crate::mavlink_proto::vehicle::identify(hb.autopilot, hb.mavtype);
                    if variant.is_none() {
                        variant = Some(v);
                    }
                    mode_events_raw.push((*t_us, hb.custom_mode));
                }
            }
            MavMessage::GLOBAL_POSITION_INT(g) => {
                push_tlog_fields(
                    &mut builder,
                    "GLOBAL_POSITION_INT",
                    *t_us,
                    &[
                        ("lat", Some("deg"), Some(g.lat as f64 / 1e7)),
                        ("lon", Some("deg"), Some(g.lon as f64 / 1e7)),
                        ("alt", Some("m"), Some(g.alt as f64 / 1000.0)),
                        (
                            "relative_alt",
                            Some("m"),
                            Some(g.relative_alt as f64 / 1000.0),
                        ),
                        ("vx", Some("m/s"), Some(g.vx as f64 / 100.0)),
                        ("vy", Some("m/s"), Some(g.vy as f64 / 100.0)),
                        ("vz", Some("m/s"), Some(g.vz as f64 / 100.0)),
                        (
                            "hdg",
                            Some("deg"),
                            if g.hdg == u16::MAX {
                                None
                            } else {
                                Some(g.hdg as f64 / 100.0)
                            },
                        ),
                    ],
                );
                if let Some((lat, lon)) = valid_lat_lon(g.lat, g.lon) {
                    track_gpi.push(TrackRaw {
                        t_us: *t_us,
                        lat,
                        lon,
                        alt_m: Some(g.alt as f64 / 1000.0),
                    });
                }
            }
            MavMessage::GPS_RAW_INT(g) => {
                let fix = match g.fix_type {
                    GpsFixType::GPS_FIX_TYPE_NO_GPS => 0.0,
                    GpsFixType::GPS_FIX_TYPE_NO_FIX => 1.0,
                    GpsFixType::GPS_FIX_TYPE_2D_FIX => 2.0,
                    GpsFixType::GPS_FIX_TYPE_3D_FIX => 3.0,
                    GpsFixType::GPS_FIX_TYPE_DGPS => 4.0,
                    GpsFixType::GPS_FIX_TYPE_RTK_FLOAT => 5.0,
                    GpsFixType::GPS_FIX_TYPE_RTK_FIXED => 6.0,
                    GpsFixType::GPS_FIX_TYPE_STATIC => 7.0,
                    GpsFixType::GPS_FIX_TYPE_PPP => 8.0,
                    _ => 0.0,
                };
                push_tlog_fields(
                    &mut builder,
                    "GPS_RAW_INT",
                    *t_us,
                    &[
                        ("lat", Some("deg"), Some(g.lat as f64 / 1e7)),
                        ("lon", Some("deg"), Some(g.lon as f64 / 1e7)),
                        ("alt", Some("m"), Some(g.alt as f64 / 1000.0)),
                        (
                            "eph",
                            None,
                            if g.eph == u16::MAX {
                                None
                            } else {
                                Some(g.eph as f64 / 100.0)
                            },
                        ),
                        (
                            "epv",
                            None,
                            if g.epv == u16::MAX {
                                None
                            } else {
                                Some(g.epv as f64 / 100.0)
                            },
                        ),
                        (
                            "vel",
                            Some("m/s"),
                            if g.vel == u16::MAX {
                                None
                            } else {
                                Some(g.vel as f64 / 100.0)
                            },
                        ),
                        (
                            "cog",
                            Some("deg"),
                            if g.cog == u16::MAX {
                                None
                            } else {
                                Some(g.cog as f64 / 100.0)
                            },
                        ),
                        ("fix_type", None, Some(fix)),
                        (
                            "satellites_visible",
                            None,
                            Some(g.satellites_visible as f64),
                        ),
                    ],
                );
                if let Some((lat, lon)) = valid_lat_lon(g.lat, g.lon) {
                    track_gps.push(TrackRaw {
                        t_us: *t_us,
                        lat,
                        lon,
                        alt_m: Some(g.alt as f64 / 1000.0),
                    });
                }
            }
            MavMessage::ATTITUDE(a) => {
                push_tlog_fields(
                    &mut builder,
                    "ATTITUDE",
                    *t_us,
                    &[
                        ("roll", Some("deg"), Some(a.roll.to_degrees() as f64)),
                        ("pitch", Some("deg"), Some(a.pitch.to_degrees() as f64)),
                        (
                            "yaw",
                            Some("deg"),
                            Some((a.yaw.to_degrees() as f64).rem_euclid(360.0)),
                        ),
                        (
                            "rollspeed",
                            Some("deg/s"),
                            Some(a.rollspeed.to_degrees() as f64),
                        ),
                        (
                            "pitchspeed",
                            Some("deg/s"),
                            Some(a.pitchspeed.to_degrees() as f64),
                        ),
                        (
                            "yawspeed",
                            Some("deg/s"),
                            Some(a.yawspeed.to_degrees() as f64),
                        ),
                    ],
                );
            }
            MavMessage::SYS_STATUS(s) => {
                push_tlog_fields(
                    &mut builder,
                    "SYS_STATUS",
                    *t_us,
                    &[
                        (
                            "voltage_battery",
                            Some("V"),
                            Some(s.voltage_battery as f64 / 1000.0),
                        ),
                        (
                            "current_battery",
                            Some("A"),
                            if s.current_battery >= 0 {
                                Some(s.current_battery as f64 / 100.0)
                            } else {
                                None
                            },
                        ),
                        (
                            "battery_remaining",
                            Some("%"),
                            if s.battery_remaining >= 0 {
                                Some(s.battery_remaining as f64)
                            } else {
                                None
                            },
                        ),
                    ],
                );
            }
            MavMessage::VFR_HUD(h) => {
                push_tlog_fields(
                    &mut builder,
                    "VFR_HUD",
                    *t_us,
                    &[
                        ("airspeed", Some("m/s"), Some(h.airspeed as f64)),
                        ("groundspeed", Some("m/s"), Some(h.groundspeed as f64)),
                        ("heading", Some("deg"), Some(h.heading as f64)),
                        ("throttle", Some("%"), Some(h.throttle as f64)),
                        ("alt", Some("m"), Some(h.alt as f64)),
                        ("climb", Some("m/s"), Some(h.climb as f64)),
                    ],
                );
            }
            _ => {}
        }
    }

    let variant_str = variant.clone().unwrap_or_else(|| "ArduCopter".to_string());
    let t0 = t_min.unwrap_or(0.0);
    let duration_s = match (t_min, t_max) {
        (Some(a), Some(b)) => (b - a) / 1e6,
        _ => 0.0,
    };
    let mode_events: Vec<(f64, u32)> = mode_events_raw
        .iter()
        .map(|&(t, n)| ((t - t0) / 1e6, n))
        .collect();
    let modes = build_mode_segments(&mode_events, duration_s, &variant_str);

    let track_raw = if !track_gpi.is_empty() {
        track_gpi
    } else {
        track_gps
    };
    let gps_point_count = track_raw.len();
    let (track, max_alt_m, distance_m, track_trunc) =
        finish_track(track_raw, t0, &modes, builder.truncated);
    let (types, types_trunc) = finish_types(builder, t0, &HashMap::new());

    Ok(ViewerDataset {
        source_format: "tlog".to_string(),
        file_name: file_name.to_string(),
        file_size_bytes: file_size,
        vehicle_variant: variant,
        duration_s,
        types,
        track,
        modes,
        summary: ViewerSummary {
            message_count,
            gps_point_count,
            max_alt_m,
            distance_m,
        },
        truncated: track_trunc || types_trunc,
    })
}

/// Filter HEARTBEATs down to the locked autopilot sender (same rule as the raw importer):
/// the first non-GCS heartbeat that identifies an autopilot locks (system, component);
/// heartbeats from other senders are dropped before dataset building so a camera/gimbal
/// heartbeat cannot flap the mode timeline. Non-heartbeat frames pass through untouched.
fn lock_autopilot_heartbeats(frames: Vec<(f64, (u8, u8), MavMessage)>) -> Vec<(f64, MavMessage)> {
    let mut fc_id: Option<(u8, u8)> = None;
    let mut out = Vec::with_capacity(frames.len());
    for (t_us, sender, msg) in frames {
        if let MavMessage::HEARTBEAT(hb) = &msg {
            let is_autopilot = hb.mavtype != MavType::MAV_TYPE_GCS
                && (hb.autopilot != MavAutopilot::MAV_AUTOPILOT_INVALID || sender.1 == 1);
            if fc_id.is_none() && is_autopilot {
                fc_id = Some(sender);
            }
            if fc_id != Some(sender) {
                continue;
            }
        }
        out.push((t_us, msg));
    }
    out
}

/// Parse the standard .tlog container: repeated [u64 BE epoch µs][one MAVLink frame].
/// A container timestamp of 0 (some writers) falls back to the message's own boot time.
fn parse_tlog_container(data: &[u8]) -> Vec<(f64, (u8, u8), MavMessage)> {
    let mut parser = MavParser::new();
    let mut frames = Vec::new();
    let mut last_boot_us: f64 = 0.0;
    let mut i = 0usize;

    while i + 8 <= data.len() {
        let ts_us = u64::from_be_bytes(data[i..i + 8].try_into().unwrap());
        i += 8;
        let mut got = false;
        let mut guard = 0;
        while i < data.len() && !got && guard < 600 {
            let b = data[i];
            i += 1;
            guard += 1;
            if let Some(frame) = parser.push(b) {
                got = true;
                // Track boot time as the fallback clock for zero container timestamps.
                match &frame.message {
                    MavMessage::GLOBAL_POSITION_INT(g) => {
                        last_boot_us = g.time_boot_ms as f64 * 1000.0
                    }
                    MavMessage::ATTITUDE(a) => last_boot_us = a.time_boot_ms as f64 * 1000.0,
                    MavMessage::GPS_RAW_INT(g) => {
                        last_boot_us = (g.time_usec / 1000) as f64 * 1000.0
                    }
                    _ => {}
                }
                let t = if ts_us > 0 {
                    ts_us as f64
                } else {
                    last_boot_us
                };
                frames.push((
                    t,
                    (frame.header.system_id, frame.header.component_id),
                    frame.message,
                ));
            }
        }
        if !got {
            break;
        }
    }
    frames
}

/// Fallback for a raw MAVLink stream saved as .tlog (no container timestamps): feed the
/// whole file to the parser and use each message's boot time as the clock.
fn parse_tlog_raw_stream(data: &[u8]) -> Vec<(f64, (u8, u8), MavMessage)> {
    let mut parser = MavParser::new();
    let mut frames = Vec::new();
    let mut last_boot_us: f64 = 0.0;
    for frame in parser.parse_bytes(data) {
        match &frame.message {
            MavMessage::GLOBAL_POSITION_INT(g) => last_boot_us = g.time_boot_ms as f64 * 1000.0,
            MavMessage::ATTITUDE(a) => last_boot_us = a.time_boot_ms as f64 * 1000.0,
            MavMessage::GPS_RAW_INT(g) => last_boot_us = (g.time_usec / 1000) as f64 * 1000.0,
            _ => {}
        }
        frames.push((
            last_boot_us,
            (frame.header.system_id, frame.header.component_id),
            frame.message,
        ));
    }
    frames
}

pub fn parse_tlog_viewer(
    data: &[u8],
    file_name: &str,
    file_size: u64,
) -> Result<ViewerDataset, String> {
    let container = parse_tlog_container(data);
    let frames = if !container.is_empty() {
        lock_autopilot_heartbeats(container)
    } else {
        lock_autopilot_heartbeats(parse_tlog_raw_stream(data))
    };
    build_tlog_dataset(frames, file_name, file_size)
}

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Open a `.bin` (DataFlash) or `.tlog` (MAVLink) file and build the viewer dataset.
/// Dispatch is by extension; corrupt/unsupported content returns a clear error string.
pub fn open_log_viewer(path: &Path) -> Result<ViewerDataset, String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "File is too large for the log viewer ({} MB; limit {} MB). Import it into the logbook instead, or trim the log first.",
            meta.len() / (1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024),
        ));
    }
    if meta.len() == 0 {
        return Err(format!("File '{}' is empty.", path.display()));
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let data = std::fs::read(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

    match ext.as_str() {
        "bin" => parse_dataflash_viewer(&data, &file_name, meta.len()),
        "tlog" => parse_tlog_viewer(&data, &file_name, meta.len()),
        _ => Err(format!(
            "Unsupported log type '.{ext}' — the log viewer opens ArduPilot .bin and MAVLink .tlog files."
        )),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────
//
// Rust cannot be compiled on the dev VM, so these run first in CI. The DataFlash fixture
// is built byte-by-byte (FMT + FMTU + GPS/MODE rows); the .tlog fixture uses the app's own
// MAVLink serializer, the same approach as raw_import's tests.

#[cfg(test)]
mod tests {
    use super::*;

    // ── DataFlash fixture helpers ──

    fn push_str_padded(out: &mut Vec<u8>, s: &str, width: usize) {
        let bytes = s.as_bytes();
        out.extend_from_slice(&bytes[..bytes.len().min(width)]);
        for _ in bytes.len().min(width)..width {
            out.push(0);
        }
    }

    /// One FMT record (type 128, 89 bytes total).
    fn fmt_record(type_id: u8, record_len: u8, name: &str, format: &str, labels: &str) -> Vec<u8> {
        let mut out = vec![0xA3, 0x95, 128, type_id, record_len];
        push_str_padded(&mut out, name, 4);
        push_str_padded(&mut out, format, 16);
        push_str_padded(&mut out, labels, 64);
        assert_eq!(out.len(), 89);
        out
    }

    fn record(type_id: u8, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0xA3, 0x95, type_id];
        out.extend_from_slice(data);
        out
    }

    /// Minimal valid DataFlash log:
    ///   GPS  (id 202) "QBHBLLff"  TimeUS,I,Status,NSats,Lat,Lng,Alt,Spd — two instances
    ///   MODE (id 203) "QBB"       TimeUS,Mode,ModeNum
    ///   FMTU (id 129) "QBNN"      TimeUS,FmtType,UnitIds,MultIds — units for GPS
    fn fixture_bin() -> Vec<u8> {
        let mut out = Vec::new();
        // FMT for FMTU itself, then FMTU's target types.
        out.extend(fmt_record(
            129,
            44,
            "FMTU",
            "QBNN",
            "TimeUS,FmtType,UnitIds,MultIds",
        ));
        out.extend(fmt_record(
            202,
            31,
            "GPS",
            "QBHBLLff",
            "TimeUS,I,Status,NSats,Lat,Lng,Alt,Spd",
        ));
        out.extend(fmt_record(203, 13, "MODE", "QBB", "TimeUS,Mode,ModeNum"));

        // FMTU for GPS (202): TimeUS unit s ×1e-6 ('F'), Lat/Lng deg, Alt m, Spd none.
        // (TimeUS matches the first GPS row so the fixture's time base starts there.)
        let mut fmtu = Vec::new();
        fmtu.extend_from_slice(&1_000_000u64.to_le_bytes());
        fmtu.push(202u8);
        push_str_padded(&mut fmtu, "s---ddm-", 16);
        push_str_padded(&mut fmtu, "F-------", 16);
        out.extend(record(129, &fmtu));

        // GPS rows: (TimeUS, I, Status, NSats, Lat, Lng, Alt, Spd)
        let gps_row = |t: u64, inst: u8, lat_e7: i32, alt: f32| -> Vec<u8> {
            let mut d = Vec::new();
            d.extend_from_slice(&t.to_le_bytes());
            d.push(inst);
            d.extend_from_slice(&3u16.to_le_bytes()); // Status = 3D fix (H)
            d.push(11u8); // NSats
            d.extend_from_slice(&lat_e7.to_le_bytes());
            d.extend_from_slice(&8_500_000i32.to_le_bytes()); // Lng 0.85°
            d.extend_from_slice(&alt.to_le_bytes());
            d.extend_from_slice(&5.0f32.to_le_bytes()); // Spd
            d
        };
        out.extend(record(202, &gps_row(1_000_000, 0, 477_000_000, 100.0)));
        out.extend(record(202, &gps_row(2_000_000, 1, 477_001_000, 101.0)));
        // MODE at 1.5 s → LOITER (copter mode 5).
        let mut mode = Vec::new();
        mode.extend_from_slice(&1_500_000u64.to_le_bytes());
        mode.push(5u8);
        mode.push(5u8);
        out.extend(record(203, &mode));
        out.extend(record(202, &gps_row(3_000_000, 0, 477_002_000, 102.0)));
        out
    }

    #[test]
    fn dataflash_fixture_decodes_types_track_modes_and_units() {
        let ds =
            parse_dataflash_viewer(&fixture_bin(), "fixture.bin", 0).expect("fixture must decode");
        assert_eq!(ds.source_format, "dataflash");
        assert!(
            (ds.duration_s - 2.0).abs() < 1e-9,
            "duration {}",
            ds.duration_s
        );

        // Instances stay separate: "GPS" (I=0) and "GPS[1]" (I=1).
        let gps = ds
            .types
            .iter()
            .find(|t| t.name == "GPS")
            .expect("GPS series");
        let gps1 = ds
            .types
            .iter()
            .find(|t| t.name == "GPS[1]")
            .expect("GPS[1] series");
        assert_eq!(gps.count, 2);
        assert_eq!(gps1.count, 1);

        // FMTU: Alt carries the metre unit; Lat decodes via the 'L' format char to degrees.
        let alt = gps
            .fields
            .iter()
            .find(|f| f.name == "Alt")
            .expect("Alt field");
        assert_eq!(alt.unit.as_deref(), Some("m"));
        assert_eq!(alt.values, vec![100.0, 102.0]);
        assert_eq!(alt.times_s, vec![0.0, 2.0]);
        let lat = gps
            .fields
            .iter()
            .find(|f| f.name == "Lat")
            .expect("Lat field");
        assert!((lat.values[0] - 47.7).abs() < 1e-7, "lat {}", lat.values[0]);

        // Track = primary receiver (I=0) only: 2 points, mode stamped from the MODE row.
        assert_eq!(ds.track.len(), 2);
        assert_eq!(ds.summary.gps_point_count, 2);
        assert_eq!(ds.track[0].mode_id.as_deref(), None);
        assert_eq!(ds.track[1].mode_id.as_deref(), Some("loiter"));
        let dist = ds.summary.distance_m.expect("distance");
        // 47.7000° → 47.7002° latitude ≈ 22 m.
        assert!(dist > 20.0 && dist < 25.0, "distance {dist}");

        // Mode timeline: one segment, copter mode 5 = loiter, 0.5 s → 2 s.
        assert_eq!(ds.modes.len(), 1);
        assert_eq!(ds.modes[0].mode_num, 5);
        assert_eq!(ds.modes[0].mode_id, "loiter");
        assert!((ds.modes[0].start_s - 0.5).abs() < 1e-9);
        assert!((ds.modes[0].end_s - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dataflash_garbage_is_an_error_not_a_panic() {
        let junk = vec![0x42u8; 4096];
        assert!(parse_dataflash_viewer(&junk, "junk.bin", junk.len() as u64).is_err());
        assert!(parse_dataflash_viewer(&[], "empty.bin", 0).is_err());
    }

    #[test]
    fn decimation_preserves_first_and_last_and_caps() {
        let times: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
        let values = times.clone();
        let (t, v, dropped) = decimate_pairs(&times, &values, 100);
        assert!(dropped);
        assert!(t.len() <= 101, "len {}", t.len());
        assert_eq!(t.first(), Some(&0.0));
        assert_eq!(t.last(), Some(&9999.0));
        assert_eq!(v.last(), Some(&9999.0));

        let (t2, _, dropped2) = decimate_pairs(&times[..10], &values[..10], 100);
        assert!(!dropped2);
        assert_eq!(t2.len(), 10);
    }

    // ── .tlog fixture ──

    fn fixture_tlog() -> Vec<u8> {
        use crate::mavlink_proto::codec::{serialize_v2, MavSequence};
        use ::mavlink::ardupilotmega::{
            MavModeFlag, MavState, ATTITUDE_DATA, GLOBAL_POSITION_INT_DATA, HEARTBEAT_DATA,
        };
        use ::mavlink::MavHeader;

        let hb = MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: 5, // copter LOITER
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
            base_mode: MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED,
            system_status: MavState::MAV_STATE_ACTIVE,
            mavlink_version: 3,
        });
        let gpi = |lat: i32, alt: i32| {
            MavMessage::GLOBAL_POSITION_INT(GLOBAL_POSITION_INT_DATA {
                time_boot_ms: 1000,
                lat,
                lon: 8_500_000,
                alt,
                relative_alt: 50_000,
                vx: 0,
                vy: 0,
                vz: 0,
                hdg: 9000,
            })
        };
        let att = MavMessage::ATTITUDE(ATTITUDE_DATA {
            time_boot_ms: 1000,
            roll: 0.1,
            pitch: -0.05,
            yaw: 1.5708,
            ..Default::default()
        });

        let frames = vec![
            hb,
            gpi(477_000_000, 100_000),
            att,
            gpi(477_002_000, 102_000),
        ];
        let mut seq = MavSequence::new();
        let mut out = Vec::new();
        for (n, msg) in frames.into_iter().enumerate() {
            out.extend_from_slice(&((n as u64 + 1) * 1_000_000).to_be_bytes());
            let header = MavHeader {
                system_id: 1,
                component_id: 1,
                sequence: 0,
            };
            out.extend_from_slice(&serialize_v2(&header, &msg, &mut seq));
        }
        out
    }

    #[test]
    fn tlog_fixture_decodes_curated_series_track_and_mode() {
        let bytes = fixture_tlog();
        let ds = parse_tlog_viewer(&bytes, "fixture.tlog", bytes.len() as u64)
            .expect("tlog must decode");
        assert_eq!(ds.source_format, "tlog");
        assert_eq!(ds.vehicle_variant.as_deref(), Some("ArduCopter"));

        let gpi = ds
            .types
            .iter()
            .find(|t| t.name == "GLOBAL_POSITION_INT")
            .expect("GPI series");
        assert_eq!(gpi.count, 2);
        let alt = gpi
            .fields
            .iter()
            .find(|f| f.name == "alt")
            .expect("alt field");
        assert_eq!(alt.unit.as_deref(), Some("m"));
        assert_eq!(alt.values, vec![100.0, 102.0]);

        assert!(ds.types.iter().any(|t| t.name == "ATTITUDE"));
        assert_eq!(ds.track.len(), 2);
        assert_eq!(ds.track[0].mode_id.as_deref(), Some("loiter"));
        assert_eq!(ds.modes.len(), 1);
        assert_eq!(ds.modes[0].mode_id, "loiter");
    }

    #[test]
    fn tlog_garbage_is_an_error_not_a_panic() {
        let junk = vec![0x99u8; 2048];
        assert!(parse_tlog_viewer(&junk, "junk.tlog", junk.len() as u64).is_err());
    }
}
