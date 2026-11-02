// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

//! NTRIP client — RTK correction streaming (Tailwind).
//!
//! Connects to an NTRIP caster (Networked Transport of RTCM via Internet Protocol, a public
//! standard — NTRIP v1/v2 over HTTP/TCP), reads the RTCM3 stream continuously, and forwards each
//! complete RTCM frame to the connected MAVLink vehicle as `GPS_RTCM_DATA` (message 233), so the
//! vehicle's GPS can reach RTK float/fixed. Implemented clean-room from the public NTRIP / RTCM3 /
//! MAVLink specs.
//!
//! Ownership model mirrors the rest of the app: a managed service (`NtripService`) owns one worker
//! thread; the frontend pushes the persisted config with `ntrip_configure` on every settings change
//! (the `telemetry_api_configure` pattern), an unchanged config is a no-op. Stopping sets an atomic
//! flag the worker polls (socket reads run with a short timeout so shutdown is prompt) and joins the
//! thread; the app-exit handler calls `stop()` too.
//!
//! Forwarding goes through the app's existing outbound MAVLink path — `MavlinkCommand::SendMessage`
//! on the handler's command channel, the same path vehicle control (`commands::control`) and mission
//! upload use — so sequence numbering, tlog recording and link stats stay consistent. The worker
//! re-resolves the vehicle handle for every frame, so a vehicle (dis)connect mid-stream is picked up
//! automatically. While no MAVLink link is up (disconnected, or an MSP/INAV link, which has no RTCM
//! injection message) the client keeps receiving and counting, forwards nothing, and the status says
//! so (`vehicle_linked = false`) — corrections are never reported as flowing when they are not.
//!
//! The socket layer is plain `std::net` on a dedicated thread: the crate's tokio features
//! (`time`/`sync`/`macros`, no `net`/`rt`) don't provide a TCP client, and the MAVLink handler sets
//! the same std-thread precedent. No new dependencies: `base64` is already in the tree.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use ::mavlink::ardupilotmega::{MavMessage, GPS_RTCM_DATA_DATA};

use crate::mavlink_proto::handler::MavlinkCommand;
use crate::state::{ActiveProtocol, AppState};

/// Tauri event carrying `NtripStatus` to the frontend on every change.
pub const STATUS_EVENT: &str = "ntrip-status";

/// Default NTRIP caster port (IANA assignment for NTRIP).
pub const DEFAULT_PORT: u16 = 2101;

/// RTCM3 preamble byte.
const RTCM_PREAMBLE: u8 = 0xD3;
/// RTCM3 frame overhead: 3 header bytes + 3 CRC-24Q bytes.
const RTCM_OVERHEAD: usize = 6;
/// `GPS_RTCM_DATA.data` capacity — one fragment carries at most this many RTCM bytes.
const FRAGMENT_MAX: usize = 180;

/// First reconnect delay after a dropped/failed session; doubles up to `RECONNECT_MAX`.
const RECONNECT_MIN: Duration = Duration::from_secs(5);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
/// Socket connect timeout (also the worst case for a stop() join while connecting).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Socket read timeout — bounds how fast the worker notices the stop flag and emits status ticks.
const READ_TIMEOUT: Duration = Duration::from_millis(500);

// ── Config / status ───────────────────────────────────────────────────────────

/// `settings.ntrip`, pushed by the frontend on every change (idempotent configure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NtripConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub mountpoint: String,
    pub username: String,
    pub password: String,
}

/// Live client status — emitted as the `ntrip-status` event and returned by the commands.
/// `state` is one of `disabled` / `disconnected` / `connecting` / `streaming` / `error`;
/// `message` carries the error (state `error`) or an honest note (e.g. streaming with no vehicle
/// link). Counters are per worker run (reset when the client (re)starts).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtripStatus {
    pub state: String,
    pub message: Option<String>,
    pub bytes_received: u64,
    pub frames_received: u64,
    pub frames_forwarded: u64,
    pub vehicle_linked: bool,
}

impl NtripStatus {
    fn new(state: &str) -> Self {
        Self {
            state: state.to_string(),
            ..Default::default()
        }
    }
}

// ── Service (managed state) ───────────────────────────────────────────────────

struct Running {
    config: NtripConfig,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

/// Managed by Tauri. Owns the worker thread + the shared status snapshot.
pub struct NtripService {
    status: Arc<Mutex<NtripStatus>>,
    running: Mutex<Option<Running>>,
}

impl NtripService {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(NtripStatus::new("disabled"))),
            running: Mutex::new(None),
        }
    }

    pub fn status(&self) -> NtripStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| NtripStatus::new("error"))
    }

    /// Apply a config: start, stop, or restart the worker when it changed. Unchanged → no-op.
    fn configure(&self, app: AppHandle, cfg: NtripConfig) -> NtripStatus {
        let mut running = match self.running.lock() {
            Ok(r) => r,
            Err(_) => return self.status(),
        };
        if !cfg.enabled {
            stop_running(&mut running);
            self.set_status(&app, NtripStatus::new("disabled"));
            return self.status();
        }
        if running.as_ref().is_some_and(|r| r.config == cfg) {
            return self.status();
        }
        stop_running(&mut running);
        if cfg.host.trim().is_empty() || cfg.mountpoint.trim().is_empty() {
            let mut st = NtripStatus::new("error");
            st.message = Some("Host and mountpoint are required".to_string());
            self.set_status(&app, st);
            return self.status();
        }
        // Fresh counters for the new run.
        self.set_status(&app, NtripStatus::new("connecting"));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let app = app.clone();
            let cfg = cfg.clone();
            let stop = stop.clone();
            let status = self.status.clone();
            thread::spawn(move || worker(app, cfg, stop, status))
        };
        *running = Some(Running {
            config: cfg,
            stop,
            thread: Some(thread),
        });
        self.status()
    }

    /// Stop the worker (app exit). The final status is left as-is; the process is going away.
    pub fn stop(&self) {
        if let Ok(mut running) = self.running.lock() {
            stop_running(&mut running);
        }
    }

    fn set_status(&self, app: &AppHandle, st: NtripStatus) {
        if let Ok(mut cur) = self.status.lock() {
            *cur = st.clone();
        }
        let _ = app.emit(STATUS_EVENT, st);
    }
}

impl Default for NtripService {
    fn default() -> Self {
        Self::new()
    }
}

/// Signal the worker to stop and join it. The worker polls the flag at most every
/// `READ_TIMEOUT` (or every 200 ms while backing off); a connect in flight can add up to
/// `CONNECT_TIMEOUT` once.
fn stop_running(running: &mut Option<Running>) {
    if let Some(mut r) = running.take() {
        r.stop.store(true, Ordering::SeqCst);
        if let Some(t) = r.thread.take() {
            let _ = t.join();
        }
    }
}

// ── Worker ────────────────────────────────────────────────────────────────────

/// Worker-thread status writer: updates the shared snapshot and emits the event when the status
/// actually changed, or at least once a second while streaming (counter ticks).
struct StatusSink {
    app: AppHandle,
    status: Arc<Mutex<NtripStatus>>,
    last_emitted: Option<NtripStatus>,
    last_emit_at: Instant,
}

impl StatusSink {
    fn new(app: AppHandle, status: Arc<Mutex<NtripStatus>>) -> Self {
        Self {
            app,
            status,
            last_emitted: None,
            last_emit_at: Instant::now() - Duration::from_secs(1),
        }
    }

    fn update(&mut self, mutate: impl FnOnce(&mut NtripStatus)) {
        let snapshot = match self.status.lock() {
            Ok(mut cur) => {
                mutate(&mut cur);
                cur.clone()
            }
            Err(_) => return,
        };
        let due = self.last_emit_at.elapsed() >= Duration::from_secs(1);
        if self.last_emitted.as_ref() != Some(&snapshot) || due {
            let _ = self.app.emit(STATUS_EVENT, snapshot.clone());
            self.last_emitted = Some(snapshot);
            self.last_emit_at = Instant::now();
        }
    }

    fn set(&mut self, state: &str, message: Option<String>) {
        self.update(|s| {
            s.state = state.to_string();
            s.message = message;
        });
    }
}

fn worker(
    app: AppHandle,
    cfg: NtripConfig,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<NtripStatus>>,
) {
    let mut sink = StatusSink::new(app.clone(), status);
    let mut backoff = RECONNECT_MIN;
    // GPS_RTCM_DATA sequence id: increments per RTCM frame, mod 32. Lives for the whole worker run
    // (across reconnects) so the FC's reassembly never sees an ambiguous reuse mid-stream.
    let mut sequence: u8 = 0;

    while !stop.load(Ordering::SeqCst) {
        sink.set("connecting", None);
        match run_session(&app, &cfg, &stop, &mut sink, &mut sequence) {
            Ok(()) => {
                // Clean EOF (caster closed) — reconnect like any other drop.
                backoff = RECONNECT_MIN;
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                sink.set(
                    "error",
                    Some("Connection closed by caster — reconnecting".to_string()),
                );
            }
            Err(e) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                log::warn!("[ntrip] {e}");
                sink.set("error", Some(format!("{e} — reconnecting")));
            }
        }
        // Interruptible backoff: 5 s, doubling to a 30 s cap.
        let wait = backoff;
        backoff = (backoff * 2).min(RECONNECT_MAX);
        let mut waited = Duration::ZERO;
        while waited < wait && !stop.load(Ordering::SeqCst) {
            let step = Duration::from_millis(200);
            thread::sleep(step);
            waited += step;
        }
    }
    sink.set("disconnected", None);
    log::info!("[ntrip] client stopped");
}

/// One caster connection: handshake, then stream until EOF / error / stop.
/// `Ok(())` = the caster closed the stream cleanly; `Err` carries a displayable reason.
fn run_session(
    app: &AppHandle,
    cfg: &NtripConfig,
    stop: &Arc<AtomicBool>,
    sink: &mut StatusSink,
    sequence: &mut u8,
) -> Result<(), String> {
    let host = cfg.host.trim();
    let port = if cfg.port == 0 {
        DEFAULT_PORT
    } else {
        cfg.port
    };
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Cannot resolve {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("Cannot resolve {host}"))?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("Connect to {host}:{port} failed: {e}"))?;
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CONNECT_TIMEOUT));
    let _ = stream.set_nodelay(true);

    // NTRIP request (v2-style over HTTP/1.1; v1 casters accept the same request and answer ICY).
    let request = build_request(cfg, host, port);
    (&stream)
        .write_all(request.as_bytes())
        .map_err(|e| format!("Sending NTRIP request failed: {e}"))?;

    let mut reader = BufReader::new(stream);
    read_response_head(&mut reader)?;

    log::info!(
        "[ntrip] streaming from {}:{}/{}",
        host,
        port,
        cfg.mountpoint.trim().trim_start_matches('/')
    );
    sink.set("streaming", None);

    let mut parser = RtcmParser::new();
    let mut buf = [0u8; 4096];
    loop {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        match reader.read(&mut buf) {
            Ok(0) => return Ok(()), // EOF — caster closed the stream
            Ok(n) => {
                let frames = parser.push(&buf[..n]);
                let mut forwarded = 0u64;
                let mut linked = false;
                for frame in &frames {
                    match forward_frame(app, frame, sequence) {
                        ForwardOutcome::Forwarded => {
                            forwarded += 1;
                            linked = true;
                        }
                        ForwardOutcome::NoLink => linked = false,
                        ForwardOutcome::SendFailed(e) => {
                            // The MAVLink handler went away mid-frame (vehicle disconnected).
                            log::debug!("[ntrip] forward failed: {e}");
                            linked = false;
                        }
                    }
                }
                sink.update(|s| {
                    s.bytes_received += n as u64;
                    s.frames_received += frames.len() as u64;
                    s.frames_forwarded += forwarded;
                    // Only a batch with frames proves the link state; an empty batch leaves it
                    // to the read-timeout tick (≤ 500 ms behind) so it can't flap on chunking.
                    if !frames.is_empty() {
                        s.vehicle_linked = linked;
                    }
                    s.message = if s.vehicle_linked {
                        None
                    } else {
                        Some("No MAVLink vehicle link — receiving only, corrections are not reaching a vehicle".to_string())
                    };
                });
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Read timeout tick: refresh the link flag/message even between bursts, so the
                // status flips to "no vehicle link" promptly when the vehicle disconnects.
                let linked = link_present(app);
                sink.update(|s| {
                    s.vehicle_linked = linked;
                    s.message = if linked {
                        None
                    } else {
                        Some("No MAVLink vehicle link — receiving only, corrections are not reaching a vehicle".to_string())
                    };
                });
            }
            Err(e) => return Err(format!("NTRIP stream read failed: {e}")),
        }
    }
}

/// True while the active protocol is a MAVLink link (the only one that can take RTCM injection).
fn link_present(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let present = match state.protocol.lock() {
        Ok(p) => matches!(p.as_ref(), Some(ActiveProtocol::Mavlink(_))),
        Err(_) => false,
    };
    present
}

enum ForwardOutcome {
    Forwarded,
    NoLink,
    SendFailed(String),
}

/// Forward one complete RTCM frame as `GPS_RTCM_DATA` fragment(s) through the MAVLink handler's
/// command channel — the app's existing outbound path. Re-resolves the handle per frame.
fn forward_frame(app: &AppHandle, frame: &[u8], sequence: &mut u8) -> ForwardOutcome {
    let (cmd_tx, _fc_sysid) = {
        let state = app.state::<AppState>();
        let proto = match state.protocol.lock() {
            Ok(p) => p,
            Err(_) => return ForwardOutcome::NoLink,
        };
        match proto.as_ref() {
            Some(ActiveProtocol::Mavlink(h)) => (h.cmd_tx_clone(), h.fc_sysid),
            _ => return ForwardOutcome::NoLink,
        }
    };
    let fragments = fragment_rtcm(frame, *sequence);
    *sequence = (*sequence + 1) & 0x1F;
    for frag in fragments {
        let (reply_tx, reply_rx) = mpsc::channel();
        if cmd_tx
            .send(MavlinkCommand::SendMessage {
                msg: MavMessage::GPS_RTCM_DATA(frag),
                reply: reply_tx,
            })
            .is_err()
        {
            return ForwardOutcome::SendFailed("MAVLink handler stopped".to_string());
        }
        match reply_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return ForwardOutcome::SendFailed(e),
            Err(_) => return ForwardOutcome::SendFailed("MAVLink send timed out".to_string()),
        }
    }
    ForwardOutcome::Forwarded
}

// ── NTRIP request / response ──────────────────────────────────────────────────

/// Build the NTRIP GET request. The `Authorization` header is sent only when credentials are
/// configured (anonymous casters reject a bogus `Basic Og==` for ":"). The mountpoint is sent
/// without a leading slash duplication: `GET /<mountpoint>`.
pub fn build_request(cfg: &NtripConfig, host: &str, port: u16) -> String {
    let mount = cfg.mountpoint.trim().trim_start_matches('/');
    let mut req = format!(
        "GET /{mount} HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Ntrip-Version: Ntrip/2.0\r\n\
         User-Agent: Tailwind-GC/{} (NTRIP client)\r\n",
        env!("CARGO_PKG_VERSION")
    );
    if !cfg.username.is_empty() || !cfg.password.is_empty() {
        let creds = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", cfg.username, cfg.password));
        req.push_str(&format!("Authorization: Basic {creds}\r\n"));
    }
    req.push_str("Connection: close\r\n\r\n");
    req
}

/// Read and validate the caster's response head. Accepts `ICY 200` (v1 — the response is complete
/// with that single line, the stream follows immediately) and `HTTP/1.x 200` (v2 — header block
/// terminated by a blank line). Any bytes the buffered reader pre-fetched past the head stay in the
/// buffer and are returned by the caller's subsequent reads, so no RTCM data is lost.
fn read_response_head(reader: &mut BufReader<TcpStream>) -> Result<(), String> {
    let mut first = String::new();
    read_line_limited(reader, &mut first)
        .map_err(|e| format!("Reading caster response failed: {e}"))?;
    let first = first.trim_end();
    if first.is_empty() {
        return Err("Caster closed the connection without responding".to_string());
    }
    if first.starts_with("ICY 200") {
        return Ok(());
    }
    if first.starts_with("SOURCETABLE") {
        return Err(format!(
            "Mountpoint not found — caster returned its source table ({first})"
        ));
    }
    let is_http_200 = (first.starts_with("HTTP/1.1 200") || first.starts_with("HTTP/1.0 200"))
        || first.starts_with("HTTP/2 200");
    if !is_http_200 {
        if first.contains(" 401") || first.contains(" 403") {
            return Err(format!("Caster rejected the credentials ({first})"));
        }
        return Err(format!("Caster rejected the request ({first})"));
    }
    // HTTP 200: consume the header block up to the blank line (bounded).
    let mut total = 0usize;
    loop {
        let mut line = String::new();
        let n = read_line_limited(reader, &mut line)
            .map_err(|e| format!("Reading caster response failed: {e}"))?;
        total += n;
        if n == 0 || line == "\r\n" || line == "\n" || total > 16 * 1024 {
            return Ok(());
        }
    }
}

/// `BufRead::read_line` with the socket's read timeout temporarily cleared is not possible through
/// `BufReader`, so a would-block mid-head surfaces as an error — acceptable: a caster that stalls
/// its own response head is broken, and the worker reconnects with backoff.
fn read_line_limited(
    reader: &mut BufReader<TcpStream>,
    out: &mut String,
) -> std::io::Result<usize> {
    reader.read_line(out)
}

// ── RTCM3 framing ─────────────────────────────────────────────────────────────

/// Incremental RTCM3 frame parser. Feed arbitrary stream chunks; get back every complete frame
/// (header + payload + CRC, exactly as received — that whole frame is what gets forwarded).
/// Resync: scans for the 0xD3 preamble and verifies the CRC-24Q before accepting a frame, so a
/// stray 0xD3 inside a payload (or line noise) can't produce a bogus frame; on a CRC mismatch one
/// byte is dropped and scanning resumes.
pub struct RtcmParser {
    buf: Vec<u8>,
}

impl RtcmParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
        }
    }

    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            // Align to the preamble.
            match self.buf.iter().position(|&b| b == RTCM_PREAMBLE) {
                Some(0) => {}
                Some(pos) => {
                    self.buf.drain(..pos);
                }
                None => {
                    self.buf.clear();
                    break;
                }
            }
            if self.buf.len() < 3 {
                break; // header incomplete — wait for more bytes
            }
            // The 6 reserved bits after the preamble are always zero in RTCM3 — a non-zero value
            // means this 0xD3 is a payload byte / noise, not a header. Drop it and rescan now
            // instead of waiting for the bogus (up to 1029-byte) "frame" to fill.
            if self.buf[1] & 0xFC != 0 {
                self.buf.drain(..1);
                continue;
            }
            // Length: 6 reserved bits + 10-bit payload length.
            let payload_len = (((self.buf[1] & 0x03) as usize) << 8) | self.buf[2] as usize;
            let total = payload_len + RTCM_OVERHEAD;
            if self.buf.len() < total {
                break; // frame incomplete — wait for more bytes
            }
            let candidate: Vec<u8> = self.buf[..total].to_vec();
            let (body, crc_bytes) = candidate.split_at(total - 3);
            let expected =
                ((crc_bytes[0] as u32) << 16) | ((crc_bytes[1] as u32) << 8) | crc_bytes[2] as u32;
            if crc24q(body) == expected {
                frames.push(candidate);
                self.buf.drain(..total);
            } else {
                // False preamble (payload byte or noise): drop one byte and rescan.
                self.buf.drain(..1);
            }
        }
        frames
    }
}

/// RTCM3 CRC-24Q (polynomial 0x1864CFB, init 0, no reflection / final XOR) over `data`.
pub fn crc24q(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        crc ^= (byte as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x100_0000 != 0 {
                crc ^= 0x1864_CFB;
            }
        }
    }
    crc & 0xFF_FFFF
}

// ── GPS_RTCM_DATA fragmentation ───────────────────────────────────────────────

/// Split one RTCM frame into `GPS_RTCM_DATA` payloads per the message spec:
/// `data` holds at most 180 bytes; `flags` bit 0 = fragmented (frame larger than one fragment),
/// bits 1–2 = fragment id (the fragment's index within this frame, 0-based), bits 3–7 = sequence
/// id (per-RTCM-frame counter, mod 32 — supplied by the caller). Frames needing more than 4
/// fragments (> 720 bytes) wrap the 2-bit fragment id — the spec's reassembly model tops out at 4
/// fragments; large MSM frames that big are rare on 1 Hz caster streams and are a bench-test item.
pub fn fragment_rtcm(frame: &[u8], sequence: u8) -> Vec<GPS_RTCM_DATA_DATA> {
    let fragmented = frame.len() > FRAGMENT_MAX;
    let chunks: Vec<&[u8]> = if frame.is_empty() {
        vec![&frame[..0]]
    } else {
        frame.chunks(FRAGMENT_MAX).collect()
    };
    chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let mut data = [0u8; FRAGMENT_MAX];
            data[..chunk.len()].copy_from_slice(chunk);
            let flags = ((sequence & 0x1F) << 3)
                | (((i as u8) & 0x03) << 1)
                | if fragmented { 1 } else { 0 };
            GPS_RTCM_DATA_DATA {
                flags,
                len: chunk.len() as u8,
                data,
            }
        })
        .collect()
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// (Re)configure the NTRIP client — called on every `settings.ntrip` change and once at start-up
/// (which is what auto-starts the client on launch when it was left enabled).
#[tauri::command(async)]
pub fn ntrip_configure(
    app: AppHandle,
    service: State<'_, NtripService>,
    config: NtripConfig,
) -> NtripStatus {
    service.configure(app, config)
}

/// Current status snapshot for the settings row (the `ntrip-status` event is the live feed).
#[tauri::command]
pub fn ntrip_status(service: State<'_, NtripService>) -> NtripStatus {
    service.status()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> NtripConfig {
        NtripConfig {
            enabled: true,
            host: "caster.example".into(),
            port: 2101,
            mountpoint: "MSM".into(),
            username: "user".into(),
            password: "pass".into(),
        }
    }

    /// Build a valid RTCM3 frame around `payload` (correct CRC-24Q appended).
    fn rtcm_frame(payload: &[u8]) -> Vec<u8> {
        let len = payload.len();
        let mut f = vec![0xD3, ((len >> 8) & 0x03) as u8, (len & 0xFF) as u8];
        f.extend_from_slice(payload);
        let crc = crc24q(&f);
        f.extend_from_slice(&[(crc >> 16) as u8, (crc >> 8) as u8, crc as u8]);
        f
    }

    #[test]
    fn request_shape() {
        let req = build_request(&cfg(), "caster.example", 2101);
        assert!(req.starts_with("GET /MSM HTTP/1.1\r\n"));
        assert!(req.contains("Host: caster.example:2101\r\n"));
        assert!(req.contains("Ntrip-Version: Ntrip/2.0\r\n"));
        assert!(req.contains("Authorization: Basic dXNlcjpwYXNz\r\n")); // user:pass
        assert!(req.ends_with("\r\n\r\n"));
    }

    #[test]
    fn request_omits_auth_without_credentials() {
        let mut c = cfg();
        c.username.clear();
        c.password.clear();
        let req = build_request(&c, "caster.example", 2101);
        assert!(!req.contains("Authorization"));
    }

    #[test]
    fn parser_extracts_whole_frames_across_chunks() {
        let a = rtcm_frame(&[1, 2, 3]);
        let b = rtcm_frame(&vec![7u8; 300]);
        let mut stream = a.clone();
        stream.extend_from_slice(&b);
        let mut p = RtcmParser::new();
        let mut out = Vec::new();
        for chunk in stream.chunks(7) {
            out.extend(p.push(chunk));
        }
        assert_eq!(out, vec![a, b]);
    }

    #[test]
    fn parser_resyncs_after_garbage_and_payload_preamble() {
        // Payload contains a 0xD3 byte; garbage precedes the frame.
        let f = rtcm_frame(&[0xD3, 0x00, 0x55]);
        let mut p = RtcmParser::new();
        let mut data = vec![0x10, 0x22, 0xD3, 0xFF]; // stray preamble + junk
        data.extend_from_slice(&f);
        let out = p.push(&data);
        assert_eq!(out, vec![f]);
    }

    #[test]
    fn fragmentation_math() {
        // Small frame: single fragment, not fragmented, flags = seq << 3.
        let f = rtcm_frame(&[9u8; 10]); // 16 bytes total
        let frags = fragment_rtcm(&f, 5);
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0].flags, 5 << 3);
        assert_eq!(frags[0].len as usize, f.len());
        assert_eq!(&frags[0].data[..f.len()], &f[..]);

        // 400-byte frame: 180 + 180 + 40, fragment ids 0/1/2, fragmented bit set.
        let big = rtcm_frame(&vec![3u8; 394]); // 400 bytes total
        let frags = fragment_rtcm(&big, 31);
        assert_eq!(frags.len(), 3);
        assert_eq!(frags[0].len, 180);
        assert_eq!(frags[1].len, 180);
        assert_eq!(frags[2].len, 40);
        for (i, frag) in frags.iter().enumerate() {
            assert_eq!(frag.flags & 1, 1, "fragmented bit");
            assert_eq!((frag.flags >> 1) & 0x03, i as u8, "fragment id");
            assert_eq!(frag.flags >> 3, 31, "sequence id");
        }
        // Reassembly reproduces the frame.
        let mut joined = Vec::new();
        for frag in &frags {
            joined.extend_from_slice(&frag.data[..frag.len as usize]);
        }
        assert_eq!(joined, big);
    }
}
