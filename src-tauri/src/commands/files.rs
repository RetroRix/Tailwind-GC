// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

//! Chunked binary file transfer (Tailwind offline map packs).
//!
//! A .mappack can be tens of megabytes — too big to pass through one JSON invoke as a
//! number array. The frontend builds/reads the archive in memory and moves it through
//! these commands in base64 chunks instead. `append=false` truncates/creates the file.

use base64::Engine;
use std::io::{Read, Seek, SeekFrom, Write};

/// Write one base64 chunk to `path`. With `append=false` the file is truncated first.
#[tauri::command]
pub fn binary_file_write_chunk(
    path: String,
    chunk_base64: String,
    append: bool,
) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(chunk_base64.as_bytes())
        .map_err(|e| format!("base64 decode failed: {e}"))?;
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true);
    if append {
        opts.append(true);
    } else {
        opts.truncate(true);
    }
    let mut file = opts.open(&path).map_err(|e| format!("open {path}: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
}

/// Size of a file in bytes (the frontend pages through it with read chunks).
#[tauri::command]
pub fn binary_file_size(path: String) -> Result<u64, String> {
    let meta = std::fs::metadata(&path).map_err(|e| format!("stat {path}: {e}"))?;
    Ok(meta.len())
}

/// Read `len` bytes at `offset` from `path`, returned base64-encoded.
#[tauri::command]
pub fn binary_file_read_chunk(path: String, offset: u64, len: u64) -> Result<String, String> {
    let mut file = std::fs::File::open(&path).map_err(|e| format!("open {path}: {e}"))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek {path}: {e}"))?;
    let mut buf = vec![0u8; len as usize];
    file.read_exact(&mut buf)
        .map_err(|e| format!("read {path}: {e}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}
