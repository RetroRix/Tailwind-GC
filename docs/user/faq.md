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
- **CartoDB Dark Matter** — a dark, low-contrast basemap.

All of them work **offline** once the tiles have been cached — see below.

### Can I use the maps offline?

Yes — two ways. Tailwind caches every map tile it downloads (**[Settings → Data → Map →
Tile Cache](reference/settings.md)**), so anything you pan and zoom over **while you still have an
internet connection** stays available offline afterwards. The default cache is 200 MB and can be
raised up to 5 GB.

For flying sites, use the **[Offline Maps](guides/offline-maps.md)** panel instead: pick a radius
around your field and a max zoom, and Tailwind downloads the whole area ahead of time, shows a
coverage bar per area, and can export an area as a **.mappack** file to share with other Tailwind
users — they import it and get the exact same coverage without downloading anything.

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
