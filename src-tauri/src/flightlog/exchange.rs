// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Flight data exchange — export/import complete flight records as .kflight files.
// A .kflight file is a self-contained SQLite database with the same schema as the
// main flights database, containing one or more flights with all their data:
// metadata, telemetry records, blackbox records, and archived blackbox files.

use std::fmt::Write;
use std::path::Path;

use rusqlite::{params, Connection};

use super::db;
use super::types::{BatteryRecord, Flight, FlightSummary, TelemetryRecord};

/// Stays 2 while changes are additive: Kite 1.0 reads named columns and never checks it. Raise it when a
/// file carries data an older importer drops — that importer then tells the user (`ImportResult::newer_format`).
const KFLIGHT_SCHEMA_VERSION: u32 = 2;

/// Application identifier stored in the export header
const KFLIGHT_APP_ID: &str = "KiteGC";

// ── Export ───────────────────────────────────────────────────────────

/// Export one or more flights (by ID) from the main database into a .kflight file.
/// The output file is a self-contained SQLite database.
pub fn export_flights(
    source_conn: &Connection,
    flight_ids: &[i64],
    output_path: &Path,
) -> Result<usize, String> {
    if flight_ids.is_empty() {
        return Err("No flights selected for export".into());
    }

    // Remove existing file if present (user confirmed overwrite in the save dialog)
    if output_path.exists() {
        std::fs::remove_file(output_path)
            .map_err(|e| format!("Failed to remove existing file: {}", e))?;
    }

    // Create the export database with full schema
    let out = create_export_db(output_path)?;

    // Copy each flight, tracking old→new ID mapping for linked_flight_id remapping
    let mut exported = 0;
    let mut id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let mut exported_flights: Vec<Flight> = Vec::new();
    for &flight_id in flight_ids {
        match copy_flight(source_conn, &out, flight_id) {
            Ok(new_id) => {
                id_map.insert(flight_id, new_id);
                exported += 1;
                if let Ok(Some(flight)) = db::get_flight(source_conn, flight_id) {
                    exported_flights.push(flight);
                }
            }
            Err(e) => {
                log::warn!("Failed to export flight {}: {}", flight_id, e);
            }
        }
    }

    // Remap linked_flight_id references using the old→new ID mapping
    for (&old_id, &new_id) in &id_map {
        let old_linked: Option<i64> = source_conn
            .query_row(
                "SELECT linked_flight_id FROM flights WHERE id = ?1",
                params![old_id],
                |row| row.get(0),
            )
            .unwrap_or(None);
        if let Some(old_linked_id) = old_linked {
            if let Some(&new_linked_id) = id_map.get(&old_linked_id) {
                out.execute(
                    "UPDATE flights SET linked_flight_id = ?1 WHERE id = ?2",
                    params![new_linked_id, new_id],
                ).ok();
            }
        }
    }

    if exported == 0 {
        // Clean up the empty file
        drop(out);
        std::fs::remove_file(output_path).ok();
        return Err("No flights could be exported".into());
    }

    // Store export metadata
    out.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS _kflight_meta (key TEXT PRIMARY KEY, value TEXT);
         INSERT OR REPLACE INTO _kflight_meta VALUES ('app', '{}');
         INSERT OR REPLACE INTO _kflight_meta VALUES ('schema_version', '{}');
         INSERT OR REPLACE INTO _kflight_meta VALUES ('exported_at', datetime('now'));
         INSERT OR REPLACE INTO _kflight_meta VALUES ('flight_count', '{}');",
        KFLIGHT_APP_ID, KFLIGHT_SCHEMA_VERSION, exported,
    ))
    .map_err(|e| format!("Failed to write export metadata: {}", e))?;

    // Compact the file
    out.execute_batch("VACUUM;")
        .map_err(|e| format!("VACUUM failed: {}", e))?;

    // Export a human-readable summary sidecar next to the .kflight file.
    // Example: flights_export.kflight -> flights_export.txt
    write_export_info_sidecar(output_path, &exported_flights)?;

    log::info!(
        "Exported {} flight(s) to {}",
        exported,
        output_path.display()
    );
    Ok(exported)
}

fn write_export_info_sidecar(output_path: &Path, flights: &[Flight]) -> Result<(), String> {
    let txt_path = output_path.with_extension("txt");
    let mut text = String::new();

    writeln!(&mut text, "KiteGC Flight Export Info").ok();
    writeln!(&mut text, "=======================").ok();
    writeln!(&mut text, "Export file: {}", output_path.display()).ok();
    writeln!(&mut text, "Exported at (UTC): {}", chrono::Utc::now().to_rfc3339()).ok();
    writeln!(&mut text, "Flight count: {}", flights.len()).ok();
    writeln!(&mut text).ok();

    for (idx, f) in flights.iter().enumerate() {
        writeln!(&mut text, "[Flight {}]", idx + 1).ok();
        writeln!(&mut text, "ID: {}", f.id).ok();
        writeln!(&mut text, "Start: {}", f.start_time.to_rfc3339()).ok();
        writeln!(
            &mut text,
            "End: {}",
            f.end_time
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "-".to_string())
        )
        .ok();
        writeln!(&mut text, "Duration sec: {}", f.duration_sec.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())).ok();
        writeln!(&mut text, "Source: {}", f.source).ok();
        writeln!(&mut text, "Craft: {}", f.craft_name).ok();
        writeln!(&mut text, "FC: {} {}", f.fc_variant, f.fc_version).ok();
        writeln!(&mut text, "Board: {}", f.board_id).ok();
        writeln!(&mut text, "Location: {}", f.location_name.clone().unwrap_or_else(|| "-".to_string())).ok();
        writeln!(&mut text, "Max alt m: {}", f.max_alt_m.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".to_string())).ok();
        writeln!(&mut text, "Max speed m/s: {}", f.max_speed_ms.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".to_string())).ok();
        writeln!(&mut text, "Total distance m: {}", f.total_distance_m.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "-".to_string())).ok();
        writeln!(
            &mut text,
            "Weather: temp={} wind={} wind_deg={} desc={}",
            f.weather_temp_c.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "-".to_string()),
            f.weather_wind_ms.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "-".to_string()),
            f.weather_wind_deg.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string()),
            f.weather_desc.clone().unwrap_or_else(|| "-".to_string())
        )
        .ok();
        writeln!(&mut text, "Linked flight ID: {}", f.linked_flight_id.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())).ok();
        writeln!(&mut text, "Notes:").ok();
        writeln!(
            &mut text,
            "{}",
            f.notes
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| "-".to_string())
        )
        .ok();
        writeln!(&mut text).ok();
    }

    std::fs::write(&txt_path, text)
        .map_err(|e| format!("Failed to write export info TXT '{}': {}", txt_path.display(), e))?;
    Ok(())
}

/// Create a new SQLite database with the full flight log schema.
fn create_export_db(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to create export file: {}", e))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| format!("Pragma error: {}", e))?;

    // Create all tables matching the main DB schema (v6)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS flights (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time      TEXT NOT NULL,
            end_time        TEXT,
            duration_sec    INTEGER,
            source          TEXT NOT NULL DEFAULT 'live',
            craft_name      TEXT NOT NULL DEFAULT '',
            fc_variant      TEXT NOT NULL DEFAULT '',
            fc_version      TEXT NOT NULL DEFAULT '',
            board_id        TEXT NOT NULL DEFAULT '',
            platform_type   INTEGER NOT NULL DEFAULT 0,
            protocol        TEXT NOT NULL DEFAULT 'MSP',
            start_lat       REAL,
            start_lon       REAL,
            location_name   TEXT,
            weather_temp_c  REAL,
            weather_wind_ms REAL,
            weather_wind_deg INTEGER,
            weather_desc    TEXT,
            max_alt_m       REAL,
            max_speed_ms    REAL,
            max_distance_m  REAL,
            total_distance_m REAL,
            battery_used_mah INTEGER,
            notes           TEXT,
            linked_flight_id INTEGER REFERENCES flights(id),
            pilot_name      TEXT,
            pilot_id        TEXT,
            battery_serial  TEXT,
            utc_offset_min  INTEGER,
            fc_uid          TEXT
        );

        CREATE TABLE IF NOT EXISTS telemetry_records (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            flight_id    INTEGER NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
            timestamp_ms INTEGER NOT NULL,
            lat          REAL,
            lon          REAL,
            alt_m        REAL,
            speed_ms     REAL,
            heading      INTEGER,
            vario_ms     REAL,
            voltage      REAL,
            current_a    REAL,
            mah_drawn    INTEGER,
            rssi         INTEGER,
            roll         REAL,
            pitch        REAL,
            yaw          INTEGER,
            fix_type     INTEGER,
            num_sat      INTEGER,
            cpu_load     INTEGER,
            link_quality INTEGER,
            baro_alt_m   REAL,
            gps_hdop     REAL,
            gps_eph      REAL,
            gps_epv      REAL,
            active_wp_number INTEGER,
            active_flight_mode_flags INTEGER,
            state_flags  INTEGER,
            nav_state    INTEGER,
            nav_flags    INTEGER,
            rx_signal_received INTEGER,
            hw_health_status INTEGER,
            baro_temperature REAL,
            wind_n_ms    REAL,
            wind_e_ms    REAL,
            wind_d_ms    REAL,
            rc_data_json TEXT,
            rc_command_json TEXT,
            nav_lat      REAL,
            nav_lon      REAL,
            nav_alt_m    REAL,
            battery_percentage INTEGER,
            mode_primary TEXT,
            mode_modifiers TEXT,
            link_snr     INTEGER,
            link_rssi_dbm INTEGER,
            airspeed_ms  REAL,
            throttle_pct REAL,
            target_lat   REAL,
            target_lon   REAL,
            target_alt_m REAL
        );

        CREATE TABLE IF NOT EXISTS battery_records (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            flight_id     INTEGER NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
            timestamp_ms  INTEGER NOT NULL,
            instance      INTEGER NOT NULL,
            voltage       REAL,
            current_a     REAL,
            mah_drawn     INTEGER,
            battery_percentage INTEGER,
            cell_count    INTEGER,
            temperature   REAL
        );

        CREATE TABLE IF NOT EXISTS blackbox_records (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            flight_id     INTEGER NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
            timestamp_us  INTEGER NOT NULL,
            csv_data      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS blackbox_files (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            flight_id         INTEGER NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
            original_filename TEXT NOT NULL,
            log_index         INTEGER NOT NULL DEFAULT 0,
            file_data         BLOB NOT NULL,
            file_size         INTEGER NOT NULL,
            imported_at       TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_telemetry_flight
            ON telemetry_records(flight_id, timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_battery_flight
            ON battery_records(flight_id, timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_blackbox_records_flight
            ON blackbox_records(flight_id, timestamp_us);
        CREATE INDEX IF NOT EXISTS idx_blackbox_files_flight
            ON blackbox_files(flight_id);",
    )
    .map_err(|e| format!("Schema creation error: {}", e))?;

    Ok(conn)
}

/// Copy a single flight with all related data from source to destination DB.
fn copy_flight(
    src: &Connection,
    dst: &Connection,
    flight_id: i64,
) -> Result<i64, String> {
    // 1. Read the flight row
    let flight = db::get_flight(src, flight_id)
        .map_err(|e| format!("Read flight: {}", e))?
        .ok_or_else(|| format!("Flight {} not found", flight_id))?;

    // 2. Insert into destination (gets a new ID)
    let new_id = db::insert_flight(dst, &flight)
        .map_err(|e| format!("Insert flight: {}", e))?;

    // Finalize with stats + weather (insert_flight doesn't copy all computed fields)
    dst.execute(
        "UPDATE flights SET
            end_time = ?1, duration_sec = ?2,
            max_alt_m = ?3, max_speed_ms = ?4, max_distance_m = ?5,
            total_distance_m = ?6, battery_used_mah = ?7,
            location_name = ?8, weather_temp_c = ?9, weather_wind_ms = ?10,
            weather_wind_deg = ?11, weather_desc = ?12
         WHERE id = ?13",
        params![
            flight.end_time.map(|t| t.to_rfc3339()),
            flight.duration_sec,
            flight.max_alt_m,
            flight.max_speed_ms,
            flight.max_distance_m,
            flight.total_distance_m,
            flight.battery_used_mah,
            flight.location_name,
            flight.weather_temp_c,
            flight.weather_wind_ms,
            flight.weather_wind_deg,
            flight.weather_desc,
            new_id,
        ],
    )
    .map_err(|e| format!("Finalize exported flight: {}", e))?;

    // 3. Copy telemetry records
    let track = db::get_flight_track(src, flight_id)
        .map_err(|e| format!("Read telemetry: {}", e))?;
    if !track.is_empty() {
        let remapped: Vec<TelemetryRecord> = track
            .into_iter()
            .map(|mut r| {
                r.flight_id = new_id;
                r
            })
            .collect();
        db::insert_telemetry_batch(dst, &remapped)
            .map_err(|e| format!("Insert telemetry: {}", e))?;
    }

    // 3b. Copy per-instance battery samples (multi-battery)
    let bat = db::get_flight_battery_records(src, flight_id)
        .map_err(|e| format!("Read battery records: {}", e))?;
    if !bat.is_empty() {
        let remapped: Vec<BatteryRecord> = bat
            .into_iter()
            .map(|mut r| {
                r.flight_id = new_id;
                r
            })
            .collect();
        db::insert_battery_records_batch(dst, &remapped)
            .map_err(|e| format!("Insert battery records: {}", e))?;
    }

    // 4. Copy blackbox records
    copy_blackbox_records(src, dst, flight_id, new_id)?;

    // 5. Copy blackbox files (BLOBs) — the archived originals (INAV blackbox logs, ArduPilot
    //    dataflash). Not on mobile: the mobile database never archives originals (ANDROID_SUPPORT.md
    //    §4 — nothing there could decode them, and dataflash alone runs to hundreds of megabytes), so
    //    an imported .kflight brings the flight, its track and its records and leaves the blobs
    //    behind. The desktop copy it came from still has them.
    if cfg!(any(target_os = "android", target_os = "ios")) {
        log::info!("Import: flight {flight_id} — archived original log files are not stored on mobile");
    } else {
        copy_blackbox_files(src, dst, flight_id, new_id)?;
    }

    Ok(new_id)
}

fn copy_blackbox_records(
    src: &Connection,
    dst: &Connection,
    old_flight_id: i64,
    new_flight_id: i64,
) -> Result<(), String> {
    let mut stmt = src
        .prepare("SELECT timestamp_us, csv_data FROM blackbox_records WHERE flight_id = ?1 ORDER BY timestamp_us")
        .map_err(|e| format!("Prepare blackbox_records: {}", e))?;

    let rows: Vec<(i64, String)> = stmt
        .query_map(params![old_flight_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("Query blackbox_records: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read blackbox_records: {}", e))?;

    if rows.is_empty() {
        return Ok(());
    }

    db::insert_blackbox_records(dst, new_flight_id, &rows)
        .map_err(|e| format!("Insert blackbox_records: {}", e))?;

    Ok(())
}

fn copy_blackbox_files(
    src: &Connection,
    dst: &Connection,
    old_flight_id: i64,
    new_flight_id: i64,
) -> Result<(), String> {
    let mut stmt = src
        .prepare(
            "SELECT original_filename, log_index, file_data, file_size, imported_at
             FROM blackbox_files WHERE flight_id = ?1",
        )
        .map_err(|e| format!("Prepare blackbox_files: {}", e))?;

    let files: Vec<(String, i32, Vec<u8>, i64, String)> = stmt
        .query_map(params![old_flight_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|e| format!("Query blackbox_files: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Read blackbox_files: {}", e))?;

    for (filename, log_index, file_data, file_size, imported_at) in files {
        dst.execute(
            "INSERT INTO blackbox_files (flight_id, original_filename, log_index, file_data, file_size, imported_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![new_flight_id, filename, log_index, file_data, file_size, imported_at],
        )
        .map_err(|e| format!("Insert blackbox_file: {}", e))?;
    }

    Ok(())
}

// ── Import ──────────────────────────────────────────────────────────

/// Result of importing flights from a .kflight file
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
    /// The file was written by a newer Kite: what this version knows was imported, the rest was left out.
    pub newer_format: bool,
    /// The new flights' ids in this database.
    pub imported_ids: Vec<i64>,
}

/// Import flights from a .kflight file into the main database.
/// Skips duplicates (same craft_name + start_time ±10s).
pub fn import_flights(
    target_conn: &Connection,
    kflight_path: &Path,
) -> Result<ImportResult, String> {
    if !kflight_path.exists() {
        return Err("File not found".into());
    }
    with_kflight_copy(kflight_path, |src| import_from(target_conn, src, kflight_path))
}

/// `import_flights` on the open working copy `src` (`kflight_path` names the user's file in the log).
fn import_from(
    target_conn: &Connection,
    src: &Connection,
    kflight_path: &Path,
) -> Result<ImportResult, String> {
    // No meta table, no version row, or one that does not parse: an old or foreign file, not a newer one.
    let file_version: u32 = src
        .query_row("SELECT value FROM _kflight_meta WHERE key = 'schema_version'", [], |r| r.get::<_, String>(0))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let newer_format = file_version > KFLIGHT_SCHEMA_VERSION;
    if newer_format {
        log::warn!(
            ".kflight: format version {file_version}, this Kite reads up to {KFLIGHT_SCHEMA_VERSION} — importing what it knows"
        );
    }

    let has_flights: bool = src
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='flights'")
        .and_then(|mut s| s.exists([]))
        .unwrap_or(false);

    if !has_flights {
        return Err("Not a valid .kflight file: missing flights table".into());
    }
    // A `.kflight` exported before v19 lacks `fc_uid`; add it so the shared row reader applies.
    if !db::column_exists(&src, "flights", "fc_uid").unwrap_or(true) {
        let _ = src.execute_batch("ALTER TABLE flights ADD COLUMN fc_uid TEXT;");
    }

    // List all flights in the source file
    let src_flights = db::list_flights(&src)
        .map_err(|e| format!("Failed to list flights in .kflight: {}", e))?;

    // Snapshot target flights BEFORE import starts.
    // Duplicate detection must only compare against pre-existing flights,
    // otherwise linked pairs inside the same .kflight will self-conflict.
    let baseline_target_flights = db::list_flights(target_conn)
        .map_err(|e| format!("Failed to list existing target flights: {}", e))?;

    // old source ID -> target ID (newly imported OR matched existing duplicate)
    let mut id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();

    let mut result = ImportResult {
        imported: 0,
        skipped: 0,
        errors: Vec::new(),
        newer_format,
        imported_ids: Vec::new(),
    };

    for summary in &src_flights {
        // Check duplicate only against baseline target flights.
        if let Some(existing_id) = find_duplicate_in_summaries(&baseline_target_flights, summary) {
            result.skipped += 1;
            id_map.insert(summary.id, existing_id);
            log::info!("Import: skipping duplicate flight {} ({})", summary.id, summary.start_time);
            continue;
        }

        match copy_flight(&src, target_conn, summary.id) {
            Ok(new_id) => {
                result.imported += 1;
                id_map.insert(summary.id, new_id);
            }
            Err(e) => {
                log::warn!("Import: failed to import flight {}: {}", summary.id, e);
                result.errors.push(format!("Flight {}: {}", summary.id, e));
            }
        }
    }

    // Restore linked relationships from source file using old->target ID mapping.
    // This also supports mixed cases where one side was skipped as duplicate and
    // mapped to an existing target flight.
    let mut linked_pairs_done: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
    for summary in &src_flights {
        let Some(old_linked_id) = summary.linked_flight_id else {
            continue;
        };
        let Some(&target_a) = id_map.get(&summary.id) else {
            continue;
        };
        let Some(&target_b) = id_map.get(&old_linked_id) else {
            continue;
        };
        if target_a == target_b {
            continue;
        }

        let pair = if target_a < target_b {
            (target_a, target_b)
        } else {
            (target_b, target_a)
        };
        if linked_pairs_done.contains(&pair) {
            continue;
        }

        if let Err(e) = db::link_flights(target_conn, pair.0, pair.1) {
            log::warn!("Import: failed to relink flights {} <-> {}: {}", pair.0, pair.1, e);
            result
                .errors
                .push(format!("Relink {}<->{}: {}", pair.0, pair.1, e));
        } else {
            linked_pairs_done.insert(pair);
        }
    }

    log::info!(
        "Imported {} flight(s) from {} ({} skipped, {} errors)",
        result.imported,
        kflight_path.display(),
        result.skipped,
        result.errors.len(),
    );

    Ok(result)
}

/// Run `f` on a private working copy of the `.kflight` (deleted afterwards): the user's file — maybe
/// read-only or on a share — is never opened itself (columns get back-filled, `-wal`/`-shm` created).
fn with_kflight_copy<T>(
    path: &Path,
    f: impl FnOnce(&Connection) -> Result<T, String>,
) -> Result<T, String> {
    let copy = crate::user_file::temp_path("kflight")?;
    if let Err(e) = std::fs::copy(path, &copy) {
        db::remove_temp_session(&copy); // a partial copy
        return Err(format!("Failed to open .kflight file: {}", e));
    }
    // A WAL left by an unclean close holds committed pages of its own — bring it along.
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    let wal = std::path::PathBuf::from(wal);
    if wal.exists() {
        let mut copy_wal = copy.as_os_str().to_os_string();
        copy_wal.push("-wal");
        if let Err(e) = std::fs::copy(&wal, std::path::PathBuf::from(copy_wal)) {
            db::remove_temp_session(&copy);
            return Err(format!("Failed to open .kflight file: {}", e));
        }
    }
    let result = Connection::open(&copy)
        .map_err(|e| format!("Failed to open .kflight file: {}", e))
        .and_then(|conn| f(&conn));
    db::remove_temp_session(&copy); // the copy + its -wal / -shm
    result
}

/// A `.kflight` written by an older Kite lacks the columns added since; add them so the shared row
/// readers apply. `flights.source_before_link` (v21) is NOT added: its absence marks a file whose value
/// has to be derived (`source_before_link`).
fn add_missing_columns(conn: &Connection) {
    for (table, column, ddl) in [
        ("flights", "fc_uid", "ALTER TABLE flights ADD COLUMN fc_uid TEXT;"),
        ("flights", "group_id", "ALTER TABLE flights ADD COLUMN group_id TEXT;"),
        ("flights", "vehicle_id", "ALTER TABLE flights ADD COLUMN vehicle_id INTEGER;"),
        ("telemetry_records", "target_lat", "ALTER TABLE telemetry_records ADD COLUMN target_lat REAL;"),
        ("telemetry_records", "target_lon", "ALTER TABLE telemetry_records ADD COLUMN target_lon REAL;"),
        ("telemetry_records", "target_alt_m", "ALTER TABLE telemetry_records ADD COLUMN target_alt_m REAL;"),
    ] {
        if !db::column_exists(conn, table, column).unwrap_or(true) {
            if let Err(e) = conn.execute_batch(ddl) {
                log::warn!(".kflight: could not add {table}.{column}: {e}");
            }
        }
    }
}

fn find_duplicate_in_summaries(
    baseline: &[FlightSummary],
    incoming: &FlightSummary,
) -> Option<i64> {
    let lower = incoming.start_time - chrono::Duration::seconds(10);
    let upper = incoming.start_time + chrono::Duration::seconds(10);

    baseline
        .iter()
        // Keep behavior aligned with find_duplicate_flight(): only blackbox/both are duplicate sources.
        .filter(|f| f.source == "blackbox" || f.source == "both")
        .find(|f| {
            f.craft_name == incoming.craft_name
                && f.start_time >= lower
                && f.start_time <= upper
        })
        .map(|f| f.id)
}

// ── Read from .kflight (for offline replay) ─────────────────────────

/// List flights in a .kflight file (for offline replay / preview)
pub fn list_flights_in_file(path: &Path) -> Result<Vec<FlightSummary>, String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open .kflight file: {}", e))?;
    db::list_flights(&conn).map_err(|e| format!("Query error: {}", e))
}

/// Get a single flight from a .kflight file
pub fn get_flight_from_file(path: &Path, flight_id: i64) -> Result<Option<Flight>, String> {
    let conn = Connection::open(path)
        .map_err(|e| format!("Failed to open .kflight file: {}", e))?;
    db::get_flight(&conn, flight_id).map_err(|e| format!("Query error: {}", e))
}

/// Get the telemetry track from a .kflight file
pub fn get_track_from_file(path: &Path, flight_id: i64) -> Result<Vec<TelemetryRecord>, String> {
    with_kflight_copy(path, |conn| {
        add_missing_columns(conn);
        db::get_flight_track(conn, flight_id).map_err(|e| format!("Query error: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kite-kflight-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn vehicle(conn: &Connection, name: &str, craft: Option<&str>, uid: Option<&str>) -> i64 {
        conn.execute(
            "INSERT INTO vehicles (name, craft_name, fc_uid) VALUES (?1, ?2, ?3)",
            params![name, craft, uid],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn flight(craft: &str, uid: Option<&str>) -> Flight {
        Flight {
            id: 0,
            start_time: chrono::DateTime::parse_from_rfc3339("2026-11-01T10:00:00Z").unwrap().with_timezone(&chrono::Utc),
            end_time: None,
            duration_sec: Some(60),
            source: "live".into(),
            craft_name: craft.into(),
            fc_variant: "INAV".into(),
            fc_version: "9.0.0".into(),
            board_id: String::new(),
            platform_type: 1,
            protocol: "MSP".into(),
            start_lat: Some(48.1),
            start_lon: Some(11.5),
            location_name: None,
            weather_temp_c: None,
            weather_wind_ms: None,
            weather_wind_deg: None,
            weather_desc: None,
            max_alt_m: None,
            max_speed_ms: None,
            max_distance_m: None,
            total_distance_m: None,
            battery_used_mah: None,
            notes: None,
            linked_flight_id: None,
            pilot_name: None,
            pilot_id: None,
            battery_serial: None,
            utc_offset_min: None,
            fc_uid: uid.map(String::from),
            group_id: None,
            vehicle_id: None,
        }
    }

    /// The local vehicle link never travels in a `.kflight`: the export stores none, the import resolves
    /// the flight against the target's library (FC id first) — also for a file written before v20.
    #[test]
    fn kflight_round_trip_resolves_the_vehicle_in_the_target_library() {
        let dir = temp_dir("vehicle");
        let file = dir.join("export.kflight");
        {
            let src = db::open_database(&dir.join("src").join("flights.db")).unwrap();
            let src_vehicle = vehicle(&src, "Wing", Some("Wing"), Some("UIDA"));
            let id = db::insert_flight(&src, &flight("Wing", Some("UIDA"))).unwrap();
            assert_eq!(db::get_flight(&src, id).unwrap().unwrap().vehicle_id, Some(src_vehicle));
            assert_eq!(export_flights(&src, &[id], &file).unwrap(), 1);
        }
        {
            let k = Connection::open(&file).unwrap();
            let v: Option<i64> = k.query_row("SELECT vehicle_id FROM flights", [], |r| r.get(0)).unwrap();
            assert_eq!(v, None, "the export carries no local vehicle id");
        }

        {
            let dst = db::open_database(&dir.join("dst").join("flights.db")).unwrap();
            vehicle(&dst, "Filler A", Some("Quad"), None);
            vehicle(&dst, "Filler B", Some("Plane"), None);
            let by_craft = vehicle(&dst, "Same name", Some("wing"), None);
            let same_fc = vehicle(&dst, "Same FC", None, Some("uida"));
            let r = import_flights(&dst, &file).unwrap();
            assert_eq!((r.imported, r.errors.len()), (1, 0));
            let imported = db::list_flights(&dst).unwrap();
            assert_eq!(imported.len(), 1);
            assert_eq!(imported[0].vehicle_id, Some(same_fc), "the FC id beats the craft name ({by_craft})");
            let f = db::get_flight(&dst, imported[0].id).unwrap().unwrap();
            assert_eq!((f.craft_name.as_str(), f.fc_uid.as_deref()), ("Wing", Some("UIDA")));
        }
        // When unsure, unlinked: only a craft-name vehicle that stores another FC id → no link.
        {
            let dst = db::open_database(&dir.join("dst-conflict").join("flights.db")).unwrap();
            vehicle(&dst, "Other FC", Some("Wing"), Some("UIDZ"));
            let r = import_flights(&dst, &file).unwrap();
            assert_eq!((r.imported, r.errors.len()), (1, 0));
            assert_eq!(db::list_flights(&dst).unwrap()[0].vehicle_id, None);
        }

        // A `.kflight` from before v20 has no vehicle_id column: listed, imported, resolved.
        {
            let k = Connection::open(&file).unwrap();
            k.execute_batch("ALTER TABLE flights DROP COLUMN vehicle_id;").unwrap();
        }
        assert_eq!(list_flights_in_file(&file).unwrap()[0].vehicle_id, None);
        {
            let dst = db::open_database(&dir.join("dst-old-file").join("flights.db")).unwrap();
            let by_craft = vehicle(&dst, "By craft", Some(" WING "), None);
            let r = import_flights(&dst, &file).unwrap();
            assert_eq!((r.imported, r.errors.len()), (1, 0));
            assert_eq!(db::list_flights(&dst).unwrap()[0].vehicle_id, Some(by_craft));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn source_of(conn: &Connection, flight_id: i64) -> (String, Option<String>) {
        conn.query_row(
            "SELECT source, source_before_link FROM flights WHERE id = ?1",
            params![flight_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    /// Imported flights by start minute → id (each test flight starts at its own minute).
    fn by_minute(conn: &Connection) -> std::collections::HashMap<u32, i64> {
        use chrono::Timelike;
        db::list_flights(conn).unwrap().iter().map(|f| (f.start_time.minute(), f.id)).collect()
    }

    /// A flight in the `src` main DB, starting at `hh:mm` (tests tell flights apart by start minute).
    fn at(src: &Connection, hour: u32, minute: u32, source: &str, protocol: &str) -> i64 {
        let start = chrono::DateTime::parse_from_rfc3339(&format!("2026-11-01T{hour:02}:{minute:02}:00Z"))
            .unwrap()
            .with_timezone(&chrono::Utc);
        let f = Flight { start_time: start, source: source.into(), protocol: protocol.into(), ..flight("Wing", None) };
        db::insert_flight(src, &f).unwrap()
    }

    /// The file's `blackbox_records` rows as (flight id, timestamp_us, csv_data).
    fn records_in(k: &Connection) -> Vec<(i64, i64, String)> {
        k.prepare("SELECT flight_id, timestamp_us, csv_data FROM blackbox_records ORDER BY flight_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Two linked pairs in one `.kflight`, one log flight with and one without an archived original: the
    /// import keeps `source_before_link`, never copies the marker rows, and an unlink in the target
    /// restores each flight's own source.
    #[test]
    fn kflight_round_trip_keeps_source_before_link() {
        let dir = temp_dir("source-before-link");
        let file = dir.join("pairs.kflight");
        {
            let src = db::open_database(&dir.join("src").join("flights.db")).unwrap();
            let (live_1, log_1) = (at(&src, 10, 1, "live", "MSP"), at(&src, 10, 2, "blackbox", "MSP"));
            let (live_2, log_2) = (at(&src, 10, 3, "live", "MSP"), at(&src, 10, 4, "blackbox", "MSP"));
            db::insert_blackbox_file(&src, log_1, "LOG00001.TXT", 0, b"txt").unwrap();
            db::link_flights(&src, live_1, log_1).unwrap();
            db::link_flights(&src, live_2, log_2).unwrap();
            assert_eq!(export_flights(&src, &[live_1, log_1, live_2, log_2], &file).unwrap(), 4);
        }
        {
            let k = Connection::open(&file).unwrap();
            let m = by_minute(&k);
            assert_eq!(
                records_in(&k),
                [(m[&2], 0, String::new()), (m[&4], 0, String::new())],
                "exactly one empty marker row per log flight, none for the live flights"
            );
            assert_eq!(source_of(&k, m[&2]), ("both".into(), Some("blackbox".into())));
            assert_eq!(source_of(&k, m[&4]), ("both".into(), Some("blackbox".into())));
            assert_eq!(source_of(&k, m[&3]), ("both".into(), Some("live".into())));
        }
        let dst = db::open_database(&dir.join("dst").join("flights.db")).unwrap();
        let r = import_flights(&dst, &file).unwrap();
        assert_eq!((r.imported, r.errors.len()), (4, 0));
        let m = by_minute(&dst);
        for (live, log) in [(m[&1], m[&2]), (m[&3], m[&4])] {
            assert_eq!(db::get_flight(&dst, live).unwrap().unwrap().linked_flight_id, Some(log));
            assert_eq!(source_of(&dst, live), ("both".into(), Some("live".into())));
            assert_eq!(source_of(&dst, log), ("both".into(), Some("blackbox".into())));
            db::unlink_flight(&dst, live).unwrap();
            assert_eq!(source_of(&dst, live), ("live".into(), None));
            assert_eq!(source_of(&dst, log), ("blackbox".into(), None), "flight {log}");
        }
        assert!(!db::table_exists(&dst, "blackbox_records").unwrap());
        drop(dst);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A flight of a pair exported without its partner travels as the single flight it was: its own
    /// source, no `source_before_link`, no link — and the marker row only if it is the log flight.
    #[test]
    fn flight_exported_without_its_partner_is_unlinked_in_the_file() {
        let dir = temp_dir("lone-export");
        let (log_file, live_file) = (dir.join("log.kflight"), dir.join("live.kflight"));
        {
            let src = db::open_database(&dir.join("src").join("flights.db")).unwrap();
            let (live_1, log_1) = (at(&src, 12, 1, "live", "MSP"), at(&src, 12, 2, "blackbox", "DATAFLASH"));
            let (live_2, log_2) = (at(&src, 12, 3, "live", "MSP"), at(&src, 12, 4, "blackbox", "BLACKBOX"));
            db::link_flights(&src, live_1, log_1).unwrap();
            db::link_flights(&src, live_2, log_2).unwrap();
            assert_eq!(export_flights(&src, &[log_1], &log_file).unwrap(), 1);
            assert_eq!(export_flights(&src, &[live_2], &live_file).unwrap(), 1);
            // The main DB keeps its pairs.
            assert_eq!(source_of(&src, log_1), ("both".into(), Some("blackbox".into())));
        }
        {
            let k = Connection::open(&log_file).unwrap();
            let id = by_minute(&k)[&2];
            assert_eq!(source_of(&k, id), ("blackbox".into(), None));
            assert_eq!(db::get_flight(&k, id).unwrap().unwrap().linked_flight_id, None);
            assert_eq!(records_in(&k), [(id, 0, String::new())]);
            let k = Connection::open(&live_file).unwrap();
            assert_eq!(source_of(&k, by_minute(&k)[&3]), ("live".into(), None));
            assert!(records_in(&k).is_empty());
        }
        let dst = db::open_database(&dir.join("dst").join("flights.db")).unwrap();
        for f in [&log_file, &live_file] {
            let r = import_flights(&dst, f).unwrap();
            assert_eq!((r.imported, r.errors.len()), (1, 0));
        }
        let m = by_minute(&dst);
        assert_eq!(source_of(&dst, m[&2]), ("blackbox".into(), None));
        assert_eq!(source_of(&dst, m[&3]), ("live".into(), None));
        drop(dst);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `.kflight` from before v21: the import derives `source_before_link` for the `both` flights from
    /// the FILE and never copies its `blackbox_records` rows. A `both` flight without its partner is
    /// imported as the single flight it was.
    #[test]
    fn old_kflight_import_derives_source_before_link() {
        let dir = temp_dir("old-source");
        let file = dir.join("old.kflight");
        {
            let src = db::open_database(&dir.join("src").join("flights.db")).unwrap();
            let (live_1, log_1) = (at(&src, 11, 1, "live", "MSP"), at(&src, 11, 2, "blackbox", "MSP"));
            let (live_2, log_2) = (at(&src, 11, 3, "live", "MSP"), at(&src, 11, 4, "blackbox", "MSP"));
            let lone = at(&src, 11, 5, "blackbox", "MSP");
            let (live_3, log_3) = (at(&src, 11, 6, "live", "MSP"), at(&src, 11, 7, "blackbox", "MSP"));
            let (live_4, log_4) = (at(&src, 11, 8, "live", "MSP"), at(&src, 11, 9, "blackbox", "DATAFLASH"));
            let (live_5, log_5) = (at(&src, 11, 10, "live", "MSP"), at(&src, 11, 11, "blackbox", "MSP"));
            db::insert_blackbox_file(&src, log_2, "00000042.BIN", 0, b"bin").unwrap();
            db::insert_blackbox_file(&src, log_5, "LOG00005.TXT", 0, b"txt").unwrap();
            for (a, b) in [(live_1, log_1), (live_2, log_2), (live_3, log_3), (live_4, log_4), (live_5, log_5)] {
                db::link_flights(&src, a, b).unwrap();
            }
            let all = [live_1, log_1, live_2, log_2, lone, live_3, log_3, live_4, log_4, live_5, log_5];
            assert_eq!(export_flights(&src, &all, &file).unwrap(), all.len());
        }
        {
            // Make it an old file: no source_before_link column, no markers; decoded rows for pair 1's
            // log flight and the unlinked one, a marker-shaped row for pair 3's; pair 4's ArduPilot flight
            // has nothing but its protocol; pair 5 lost its link (each flight exported alone).
            let k = Connection::open(&file).unwrap();
            k.execute_batch("ALTER TABLE flights DROP COLUMN source_before_link; DELETE FROM blackbox_records;")
                .unwrap();
            let m = by_minute(&k);
            for (id, csv) in [(m[&2], "1,2,3"), (m[&5], "1,2,3"), (m[&7], "")] {
                k.execute(
                    "INSERT INTO blackbox_records (flight_id, timestamp_us, csv_data) VALUES (?1, 0, ?2)",
                    params![id, csv],
                )
                .unwrap();
            }
            k.execute("UPDATE flights SET linked_flight_id = NULL WHERE id IN (?1, ?2)", params![m[&10], m[&11]])
                .unwrap();
        }
        let dst = db::open_database(&dir.join("dst").join("flights.db")).unwrap();
        let r = import_flights(&dst, &file).unwrap();
        assert_eq!((r.imported, r.errors.len()), (11, 0));
        let m = by_minute(&dst);
        let both = |before: &str| ("both".to_string(), Some(before.to_string()));
        assert_eq!(source_of(&dst, m[&1]), both("live"));
        assert_eq!(source_of(&dst, m[&2]), both("blackbox"), "decoded rows in the file");
        assert_eq!(source_of(&dst, m[&3]), both("live"));
        assert_eq!(source_of(&dst, m[&4]), both("blackbox"), "archived log in the file");
        assert_eq!(source_of(&dst, m[&5]), ("blackbox".into(), None), "unlinked flights stay NULL");
        assert_eq!(source_of(&dst, m[&6]), both("live"));
        assert_eq!(source_of(&dst, m[&7]), both("blackbox"), "a marker row counts like decoded rows");
        assert_eq!(source_of(&dst, m[&8]), both("live"));
        assert_eq!(source_of(&dst, m[&9]), both("blackbox"), "a log importer's protocol");
        assert_eq!(source_of(&dst, m[&10]), ("live".into(), None), "arrived without its partner");
        assert_eq!(source_of(&dst, m[&11]), ("blackbox".into(), None), "arrived without its partner");
        assert_eq!(db::get_flight(&dst, m[&11]).unwrap().unwrap().linked_flight_id, None);
        assert!(!db::table_exists(&dst, "blackbox_records").unwrap(), "decoded rows never reach the main DB");
        db::unlink_flight(&dst, m[&2]).unwrap();
        assert_eq!(source_of(&dst, m[&1]), ("live".into(), None));
        assert_eq!(source_of(&dst, m[&2]), ("blackbox".into(), None));
        drop(dst);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn mission(hash: &str, extra: Option<&str>) -> MissionInput {
        MissionInput {
            content_hash: hash.into(),
            name: "Survey".into(),
            format: "ardupilot".into(),
            waypoints_json: "[{\"lat\":48.1}]".into(),
            source_xml: None,
            wp_count: 1,
            total_distance_m: Some(120.5),
            alt_diff_m: None,
            max_alt_m: None,
            min_alt_m: None,
            bndbox_min_lat: None,
            bndbox_min_lon: None,
            bndbox_max_lat: None,
            bndbox_max_lon: None,
            notes: Some("north field".into()),
            home_lat: Some(48.1),
            home_lon: Some(11.5),
            vehicle_class: Some("plane".into()),
            extra_json: extra.map(String::from),
            created_at: Some("2026-10-01 08:00:00".into()),
            location_name: Some("Testfeld".into()),
        }
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    /// The linked mission, the v22 child tables and `alt_relative` travel with the flight; two flights
    /// of one mission share its row, and a target that already holds the mission keeps its own row.
    #[test]
    fn kflight_round_trip_carries_the_mission_and_the_v22_tables() {
        let dir = temp_dir("v22");
        let file = dir.join("export.kflight");
        {
            let src = db::open_database(&dir.join("src").join("flights.db")).unwrap();
            let a = db::insert_flight(&src, &flight("Wing", None)).unwrap();
            let mut second = flight("Wing", None);
            second.start_time += chrono::Duration::hours(1);
            let b = db::insert_flight(&src, &second).unwrap();
            let unlinked = db::upsert_mission(&src, &mission("hash-other", None)).unwrap();
            let m = db::upsert_mission(&src, &mission("hash-a", Some("{\"pattern\":1}"))).unwrap();
            db::link_flight_mission(&src, a, m).unwrap();
            db::link_flight_mission(&src, b, m).unwrap();
            assert_ne!(unlinked, m);
            src.execute("UPDATE flights SET alt_relative = 1, logged_wp_count = 7 WHERE id = ?1", params![a]).unwrap();
            db::insert_weather_record(&src, a, 2000, 48.1, 11.5, "open-meteo", "{\"steps\":[]}").unwrap();
            db::insert_weather_record(&src, a, 302000, 48.1, 11.5, "open-meteo", "{\"steps\":[1]}").unwrap();
            src.execute(
                "INSERT INTO gps_records (flight_id, timestamp_ms, instance, fix_raw, num_sat, hdop, lat, yaw_deg)
                 VALUES (?1, 1000, 1, 6, 17, 0.7, 48.1, NULL)",
                params![a],
            )
            .unwrap();
            src.execute(
                "INSERT INTO flight_videos (flight_id, rel_path, sync_a_frame, created_at)
                 VALUES (?1, 'cam/a.mp4', 12, '2026-11-01 10:00:00')",
                params![a],
            )
            .unwrap();
            assert_eq!(export_flights(&src, &[a, b], &file).unwrap(), 2);
        }
        {
            let k = Connection::open(&file).unwrap();
            assert_eq!(count(&k, "missions"), 1, "only the linked mission, once for both flights");
            assert_eq!(count(&k, "weather_records"), 2);
        }

        let dst = db::open_database(&dir.join("dst").join("flights.db")).unwrap();
        let r = import_flights(&dst, &file).unwrap();
        assert_eq!((r.imported, r.errors.len(), r.newer_format), (2, 0, false));

        // The same file stamped by a newer Kite: imported as far as known, and said so.
        {
            let k = Connection::open(&file).unwrap();
            k.execute(
                "UPDATE _kflight_meta SET value = ?1 WHERE key = 'schema_version'",
                params![(KFLIGHT_SCHEMA_VERSION + 1).to_string()],
            )
            .unwrap();
        }
        let again = db::open_database(&dir.join("dst-newer").join("flights.db")).unwrap();
        let r = import_flights(&again, &file).unwrap();
        assert_eq!((r.imported, r.newer_format), (2, true));
        let flights = db::list_flights(&dst).unwrap();
        let a = flights.iter().min_by_key(|f| f.start_time).unwrap().id;
        let b = flights.iter().max_by_key(|f| f.start_time).unwrap().id;
        let m = db::get_mission_for_flight(&dst, a).unwrap().unwrap();
        assert_eq!(db::get_mission_for_flight(&dst, b).unwrap().unwrap().id, m.id);
        assert_eq!(count(&dst, "missions"), 1);
        assert_eq!((m.content_hash.as_str(), m.vehicle_class.as_deref()), ("hash-a", Some("plane")));
        assert_eq!(m.extra_json.as_deref(), Some("{\"pattern\":1}"));
        assert_eq!((m.created_at.as_str(), m.location_name.as_deref()), ("2026-10-01 08:00:00", Some("Testfeld")));
        let extra = |id: i64| -> (Option<i64>, Option<i64>) {
            dst.query_row("SELECT alt_relative, logged_wp_count FROM flights WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap()
        };
        assert_eq!((extra(a), extra(b)), ((Some(1), Some(7)), (None, None)));
        for (table, cols) in CHILD_TABLES {
            let mut listed: Vec<String> = cols.split(',').map(|c| c.trim().to_string()).collect();
            let mut real: Vec<String> = dst
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}') WHERE name NOT IN ('id', 'flight_id')"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            listed.sort();
            real.sort();
            assert_eq!(listed, real, "CHILD_TABLES lists every column of {table}");
        }
        let weather = db::get_flight_weather_records(&dst, a).unwrap();
        assert_eq!(weather.iter().map(|w| w.timestamp_ms).collect::<Vec<_>>(), vec![2000, 302000]);
        assert_eq!(weather[1].data_json, "{\"steps\":[1]}");
        assert!(db::get_flight_weather_records(&dst, b).unwrap().is_empty());
        let gps: (i64, i64, Option<i64>, f64, Option<f64>) = dst
            .query_row(
                "SELECT flight_id, instance, fix_raw, hdop, yaw_deg FROM gps_records",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(gps, (a, 1, Some(6), 0.7, None));
        let video: (i64, String, Option<i64>, String) = dst
            .query_row("SELECT flight_id, rel_path, sync_a_frame, created_at FROM flight_videos", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!(video, (a, "cam/a.mp4".into(), Some(12), "2026-11-01 10:00:00".into()));
        drop(dst);

        // The target already holds the mission. Without a flight on it, the file's home, class and
        // extra data replace its own; with one, it only gains what it lacks. Its name always stays.
        for (case, has_flight) in [("dst-own", false), ("dst-own-linked", true)] {
            let own = db::open_database(&dir.join(case).join("flights.db")).unwrap();
            let mut mine = mission("hash-a", Some("{\"old\":1}"));
            mine.name = "My name".into();
            mine.vehicle_class = None;
            (mine.home_lat, mine.home_lon) = (Some(1.0), Some(2.0));
            let own_id = db::upsert_mission(&own, &mine).unwrap();
            if has_flight {
                let mut earlier = flight("Wing", None);
                earlier.start_time -= chrono::Duration::days(1);
                let id = db::insert_flight(&own, &earlier).unwrap();
                db::link_flight_mission(&own, id, own_id).unwrap();
            }
            assert_eq!(import_flights(&own, &file).unwrap().imported, 2);
            let newest = db::list_flights(&own).unwrap().into_iter().max_by_key(|f| f.start_time).unwrap().id;
            let linked = db::get_mission_for_flight(&own, newest).unwrap().unwrap();
            assert_eq!((linked.id, linked.name.as_str()), (own_id, "My name"), "{case}");
            assert_eq!(linked.vehicle_class.as_deref(), Some("plane"), "{case}: filled in either way");
            let (extra, home) = if has_flight { ("{\"old\":1}", (1.0, 2.0)) } else { ("{\"pattern\":1}", (48.1, 11.5)) };
            assert_eq!(linked.extra_json.as_deref(), Some(extra), "{case}");
            assert_eq!((linked.home_lat, linked.home_lon), (Some(home.0), Some(home.1)), "{case}");
            assert_eq!(count(&own, "missions"), 1);
            drop(own);
        }

        // A file from before v22: no mission link, none of the tables.
        {
            let k = Connection::open(&file).unwrap();
            k.execute_batch(
                "DROP TABLE missions; DROP TABLE gps_records; DROP TABLE weather_records; DROP TABLE flight_videos;
                 ALTER TABLE flights DROP COLUMN mission_id; ALTER TABLE flights DROP COLUMN alt_relative;",
            )
            .unwrap();
        }
        let old = db::open_database(&dir.join("dst-old").join("flights.db")).unwrap();
        let r = import_flights(&old, &file).unwrap();
        assert_eq!((r.imported, r.errors.len()), (2, 0));
        assert_eq!((count(&old, "missions"), count(&old, "weather_records")), (0, 0));
        drop(old);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
