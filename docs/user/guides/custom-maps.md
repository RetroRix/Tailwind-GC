# Custom maps

On the desktop — Windows, Linux and macOS — Kite can show map tiles from a **folder on your computer**
instead of an online tile provider. A custom map needs no internet connection: Kite reads the tiles
straight from disk, on the 2D map and on the 3D globe. You can keep several maps side by side and pick
one like any other tile provider.

Custom maps are not available on Android, iPhone and iPad; the selector entries, the Settings row and
the map frames are not shown there.

!!! note "Your tiles, your responsibility"
    Kite does not download tiles for a custom map — you put them into the folder yourself. You are
    responsible for having the right to use the tiles you store. The terms of many map providers do
    not allow downloading their tiles in bulk. Kite shows no attribution line for a custom map.

## The map folder

Kite reads all custom maps from one folder, the **custom map location**. By default that is a
`custom_maps` folder in Kite's app data folder:

| Windows (installed) | Linux (installed) | macOS (installed) | Portable |
|---|---|---|---|
| `%APPDATA%\kite-gc\custom_maps\` | `~/.local/share/kite-gc/custom_maps/` | `~/Library/Application Support/kite-gc/custom_maps/` | `<app>/data/custom_maps/` |

Kite creates the default folder if it does not exist. Moving the **Flight log database** to another
folder in Settings does not move it. To use a different folder — another drive, a USB stick, a network
share — pick it in Settings (see [below](#choosing-a-custom-map)).

### Layout

Every folder directly inside the location is **one map**, and its folder name is the name the map gets
in the selector. Inside a map folder the tiles follow the usual `Z/X/Y` layout: one folder per zoom
level, in it one folder per tile column (X), in that one image file per tile, named after its row (Y):

```text
custom_maps/
├── Home field/
│   ├── 15/
│   │   ├── 17000/
│   │   │   ├── 11000.png
│   │   │   └── 11001.png
│   │   └── 17001/
│   │       └── 11000.png
│   └── 16/
│       └── …
└── Coast survey/
    └── …
```

- **XYZ order, rows counted from the top** — the scheme OpenStreetMap uses: row 0 is the northern edge
  of the world. A tile set in **TMS** order (rows counted from the bottom) is not converted and lands
  in the wrong place on the map.
- The **zoom-level folders sit directly in the map folder**, with no extra folder in between. Zoom
  levels 0 to 30 are read.
- Folder and file names are **plain numbers**: `12`, not `012`.
- Tile files are **`.png`, `.jpg`, `.jpeg` or `.webp`**, in upper or lower case; one map may mix them.
  Other files are ignored.
- **One tile size per map**: 256 × 256 pixels, the usual size, or 512 × 512 or 128 × 128. Tailwind
  detects the size by itself — see [The map's tile size](#the-maps-tile-size).
- A map does not need every zoom level. It can start and end at any level and leave levels out.

Folders that Kite does **not** list as a map:

- a folder whose name starts with a dot (hidden folders),
- a folder whose name contains a colon `:` or a control character,
- a folder without any tile in the layout above (for example, tiles inside an extra subfolder).

Files lying directly in the location are ignored.

## Choosing a custom map

Open **[Settings → Interface → Map](../reference/settings.md#map)**:

- **Tile Provider** — the maps Kite found are at the **end of the list**, under **Custom maps**, each
  marked with **▦** and sorted by name. Pick one and it replaces the imagery on the 2D map and the 3D
  globe.
- **Custom map location** — the folder in use. **Choose** opens a folder picker; Kite reads the new
  folder at once and the selector lists its maps. **Use Default** goes back to the default folder (it
  is greyed out while the default is in use). Hover over the label for a short reminder of the layout.

Below the location Kite shows how many maps it found (**Scanning…** while it reads), plus two buttons:

- **Show map area** (only while a custom map is selected) moves the map to the area the map covers
  that is **nearest to the centre of the 2D map**. On the 2D map the follow mode switches to **Free**
  for this. In 3D the camera flies there in **Free** mode; in Follow, Orbit and FPV it stays with the
  aircraft. If the map holds no tiles, or its tiles cannot be read, a message appears under the row
  instead.
- **Rescan** reads the location again — see [Rescan](#after-adding-tiles-rescan).

Kite remembers the selected map by its **folder name**. When you change the location and the new one
has a map of the same name, Kite shows that one; otherwise the map shows **OpenStreetMap** until the
map is back or you pick another one.

## How a custom map is drawn

### The map's tile size

A custom map has no tile-size setting. When Tailwind reads the location, it reads the pixel size of
one tile per map: the first tile it finds in the map's **highest zoom level**. If that tile's size
cannot be read, Tailwind tries the first tile of the next lower level, and so on. The size counts as
the nearest of **128, 256 and 512 px** — 100 px tiles count as 128 px, 300 px tiles as 256 px,
1024 px tiles as 512 px; for a tile that is not square, the longer side decides. If no tile's size
can be read, the map counts as 256 px.

The whole map is then drawn with that size:

- **256 px** — the folder's level 15 is shown at map zoom 15.
- **512 px** — each tile is shown at 512 pixels, the folder's level 14 at map zoom 15. The same view
  needs a quarter as many tiles, and labels appear at the size they were drawn at.
- **128 px** — the opposite: each tile is shown at 128 pixels, the folder's level 16 at map zoom 15,
  and the same view needs four times as many tiles.

Tiles between the three sizes are stretched or shrunk a little to fit: 100 px tiles fill 128 pixels.

**One size per map.** The size Tailwind detects applies to every tile of the map; a folder that mixes
tile sizes is not supported. Tiles of another size still land in the right place, but squeezed or
stretched to the detected size, so their labels and details appear too small or too large. The size
is detected again whenever Tailwind reads the location — press [Rescan](#after-adding-tiles-rescan)
after replacing a map's tiles with ones of another size.

The 3D globe uses the tile size as well: it draws a 512 px map from one level lower.

### Missing tiles and zoom levels

Below, a **level** is one of the map's zoom-level folders. With 256 px tiles it is shown at the map
zoom of the same number; with 512 px tiles one map zoom higher, with 128 px tiles one lower.

- **Gaps are filled from lower levels.** Where a tile is missing, the 2D map enlarges the nearest
  lower-level tile that exists — however many levels down that is. A map with a coarse overview and a
  detailed patch therefore shows no holes around the patch.
- **One level below the lowest level** the 2D map still shows the map: Kite builds each tile from the
  (up to four) tiles of the lowest level, scaled down. Zoomed out further, the map is empty.
- **Outside the tiles the map is empty.** No online map fills in around a custom map.
- **Above the highest level** the 2D map enlarges the highest-level tiles and keeps zooming — up to
  map zoom 20, or two map zooms past the one that shows the highest level, if that is higher.
- **In 3D** the globe uses the same tiles up to the map's highest level. Where a tile is missing, the
  lower level stays visible. Below the map's lowest level the globe shows none of it — the "one level
  below" step is 2D only.

For example, a map with the levels 8 to 13 on the 2D map:

| Tile size | Levels 8–13 at full resolution | One level below (built from level 8) | Empty at map zoom | Highest level enlarged from map zoom |
|---|---|---|---|---|
| 256 px | map zoom 8–13 | map zoom 7 | 6 and lower | 14 |
| 512 px | map zoom 9–14 | map zoom 8 | 7 and lower | 15 |
| 128 px | map zoom 7–12 | map zoom 6 | 5 and lower | 13 |

In all three cases the 2D map zooms in to map zoom 20.

### Frames on the 2D map

A detailed patch of a few hundred metres is easy to lose on a large map. So while a custom map is
selected, the 2D map frames every **area** it covers (see the next section) with a thin blue line. The
frames are always shown, are not clickable and change with the zoom:

- **Dashed rectangle** — centred on the area and **four times its width and height**, shown while that
  rectangle fits into the map view. It points you to the area from further out.
- **Solid outline** — once you zoom in so far that the dashed rectangle would be wider or taller than
  the view, the frame turns into the outline of the area itself: exactly where its highest-resolution
  tiles are.
- **Small circle** — when you zoom out so far that the dashed rectangle would be only a few pixels
  across.

The 3D globe shows no frames.

### What counts as an area

An area is a patch of connected tiles. Kite looks at the map's **highest zoom level first**: every
separate patch of tiles there is an area. Each lower level adds areas only where it covers ground that
no higher level covers. In practice:

- A full pyramid around one airfield — from an overview of the whole world down to the detail level at
  the airfield — is **one** area: the airfield, not the world.
- Separate patches, such as two flying sites, are **separate areas**, each with its own frame, even when
  they end at different zoom levels.
- Small gaps or notches in the highest level (tiles a tool left out, for instance) do not get a frame of
  their own.
- Kite keeps at most **64 areas**, highest levels first. Further areas get no frame, and **Show map
  area** cannot jump to them; the diagnostics log then notes it.
- For very large or ragged areas the outline is drawn on a coarser grid. The frames are a help for
  finding the tiles, not an exact map of every tile.

## Practical notes

### The index file in the map folder

To draw the frames, Kite scans a map the first time it needs its areas. The scan only lists folders —
it never opens a tile — and its result goes into a small file, **`.kite-coverage.json`**, in the map's
folder. Kite reuses it as long as the map's folders are unchanged; adding or removing tiles changes the
folders, and Kite scans again. On a large map the first scan can take a moment; the tiles show
meanwhile and the frames appear when it is done.

When the map folder is **read-only** (a write-protected SD card, a read-only network share), the file
goes into a `custom_maps_index` folder in Kite's app data folder instead — for example
`%APPDATA%\kite-gc\custom_maps_index\` on Windows, or `<app>/data/custom_maps_index/` in portable mode.
You can delete the index file at any time; Kite builds it again.

### After adding tiles: Rescan

Tailwind reads the list of maps, their zoom levels and their tile sizes when it needs them, not
continuously. Press **Rescan** after you:

- add a new map folder or new zoom levels,
- connect the drive that holds the maps — a **USB stick** or **SD card** plugged in after Kite started,
- notice that new or changed tiles do not show up.

Rescan reads the location again, redraws the selected custom map and rebuilds its index file from
scratch.

### When the folder or the drive disappears

- If the drive goes away while a custom map is shown, tiles already on screen stay and new ones stay
  empty.
- The next time Kite reads the location (**Rescan**, **Choose**, or the next start) and the chosen
  folder is missing, an error appears under the row, the selector lists no custom maps and the map
  shows **OpenStreetMap**. When only the selected map's folder is gone, the other maps stay in the
  selector and the map shows OpenStreetMap as well. A missing default folder is simply created again.
- Your choice is kept: once the folder is back and Kite has read it again — **Rescan** or a restart —
  the custom map returns.

### Network folders

A location on a network share works, but every tile is a separate file read over the network, so the
map fills in more slowly than from a local disk. Some network shares do not report changes to their
folders; there the frames can stay out of date after you add tiles — press **Rescan**.

### Not part of the tile cache

Custom-map tiles are read from disk every time and never copied into Kite's **Tile Cache**. The cache
size and **Clear** in **Settings → Data** do not affect them, and a custom map does not use up cache
space.

## Where to go next

- Every map setting: **[Settings](../reference/settings.md#map)**.
- The 3D globe and its terrain: **[3D map](map-3d.md)**.
- Offline use of the online maps: **[FAQ](../faq.md#can-i-use-the-maps-offline)**.
