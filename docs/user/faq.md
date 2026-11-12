# Frequently asked questions

## Maps

### Which map providers can I use?

Tailwind ships with several free map sources — no account or API key required. Pick one under
**[Settings → Interface → Map → Tile Provider](reference/settings.md)**; the choice applies to **both
the 2D map and the 3D globe**:

- **OpenStreetMap** — the standard street map.
- **ESRI World Street Map** — an alternative street map.
- **ESRI World Imagery** — satellite / aerial imagery.
- **ESRI Hybrid** — satellite imagery with street and place labels on top (the default).
- **OpenTopoMap** — topographic map with contour lines.
- **ESRI Dark Gray** — a dark, low-contrast basemap.

All of them work **offline** once the tiles have been cached — see below. On Windows, Linux and macOS
the list also offers your own map tiles from a folder on disk — see **[Custom maps](guides/custom-maps.md)**.

### Can I use the maps offline?

Yes — two ways. Tailwind caches every map tile it downloads (**[Settings → Data → Map →
Tile Cache](reference/settings.md)**), so anything you pan and zoom over **while you still have an
internet connection** stays available offline afterwards. The default cache is 200 MB and can be
raised up to 5 GB.

For flying sites, use the **[Offline Maps](guides/offline-maps.md)** panel instead: pick a radius
around your field and a max zoom, and Tailwind downloads the whole area ahead of time, shows a
coverage bar per area, and can export an area as a **.mappack** file to share with other Tailwind
users — they import it and get the exact same coverage without downloading anything.

On the desktop there is a second way: map tiles you already have can be put into a folder and shown as
a **[custom map](guides/custom-maps.md)**, straight from disk and without any connection. Kite does not
download those tiles for you.

### Why isn't Google Maps or HERE Maps available?

This comes up because other ground stations (Mission Planner, QGroundControl) show Google satellite
imagery. We looked at both Google and HERE carefully and deliberately chose **not** to integrate them.
The short version: for a ground control station the downsides outweigh a marginal difference in map
style, and the satellite imagery is **not actually better** than what Tailwind already offers.

**Google Maps.** The way other apps show Google imagery relies on **undocumented tile endpoints** that
Google's Terms of Service explicitly forbid accessing outside the official Maps Platform. It works only
because Google doesn't bother enforcing it against small projects — it can be blocked at any time, and
it is not something we want to build into an open-source application. The **official** Google Maps Tiles
API, by contrast, requires every user to create a billing-enabled API key, forbids caching imagery for
offline use, and mandates Google branding that can't be removed. Neither path fits an offline-capable
UAV tool.

**HERE Maps.** HERE requires a per-user developer account and API key, its Acceptable Use Policy
restricts "high-risk" use, and — like Google — it does not permit pre-caching tiles for offline use.
On top of that, a direct comparison showed **no quality benefit**: HERE's satellite tiles are the same
mid-zoom imagery as ESRI, it lacks the highest zoom levels ESRI provides, and its low-zoom imagery is
the older Bing basemap that ESRI already serves in better quality.

In both cases the extra setup (account, API key, billing) and the licensing constraints simply aren't
worth it for a map style you can already get, key-free, from the built-in providers. If a specific,
properly licensed provider ever adds real value, we would add it as an optional *bring-your-own-key*
source rather than route around anyone's terms of service.

## Flying & connecting

### Why did Tailwind connect by itself when I plugged in my flight controller?

That's **USB Auto-Connect** (Settings → **Connection**, on by default): when a known flight
controller — CubePilot, Hex/3DR, pid.codes or STM32-based boards — is plugged in over USB while
Tailwind is disconnected, it connects automatically at your last-used protocol and baud rate. Each
plug-in gets one attempt, and a manual disconnect never re-triggers it. Generic USB-serial adapters
(FTDI/CP210x/CH340 dongles) are ignored on purpose. Turn the setting off if you'd rather always
connect by hand — see [Connecting](guides/connecting.md#usb-auto-connect).

### Why does the AOA widget show no data?

The **AOA** widget reads ArduPilot's angle-of-attack telemetry, which only exists when the aircraft
has an **airspeed sensor** fitted and calibrated — no airspeed sensor, no AOA. (Airspeed from a
pitot is also what makes the widget's **LOW SPD** warning meaningful.) On an aircraft without one,
leave the widget out of your dock; GPS ground speed is not a substitute — it lies in wind, which is
exactly why AOA exists. See the widget table in
[Telemetry & display](guides/telemetry-and-display.md).

### Can I put my favourite flight modes on the main screen?

Yes — **pinned flight modes** put one-tap mode buttons in the status bar. Tailwind starts with
Loiter, Guided and Auto pinned. To change them, open the **Control** panel's **Modes** section,
tick **Pin modes to status bar**, then tap the modes you want pinned. Details in
[Vehicle control](guides/vehicle-control.md#pinned-flight-modes).

### Why is the status bar quieter — and where did the firmware chatter go?

Since v1.1.0-tailwind.6 the **System messages** feed (Settings → **General**) defaults to
**Warning** and above, so day-to-day flying only shows warnings and errors. The firmware's
developer chatter — ArduPilot's `MAV_SEVERITY_DEBUG` lines such as "Sending unknown message (44)"
— is now its own **Debug** level: pick it when you're troubleshooting on the bench and want the
full firehose. Your existing setting is kept; only fresh installs get the new Warning default.

## Flight controller setup

### Can Tailwind edit flight controller parameters?

Only the ones that belong to a mission or to its safety net. Tailwind is a **mission planner, mission
manager and flight monitor** — not a configurator. Setting up the aircraft itself (ports, receiver,
modes, tuning, sensors, outputs) stays with the tool made for your firmware: **INAV Configurator** for
INAV, **Mission Planner** or **QGroundControl** for ArduPilot and PX4.

What Tailwind does write to the flight controller is everything you plan on the map:

- **Missions** — waypoints and their actions, on INAV, ArduPilot and PX4 ([Missions](guides/missions.md)).
- **INAV geozones** — each zone with its altitude band and breach action
  ([Safety → Geozones](guides/safety.md#inav-geozones)).
- **INAV safe homes and autoland** — the safe-home slots, their approach settings and the global
  autoland configuration ([Safety → Safe Home & Autoland](guides/safety.md#automatic-landing-safe-home-autoland-inav)).
- **ArduPilot / PX4 geofence** — the fence geometry plus the core fence parameters such as enable,
  breach action and altitude / distance limits ([Safety → Geofence](guides/safety.md#ardupilot-px4-geofence)).
- **ArduPilot / PX4 rally points** — the points and their parameters
  ([Safety → Rally points](guides/safety.md#rally-points-ardupilot-px4)).
- A few single settings where they belong to a workflow: the **loiter radius** from
  [Vehicle control](guides/vehicle-control.md) on ArduPilot / PX4, and the INAV **craft name** from the
  [Vehicle library](guides/vehicles.md), so that future flights link to the right vehicle.

Nothing is written in the background: a change reaches the flight controller only when you save it or
press the matching button, and only the settings that editor shows.

A general **parameter editor** — the full list of every firmware setting — is not ruled out for the
future, but it sits far down the priority list. Each firmware already has a dedicated configurator that
does this well, and a second, half-complete one inside a ground station adds risk without adding much.
