# Offline Maps

Take the map with you. Download tile areas before you fly somewhere without signal — Tailwind
stores them in its regular map tile cache, so the 2D map (and 3D, where terrain allows) keeps
rendering when the network drops. Share downloaded areas with friends as **.mappack** files.

Open it from the **left rail → Offline Maps**. The panel shows the tile-cache status at the top
(how much of your Settings cache limit is used, and how many tiles are stored).

## Downloading an area

1. Pan the map to where you fly. The download is a rectangle **centred on the current map view**,
   using the active base-map provider — the panel tells you the centre coordinates and provider.
2. Pick a **Radius** and a **Max zoom**. Tailwind downloads zooms **11 up to your max**; it shows a
   live estimate of the tile count. Tiles grow fast — each zoom step down **quadruples** the count,
   so a 5 km radius at zoom 16 is a good first pack for a home field.
3. Hit **Download area**. A 20,000-tile safety cap blocks accidental downloads of half the county —
   if the estimate is over it, shrink the radius or max zoom and try again.

While downloading, a progress bar shows fetched tiles; **Cancel** stops it but keeps everything
fetched so far. Failed tiles are simply retried the next time you download the area. If the tile
cache is turned off (cache size 0 in Settings), the panel says so — offline areas need the cache on.

## Your downloaded areas

Each area lists its provider, radius and zoom range, a **coverage bar** (how many of its tiles are
actually in the cache — it recounts when you open the panel, or hit **Recount**), and three actions:

- **Export…** — writes a `.mappack` file: a portable pack of the area's cached tiles plus a manifest.
  Hand it to a flying buddy (chat, USB stick, whatever) and they import the exact same coverage.
- **Delete** — removes the area record. Tick **Also delete its tiles** to free the cache space too;
  leave it unticked to keep the tiles (other areas, or casual map browsing, may still use them).
- **Recount** — re-scans the cache for the area's tiles if the bar looks stale.

## Importing a .mappack

Hit **Import .mappack…**, pick the file, and Tailwind adds its tiles to your cache and lists its
areas. A pack's tiles count against the same tile-cache limit as downloads, and packs only work
with the base-map provider they were exported from.

!!! tip "For the desert sites"
    Download your areas **at home on Wi-Fi the night before** — map tiles over a phone hotspot in
    the field are how good intentions go to die. Pack one area per flying site and name the export
    after it (e.g. `red-sands-5km-z16.mappack`) so sharing stays obvious.
