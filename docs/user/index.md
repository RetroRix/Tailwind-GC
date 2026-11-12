<p align="center" markdown>
  ![Tailwind Ground Control](assets/branding/tailwind-icon.png){ width="220" }
</p>

<p align="center" markdown>
**Go far. Get home.**
</p>

**Tailwind Ground Control (Tailwind GC)** is a modern, cross-platform ground control station for
**INAV**, **ArduPilot**, and **PX4** aircraft — planes, multirotors, VTOL, helicopters, rovers and
boats. It combines everything you expect from a GCS with a fast, intuitive interface and a few things
you won't find anywhere else — like a full 3D flight view, a fleet & battery manager, and live video
right next to the map.

![Tailwind Ground Control in 3D mode](assets/main_interface_3d.png)
/// caption
Tailwind Ground Control flying in 3D mode.
///

## What's new in Tailwind v1.1.0-tailwind.13

Four improvements ported from upstream Kite GC's development line:

-   **Your own map by URL template.** Under **Custom Online Provider** (Settings) you can
    add a map as a tile URL template (`{z}`/`{x}`/`{y}`), test it in place, and then pick
    it like any built-in provider — on the 2D map, the 3D globe, the mission preview and
    the video backdrop. It sits alongside the folder-based custom maps from v1.1.0-tailwind.11
    and the Offline Maps panel.
-   **Settings in three tabs.** The Settings panel is reorganized into **Interface**,
    **Map** and **Data** tabs. Every setting from before is in its new home — nothing was
    removed.
-   **A healthier flight database.** **Compact Database** now runs in the background
    instead of freezing the app, and reports how much space it freed; unused space is
    also reclaimed quietly in the background between flights. Importing several log
    files at once now continues past one that can't be read.
-   **Lighter 3D.** With **High-Resolution 3D** off, the globe loads one imagery level
    coarser — less bandwidth and memory on modest hardware.

## What's new in Tailwind v1.1.0-tailwind.12

A maintenance release — four fixes ported from upstream Kite GC's 1.0.2 line:

-   **Custom maps are drawn at their real tile size.** A folder map's tile size (128, 256 or
    512 px) is detected automatically from a tile header, and the map is drawn at that size: a
    512 px map appears one map zoom higher with its labels at their intended size, a 128 px map
    one lower. The Debug Monitor's Custom maps tab shows the detected size. 256 px maps are
    unchanged.
-   **A page reload can no longer strand a connection.** In release builds the reload keys
    (F5, Ctrl/Cmd+R) are blocked — a reload used to restart only the frontend while the backend
    stayed connected, leaving no way to disconnect short of quitting. If Connect ever does meet
    such a leftover link, it now offers to close it and connect again; a flight being recorded
    on it is kept.
-   **Requests identify the app correctly.** Map tiles, terrain, weather, geocoding, ADS-B,
    OpenAIP and the helper-program downloads now send Tailwind's real app version in the
    User-Agent instead of hard-coded, stale ones.
-   **More complete translations** — several hundred French, Bulgarian and Chinese strings
    that previously showed in English are translated.
-   Documentation corrected: **Airspeed** and **Radar** are on by default (the settings
    reference and the radar guide said the opposite).

Get it: [Windows + all platforms](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.12) ·
[Android APK](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.12-android.1)

## What's new in Tailwind v1.1.0-tailwind.11

New: **custom maps — offline tiles from a local folder** (desktop). Point Tailwind at a folder
in Settings → Map → Custom map location, and every subfolder becomes a map in the Tile Provider
list, right after the built-in providers:

-   Tiles use the usual `Z/X/Y` folder layout (.png / .jpg / .jpeg / .webp, any tile size —
    256, 512, …) and work across the 2D map, the 3D globe, the mission preview and the video
    backdrop — with no internet, and without going through the online tile cache.
-   Gaps are filled gracefully: a missing tile is drawn from the nearest lower zoom, and one
    level below a map's lowest level still draws from its children.
-   Each map's covered area is framed on the map ("Show map area" in Settings), backed by a
    background scan whose result is cached next to the tiles as `.kite-coverage.json`; the
    Debug Monitor's Custom maps tab shows the scan details.
-   Desktop only — the entries are hidden on Android. 3D terrain still needs internet; only
    the imagery is local. MBTiles files are not supported yet.

Full walkthrough: [Custom maps](guides/custom-maps.md).

Get it: [Windows + all platforms](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.11) ·
[Android APK](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.11-android.1)

## What's new in Tailwind v1.1.0-tailwind.10

Hotfix: **the dark map works again.** CARTO's basemap servers now answer every *CartoDB Dark
Matter* tile with an "API KEY REQUIRED" image, so the dark map showed that text instead of a
map in every earlier version. The dark map is now **ESRI Dark Gray** — the same ESRI service
as the other maps, with its label layer on top:

-   If you had CartoDB Dark Matter selected, Tailwind switches you to ESRI Dark Gray by
    itself — nothing to do.
-   ESRI's dark tiles end at zoom level 16. Zoom in further and the map enlarges the zoom-16
    tiles instead of pulling the provider's "Map data not yet available" placeholders (the
    3D globe likewise stops at 16).

Verified in the running app before release: the dark map renders El Paso with labels, and at
maximum zoom it shows enlarged real tiles — no placeholder or "API KEY" tiles anywhere.

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.10)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.10-android.1)**

## What's new in Tailwind v1.1.0-tailwind.9

Rick's three picks from upstream development, backported — the mode panel now shows what your
flight controller really offers, and Blackbox import loses its training wheels:

-   **The Control panel offers the modes your ArduPilot reports** — on ArduPilot 4.7+ the
    flight controller reports its own mode list, and the panel builds its grid from it: modes
    your firmware doesn't have disappear, custom and newer modes show up. Fixed-wing planes
    get a dedicated **Takeoff** button, QuadPlane gets its full landing set (QRTL, QLand,
    Loiter-to-Q-Land, AutoLand), and confirmation modes arm by hold-to-confirm. **Pinned modes
    keep working** — a pin for a mode the FC doesn't report drops out; with no reported list
    the built-in table is used exactly as before. The Debug Monitor gains an **FC modes** tab
    showing the reported list next to the panel's table. Older firmware and INAV unaffected.
-   **INAV Blackbox logs decode natively** — no more downloading the external
    `blackbox_decode` helper: the decoder is built in, imports are faster, and Blackbox logs
    now import on mobile too. The external tool remains as a fallback via the
    `TAILWIND_BLACKBOX_LEGACY=1` environment variable (the Settings decoder row only appears
    in that mode). The Log Viewer's record store is unchanged.
-   **Small stuff** — the second battery widget reads **BAT 2**; an empty network host field
    shows a faded **localhost** hint, and connecting with it empty (or typing "localhost")
    connects to this machine — handy for SITL.

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.9)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.9-android.1)**

## What's new in Tailwind v1.1.0-tailwind.8

The upstream 1.0.2 maintenance wave, ported — no flight is lost to a failed save, and rover,
boat and submarine modes are finally named (and sent) correctly:

-   **A flight whose save fails is kept, not dropped** — saving a recorded flight used to
    consume the session: if the write to the logbook failed (database busy with a long import,
    disk full), the flight silently vanished. Now a failed save keeps the recording and
    protects its temp file; the post-flight summary shows the error with a **Retry** button.
    The same protection covers the "connection lost" save, the startup recovery prompts, and
    the automatic save on re-arm. (Porting this also surfaced and fixed a bug in Tailwind's
    own crash recovery: summarizing a recovered session could fail against its temp database.)
-   **ArduSub gets its own mode table; Rover / Boat / Sub modes named correctly** — surface
    and underwater vehicles were classified with the Copter mode table (a Rover *Hold* showed
    as "Guided", a Sub *Manual* as "Avoid ADSB"), and the Sub's mode buttons sent Rover
    numbers. Mode names, the mode grid and the S.Port passthrough decoder now follow the
    actual vehicle. Copter also gains mode 28 *Turtle*; Plane gains 16 *Initialising* and
    26 *Autoland*.
-   **`.tlog` import and Log Viewer follow the first autopilot only** — on a telemetry link
    shared by several vehicles, only the first autopilot's frames make up the flight; another
    vehicle's positions and attitudes no longer interleave into one garbled track.
-   **Dialog fix** — confirm dialogs (vehicle delete, survey truncation) no longer show two
    **Cancel** buttons.

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.8)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.8-android.1)**

## What's new in Tailwind v1.1.0-tailwind.7

The rivals release — the best ideas from the other ground stations, built into Tailwind:

-   **Survey patterns, finished** — draw the survey polygon by **clicking on the map**, fly it as a
    **crosshatch** (both directions) as well as a grid, compute line spacing from your **camera**
    (sensor + focal length + overlap), and append to the mission or replace it. A new **Terrain**
    tab profiles the ground under the whole mission and can **rewrite waypoint altitudes to hold
    the clearance you set** over high ground — raising only, never lowering.
-   **NTRIP / RTK corrections** — a built-in NTRIP client (**Settings → Connection**) streams RTCM
    corrections from your caster and forwards them to the aircraft over MAVLink. No second app
    babysitting the RTK feed.
-   **Traffic on the map** — ADS-B targets show **callsign + altitude** right on the 2D map, the
    flight controller's own ADS-B-in over the main link is displayed too, and an optional
    **internet traffic feed (adsb.fi)** — off by default — adds surrounding traffic tagged **NET**.
-   **Log Viewer** — open any ArduPilot **`.bin`** or **`.tlog`**, graph **any logged field**, see
    the track coloured by flight mode, and **replay** the flight with a scrubbing playhead.
-   **FPV video (wfb-ng)** — a new video source listens for **RTP/H.264 on UDP 5600** (the
    wfb-ng/OpenHD receive contract), with a **ground-rendered OSD** that stays readable while the
    picture breaks up and a **link-health strip** graphing bitrate, RTP loss and telemetry signal.
-   **Connection presets** — one-click **DroneBridge (ESP32)** and **ExpressLRS MAVLink** presets
    in the connection bar. The GPS widget also gains a **JAM** readout for GPS jamming level when
    the firmware provides it (a dash when it doesn't).

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.7)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.7-android.1)**

## What's new in Tailwind v1.1.0-tailwind.6

Caught up with the upstream Kite GC fixes — the app is more honest about dead links and GPS quality, and ArduPilot editing stops fighting you:

-   **Link honesty** — if the flight controller goes quiet on an open link, the status bar says
    **"Reconnecting…"** after 3 s instead of sitting silent; a real transport loss tears the
    connection down cleanly, and a UDP peer's "port unreachable" counts as a stall, not a lost link.
-   **One GPS fix scale** — fix quality (0 none / 1 2D / 2 3D / 3 DGPS-RTK) is now the same on
    every link, and the ground station's own location falls back to the FC's home instead of being
    captured from the aircraft's first fix.
-   **ArduPilot mission editor** — leaving the mission tab ends ArduPilot edit mode, the waypoint
    popup no longer pans the waypoint under the side panel, and the raw-telemetry popup stops
    mislabelling an ArduPilot link as INAV.
-   **Fence / rally / geozone saves** — only changed parameters are written (a parameter the FC
    doesn't take is now a warning, not a failed save), and a failed save is shown under the Save
    button instead of failing silently.
-   **Quieter system messages** — the status feed defaults to **Warning** and above; a new
    **Debug** level surfaces the firmware's developer chatter when you're troubleshooting.
-   **Android log sharing** — "Share log file" lists the per-day log files so you can share exactly
    the right day's log. **SITL managers** (in `tools/simulators/`) run ArduPilot and INAV SITL
    without Mission Planner, for bench-testing without the real airframe.

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.6)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.6-android.1)**

## What's new in Tailwind v1.1.0-tailwind.5

Four additions aimed at one goal — Tailwind as your all-in-one ground station:

-   **Offline Maps** — download a map area around your flying site before you lose signal, watch its
    coverage, and share it as a **.mappack** file. → [Offline Maps guide](guides/offline-maps.md)
-   **AOA widget** — a fixed-wing stall instrument with a colour stall scale, flashing **STALL** and
    **LOW SPD** warnings (needs an airspeed sensor). → [Telemetry & display](guides/telemetry-and-display.md)
-   **Pinned flight modes** — your go-to modes as one-tap buttons in the status bar.
    → [Vehicle control](guides/vehicle-control.md#pinned-flight-modes)
-   **USB auto-connect** — plug in a Cube or Pixhawk and Tailwind connects by itself.
    → [Connecting](guides/connecting.md#usb-auto-connect)

Get it: **[Windows](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.5)** ·
**[Android](https://github.com/RetroRix/Tailwind-GC/releases/tag/v1.1.0-tailwind.5-android.1)**

## Highlights

<div class="grid cards" markdown>

-   :material-cube-scan:{ .lg .middle } **Immersive 3D flight view**

    ---

    A full 3D globe with real terrain, your aircraft and track in 3D, a 3D mission overlay, an FPV
    cockpit camera, and real-time day/night lighting — switch between 2D and 3D seamlessly.

-   :material-quadcopter:{ .lg .middle } **One GCS for INAV, ArduPilot & PX4**

    ---

    Plan, fly, and log across all three autopilots with a consistent interface — including passive
    (listen-only) and relay link modes.

-   :material-warehouse:{ .lg .middle } **Fleet, Battery & Mission managers**

    ---

    Keep a library of your aircraft and batteries with full build sheets and lifetime stats, and a
    reusable mission library — all linked to your flight log.

-   :material-flash:{ .lg .middle } **Fast & intuitive**

    ---

    A performance-oriented interface with dockable widgets and panels that remembers your layout, so
    the focus stays on flying.

</div>

## Supported setups

- **Autopilots:** INAV (7.0+), ArduPilot, and PX4.
- **Aircraft:** fixed-wing, flying-wing, VTOL, multirotor, helicopter, rover, boat.
- **Connections:** USB / serial, Bluetooth (SPP & BLE), TCP, and UDP.
- **Link modes:** live control link, **passive** listen-only telemetry, or a **relay** that
  re-broadcasts to other ground stations.

## The essentials

Everything you'd expect from a ground station:

- **Live telemetry & HUD** — attitude, altitude, speed (incl. airspeed), a compass with wind and
  ground-track indicators, GPS/sensor health, link quality, and flight-mode display.
- **Customisable widget dashboard** — drag-and-drop flight widgets docked to the side and bottom.
- **2D moving map** — your aircraft, track, home and mission, with heading-up mode and day/night
  shading.
- **Mission planning** — create, upload, download and edit missions; undo/redo; survey-pattern
  generator; terrain-following / AGL waypoints.
- **[Vehicle control](guides/vehicle-control.md)** — arm/disarm, flight-mode changes, takeoff/RTL/loiter and more (ArduPilot/PX4).
- **Comfort** — a multi-language interface (English, German, French, Chinese and Bulgarian; more
  community translations welcome) and
  persistent window, layout and settings between sessions.

## What makes Tailwind special

- **Full 3D mode** — Cesium 3D globe with real terrain, a unified 3D mission overlay, an FPV cockpit
  camera with a conformal HUD, and live day/night lighting.
- **Terrain awareness** — AGL (above-ground) waypoints, a terrain-profile analysis for your mission,
  and live *terrain radar* / AGL widgets in flight.
- **Flight Logbook** — a full flight logbook no other GCS offers: automatic recording with replay,
  plus import of INAV blackbox, **ArduPilot Dataflash**, MAVLink `.tlog`, and **MWPTools-compatible
  raw-MSP** logs — unified into one searchable flight history.
- **Fleet (Vehicle) Manager** — a build sheet per aircraft (airframe, propulsion, FC, sensors, photo)
  with lifetime statistics and records, auto-linked to your flights; export/import as `.kvehicle`.
- **Battery Manager** — track each pack by serial: cycles, lifetime usage and health, with
  `.kbatt` export/import.
- **Mission Manager** — a searchable mission library shared across autopilots.
- **Safety suite** — geofences (ArduPilot/PX4), geozones (INAV), safe-home & fixed-wing autoland,
  airspace overlays (airports, controlled airspace, obstacles), and **foreign-vehicle radar** with
  ADS-B proximity & conflict alerts.
- **Live video** — low-latency RTSP video shown alongside (or behind) the map, with one-click
  map ⇄ video swapping.
- **Telemetry relay** — re-encode and forward live telemetry to other ground stations, handsets, or an
  antenna tracker.
- **RC control** — fly from the GCS with a gamepad/joystick (HID).
- **RF link analysis** — visualise signal quality to find the best antenna setup.

## Where to start

- New here? Begin with **[Installation](getting-started/installation.md)**, then your
  **[first connection](getting-started/first-connection.md)** and the
  **[quick tour](getting-started/quick-tour.md)**.
- Having trouble connecting? See **[Troubleshooting → Connection](troubleshooting/connection.md)**.

## Community

Questions, setups, flight videos — Tailwind Ground Control is built by
[RetroRix](https://github.com/RetroRix) and flies with the **Rick Squared** crew
(RC planes, FPV, PPC & PPG). Bugs and feature ideas go to GitHub:

[:fontawesome-brands-github: Tailwind GC on GitHub](https://github.com/RetroRix/Tailwind-GC){ .md-button .md-button--primary }

See **[Reporting a problem](troubleshooting/reporting-issues.md)** for what to include in a bug
report. Tailwind GC is based on the open-source [Kite GC](https://github.com/b14ckyy/Kite-GC) by
Marc Hoffmann — the original project's documentation and community are the upstream home of the
codebase Tailwind builds on.

---

Tailwind GC is free, open-source software (GPL-3.0-or-later).
