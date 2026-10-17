<p align="center">
  <img alt="Tailwind Ground Control" src=".github/tailwind-logo.png" width="220">
</p>

<h1 align="center">Tailwind Ground Control</h1>

<p align="center"><b>Go far. Get home.</b></p>

<p align="center">A cross-platform ground control station for INAV, ArduPilot &amp; PX4 — with a desert-flying spin: an El Paso home map and a one-hold <b>GET HOME</b> button.</p>

<p align="center">
  <a href="LICENSE"><img alt="License: GPL-3.0-or-later" src="https://img.shields.io/badge/License-GPLv3-blue.svg"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-555">
  <a href="https://github.com/RetroRix/Tailwind-GC/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/RetroRix/Tailwind-GC?label=release&color=59aa29"></a>
</p>

---

## What Tailwind adds

Tailwind Ground Control is a rebrand and customisation of [Kite GC](#based-on-kite-gc) with its own identity and a growing set of personal touches:

- **🏜️ El Paso home map** — the map opens on El Paso, Texas (Franklin Mountains country) instead of a generic default.
- **🟧 GET HOME button** — a big orange tile in the bottom dock with your **live distance-to-home** on its face. Hold it for a second to send **RTL (Return to Launch)**. It stays inactive until a vehicle is connected *and* armed, so it can't be triggered on the bench.
- **🎨 Tailwind identity** — name, tagline ("Go far. Get home."), and a custom logo: a front-angle RC plane over the El Paso skyline and the Franklin Mountains, star and all.

More Tailwind touches are in the works.

## Highlights

- **🧊 Immersive 3D flight view** — a full 3D globe with real terrain, your aircraft and track in 3D, a 3D mission overlay, an FPV cockpit camera and real-time day/night lighting; switch 2D ⇄ 3D seamlessly.
- **🚁 One GCS for INAV, ArduPilot & PX4** — plan, fly and log across all three autopilots with a consistent interface, including passive (listen-only) and relay link modes.
- **🏭 Fleet, Battery & Mission managers** — keep a library of your aircraft and batteries with full build sheets and lifetime stats, plus a reusable mission library — all linked to your flight log.
- **⚡ Fast & intuitive** — a performance-oriented interface with dockable widgets and panels that remembers your layout, so the focus stays on flying.

## The essentials

- **Live telemetry & HUD** — attitude, altitude, speed (incl. airspeed), a compass with wind and ground-track indicators, GPS/sensor health, link quality and flight-mode display.
- **Customisable widget dashboard** — drag-and-drop flight widgets docked to the side and bottom.
- **2D moving map** — aircraft, track, home and mission, with heading-up mode and day/night shading.
- **Mission planning** — create, upload, download and edit missions; undo/redo; a survey-pattern generator; terrain-following / AGL waypoints.
- **Vehicle control** — arm/disarm, flight-mode changes, takeoff/RTL/loiter and more (ArduPilot/PX4).
- **Flight Logbook** — automatic recording with replay, plus import of INAV blackbox, ArduPilot Dataflash, MAVLink `.tlog` and MWPTools-compatible raw-MSP logs.
- **Safety suite** — geofences (ArduPilot/PX4), geozones (INAV), safe-home & fixed-wing autoland, airspace overlays and foreign-vehicle radar with ADS-B proximity & conflict alerts.
- **Live video** — a local capture device (webcam / USB capture card) or a low-latency RTSP stream, shown alongside (or behind) the map.
- **Comfort** — a multi-language interface (English, German, French, Bulgarian and Chinese) with persistent window, layout and settings between sessions.

## Supported setups

- **Autopilots:** INAV (7.0+), ArduPilot, PX4.
- **Aircraft:** fixed-wing, flying-wing, VTOL, multirotor, helicopter, rover, boat.
- **Connections:** USB / serial, Bluetooth (SPP & BLE), TCP, UDP.
- **Link modes:** live control link, **passive** listen-only telemetry, or a **relay** that re-broadcasts to other ground stations.
- **Platforms:** Windows, macOS (universal) and Linux (x86-64 / ARM64). Android and iOS / iPadOS are in upstream development.

## Download

Grab the latest installer for your platform from the
[**Releases**](https://github.com/RetroRix/Tailwind-GC/releases) page, or [build from source](#building-from-source) below.

> **Windows note:** Tailwind builds are unsigned, so Windows SmartScreen will warn on first run — choose **More info → Run anyway**.

## Building from source

### Prerequisites
- [Node.js](https://nodejs.org/) LTS (v20 or v24)
- [Rust](https://rustup.rs/) (via rustup)
- [just](https://github.com/casey/just) — the primary task runner
- Platform toolchain & system dependencies — see the upstream [Build Guide](https://b14ckyy.github.io/Kite-GC/for-developers/building/)

### Develop
```bash
npm install      # one-time
just dev         # start with hot reload  (alt: npm run tauri dev)
```

### Build
```bash
just build           # current platform   (alt: npm run tauri build)
just build-windows   # Windows release
just build-macos     # macOS release (on macOS)
just build-linux     # Linux release (on Linux)
```

## Based on Kite GC

Tailwind Ground Control is a fork of **[Kite GC](https://github.com/b14ckyy/Kite-GC)** by
Marc Hoffmann ([b14ckyy](https://github.com/b14ckyy)) — the original project, its documentation
([b14ckyy.github.io/Kite-GC](https://b14ckyy.github.io/Kite-GC/)) and its community deserve the
credit for the heavy lifting. Tailwind keeps the same GPL license and open source code, per the
license terms.

## License

[GPL-3.0-or-later](LICENSE) — Copyright © 2026 Marc Hoffmann ([b14ckyy](https://github.com/b14ckyy)); Tailwind Ground Control modifications © 2026 [RetroRix](https://github.com/RetroRix).
