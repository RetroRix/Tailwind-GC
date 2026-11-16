// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Runtime MAVLink parameter reads (geofence params: ArduPilot FENCE_* / PX4 GF_*). Mirrors the mission
// microprotocol's request/receiver pattern via `RegisterParamReceiver`. Writes reuse
// `control::set_param` (fire-and-forget PARAM_SET). See docs/active/GEOFENCE.md.

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ::mavlink::ardupilotmega::{MavMessage, PARAM_REQUEST_LIST_DATA, PARAM_REQUEST_READ_DATA};

use super::handler::MavlinkCommand;

const PARAM_TIMEOUT: Duration = Duration::from_secs(3);

fn pack_param_id(name: &str) -> [u8; 16] {
    let mut id = [0u8; 16];
    let b = name.as_bytes();
    let n = b.len().min(16);
    id[..n].copy_from_slice(&b[..n]);
    id
}

fn send(cmd_tx: &mpsc::Sender<MavlinkCommand>, msg: MavMessage) -> Result<(), String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    cmd_tx.send(MavlinkCommand::SendMessage { msg, reply: reply_tx })
        .map_err(|_| "MAVLink handler stopped".to_string())?;
    reply_rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| "MAVLink send timed out".to_string())?
}

/// Read the given parameters by name; returns the ones the FC actually reports (missing names are
/// simply absent — e.g. FENCE_* on PX4 or GF_* on ArduPilot). Best-effort, used to populate the
/// geofence panel's core-param controls.
pub fn read_params(
    cmd_tx: &mpsc::Sender<MavlinkCommand>,
    fc_sysid: u8,
    names: &[&str],
) -> HashMap<String, f32> {
    let (tx, rx) = mpsc::channel();
    if cmd_tx.send(MavlinkCommand::RegisterParamReceiver(tx)).is_err() {
        return HashMap::new();
    }
    std::thread::sleep(Duration::from_millis(10)); // let the handler pick up the registration

    let mut out: HashMap<String, f32> = HashMap::new();
    for &name in names {
        if send(cmd_tx, MavMessage::PARAM_REQUEST_READ(PARAM_REQUEST_READ_DATA {
            param_index: -1,
            target_system: fc_sysid,
            target_component: 1, // MAV_COMP_ID_AUTOPILOT1
            param_id: pack_param_id(name).into(),
        })).is_err() {
            continue;
        }
        let deadline = Instant::now() + PARAM_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() { break; }
            match rx.recv_timeout(remaining) {
                Ok(MavMessage::PARAM_VALUE(pv)) => {
                    let end = pv.param_id.iter().position(|&c| c == 0).unwrap_or(pv.param_id.len());
                    let pname: String = pv.param_id[..end].iter().map(|&c| c as char).collect();
                    out.insert(pname.clone(), pv.param_value);
                    if pname == name { break; } // got the one we asked for
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
    let _ = cmd_tx.send(MavlinkCommand::UnregisterParamReceiver);
    out
}

/// One FC parameter as downloaded (QGroundControl `.params` interop + the Parameters panel).
#[derive(Clone, serde::Serialize)]
pub struct ParamEntry {
    pub name: String,
    pub value: f32,
    /// MAVLink `MAV_PARAM_TYPE` (1 = int8 … 9 = real32, 10 = real64).
    pub param_type: u8,
}

/// Result of a full parameter download: what arrived, and whether it is the whole set.
#[derive(serde::Serialize)]
pub struct ParamDownload {
    pub params: Vec<ParamEntry>,
    pub expected: u16,
    pub complete: bool,
}

/// Download the FC's full parameter list (PARAM_REQUEST_LIST). Streams until every index has
/// been seen; when the stream stalls with gaps, the missing indices are re-requested individually
/// (up to three rounds). A partial result is returned rather than an error when the deadline
/// passes — the panel shows the count either way. Errors only when nothing arrives at all.
pub fn download_all_params(
    cmd_tx: &mpsc::Sender<MavlinkCommand>,
    fc_sysid: u8,
) -> Result<ParamDownload, String> {
    let (tx, rx) = mpsc::channel();
    if cmd_tx.send(MavlinkCommand::RegisterParamReceiver(tx)).is_err() {
        return Err("MAVLink handler stopped".to_string());
    }
    std::thread::sleep(Duration::from_millis(10)); // let the handler pick up the registration
    let result = collect_params(cmd_tx, fc_sysid, &rx);
    let _ = cmd_tx.send(MavlinkCommand::UnregisterParamReceiver);
    result
}

fn collect_params(
    cmd_tx: &mpsc::Sender<MavlinkCommand>,
    fc_sysid: u8,
    rx: &mpsc::Receiver<MavMessage>,
) -> Result<ParamDownload, String> {
    send(cmd_tx, MavMessage::PARAM_REQUEST_LIST(PARAM_REQUEST_LIST_DATA {
        target_system: fc_sysid,
        target_component: 1, // MAV_COMP_ID_AUTOPILOT1
    }))?;

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut by_index: std::collections::BTreeMap<u16, ParamEntry> = std::collections::BTreeMap::new();
    let mut expected: u16 = 0;
    let mut last_msg = Instant::now();
    let mut rerequest_rounds = 0;

    loop {
        if expected > 0 && by_index.len() >= expected as usize {
            break;
        }
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        // Stream stalled with gaps → re-request the missing indices (bounded rounds).
        if expected > 0 && last_msg.elapsed() > Duration::from_secs(4) && by_index.len() < expected as usize {
            if rerequest_rounds >= 3 {
                break;
            }
            rerequest_rounds += 1;
            let mut sent = 0;
            for idx in 0..expected {
                if sent >= 50 {
                    break;
                }
                if !by_index.contains_key(&idx) {
                    sent += 1;
                    let _ = send(cmd_tx, MavMessage::PARAM_REQUEST_READ(PARAM_REQUEST_READ_DATA {
                        param_index: idx as i16,
                        target_system: fc_sysid,
                        target_component: 1,
                        param_id: pack_param_id("").into(),
                    }));
                }
            }
            last_msg = Instant::now();
            continue;
        }
        // Nothing at all after 10 s: the FC is not answering (not MAVLink / no params).
        if expected == 0 && last_msg.elapsed() > Duration::from_secs(10) {
            return Err("no parameters received — the flight controller did not answer the parameter request".to_string());
        }
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(MavMessage::PARAM_VALUE(pv)) => {
                last_msg = Instant::now();
                expected = pv.param_count;
                let end = pv.param_id.iter().position(|&c| c == 0).unwrap_or(pv.param_id.len());
                let name: String = pv.param_id[..end].iter().map(|&c| c as char).collect();
                by_index.insert(pv.param_index, ParamEntry {
                    name,
                    value: pv.param_value,
                    param_type: pv.param_type as u8,
                });
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    if by_index.is_empty() {
        return Err("no parameters received — the flight controller did not answer the parameter request".to_string());
    }
    let complete = expected > 0 && by_index.len() >= expected as usize;
    Ok(ParamDownload {
        params: by_index.into_values().collect(),
        expected,
        complete,
    })
}
