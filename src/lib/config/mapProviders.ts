// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Map tile provider definitions for Leaflet
// All providers listed here are free and require no API key.

export interface MapProvider {
  id: string;
  label: string;
  url: string;
  attribution: string;
  maxZoom: number;
  /** Highest zoom the server has real tiles for, when lower than maxZoom — the 2D map enlarges
   *  those tiles above it instead of requesting levels that do not exist. */
  maxNativeZoom?: number;
  /** Max zoom for CesiumJS 3D view — lower than maxZoom for providers with sparse high-zoom coverage */
  cesiumMaxZoom?: number;
  /** Optional overlay layer URLs rendered on top of the base layer (e.g. labels on satellite) */
  overlays?: { url: string; attribution: string; maxZoom: number; maxNativeZoom?: number; cesiumMaxZoom?: number }[];
  /** True when the base imagery is dark (satellite, dark theme) — used for UI contrast */
  dark?: boolean;
  /** Run over-zoom placeholder detection on this base layer (ESRI satellite
   *  returns a fixed blank tile instead of a 404 above the available zoom). */
  detectPlaceholders?: boolean;
  /** Set on a custom (local tile folder) map — see `src/lib/stores/customMaps.ts`. */
  custom?: CustomMapSpec;
}

/** A custom map: one folder under the custom-map root, holding XYZ tiles from `minZoom` to `maxZoom`. */
export interface CustomMapSpec {
  /** Folder name (= label and the part after `custom:` in the provider id). */
  name: string;
  minZoom: number;
  maxZoom: number;
  /** Tile pixel size detected by the backend from one tile's header — one size for the whole map. */
  tileSize: TileSize;
}

/** A tile pixel size: custom maps are drawn at the size their tiles really are. */
export type TileSize = 128 | 256 | 512;

/** Normalise a detected tile pixel size to the nearest supported one (128 / 256 / 512);
 *  anything unreadable or non-finite becomes 256. */
export function normalizeTileSize(n: unknown): TileSize {
  if (typeof n !== "number" || !Number.isFinite(n) || n <= 0) return 256;
  const exp = Math.max(7, Math.min(9, Math.round(Math.log2(n))));
  return (1 << exp) as TileSize;
}

/** Leaflet layer options for a custom (offline folder) map whose tiles are `tileSize` px and whose
 *  highest FILE level is `maxLevel` (the folder names; the same level shift upstream applies to its
 *  user online maps — file level = map zoom + `zoomOffset`, so a 512 px map's level Z is native at
 *  map zoom Z+1, a 128 px one's at Z−1), and over-zoom up to two map zooms past the highest native
 *  one (at least 20). `minNativeZoom` / `maxNativeZoom` are MAP zooms (Leaflet's GridLayer compares
 *  them with the map zoom before the offset is added for the URL level). */
export function customLayerZoom(
  maxLevel: number,
  tileSize: TileSize,
): {
  tileSize: number;
  zoomOffset: number;
  maxNativeZoom: number;
  minNativeZoom?: number;
  maxZoom: number;
  className: string;
} {
  const zoomOffset = Math.round(Math.log2(256 / tileSize)); // 512 → −1, 256 → 0, 128 → +1
  const maxNativeZoom = Math.max(0, maxLevel - zoomOffset);
  return {
    tileSize,
    // Leaflet puts it on the layer container: Map.svelte's seam rules (tile-overlap / tile-snap)
    // force a tile size and need the real one for 512 / 128 px tiles.
    className: `kite-tiles-${tileSize}`,
    zoomOffset,
    maxNativeZoom,
    ...(zoomOffset < 0 ? { minNativeZoom: -zoomOffset } : {}),
    maxZoom: Math.max(20, maxNativeZoom + 2),
  };
}

export const MAP_PROVIDERS: MapProvider[] = [
  {
    id: "osm",
    label: "OpenStreetMap",
    url: "https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png",
    attribution:
      '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
    maxZoom: 19,
  },
  {
    id: "esri-street",
    label: "ESRI World Street Map",
    url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Street_Map/MapServer/tile/{z}/{y}/{x}",
    attribution:
      '&copy; <a href="https://www.esri.com/">Esri</a> &mdash; Sources: Esri, DeLorme, NAVTEQ',
    maxZoom: 20,
    cesiumMaxZoom: 17,
  },
  {
    id: "esri-satellite",
    label: "ESRI World Imagery",
    url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
    attribution:
      '&copy; <a href="https://www.esri.com/">Esri</a> &mdash; Sources: Esri, Maxar, Earthstar Geographics',
    maxZoom: 20,
    // Raised from 17 → 20 now that placeholder detection falls back per-region.
    cesiumMaxZoom: 20,
    dark: true,
    detectPlaceholders: true,
  },
  {
    id: "esri-hybrid",
    label: "ESRI Hybrid",
    url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
    attribution:
      '&copy; <a href="https://www.esri.com/">Esri</a> &mdash; Sources: Esri, Maxar, Earthstar Geographics',
    maxZoom: 20,
    // Raised from 17 → 20 now that placeholder detection falls back per-region.
    cesiumMaxZoom: 20,
    dark: true,
    detectPlaceholders: true,
    overlays: [
      {
        url: "https://server.arcgisonline.com/ArcGIS/rest/services/Reference/World_Boundaries_and_Places/MapServer/tile/{z}/{y}/{x}",
        attribution: "",
        maxZoom: 20,
        cesiumMaxZoom: 17,
      },
      {
        url: "https://server.arcgisonline.com/ArcGIS/rest/services/Reference/World_Transportation/MapServer/tile/{z}/{y}/{x}",
        attribution: "",
        maxZoom: 20,
        cesiumMaxZoom: 17,
      },
    ],
  },
  {
    id: "opentopomap",
    label: "OpenTopoMap",
    url: "https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png",
    attribution:
      '&copy; <a href="https://opentopomap.org">OpenTopoMap</a> (<a href="https://creativecommons.org/licenses/by-sa/3.0/">CC-BY-SA</a>)',
    maxZoom: 17,
  },
  {
    // Replaces "CartoDB Dark Matter" (id cartodb-dark): CARTO's basemap servers answer every tile
    // with an "API KEY REQUIRED" image since November 2026.
    id: "esri-dark",
    label: "ESRI Dark Gray",
    url: "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Base/MapServer/tile/{z}/{y}/{x}",
    attribution:
      '&copy; <a href="https://www.esri.com/">Esri</a> &mdash; Sources: Esri, HERE, Garmin, &copy; OpenStreetMap contributors',
    // Real tiles end at z16; above that the server sends a "Map data not yet available" image.
    maxZoom: 20,
    maxNativeZoom: 16,
    cesiumMaxZoom: 16,
    dark: true,
    overlays: [
      {
        url: "https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/World_Dark_Gray_Reference/MapServer/tile/{z}/{y}/{x}",
        attribution: "",
        maxZoom: 20,
        maxNativeZoom: 16,
        cesiumMaxZoom: 16,
      },
    ],
  },
];

/** Provider ids of earlier versions that no longer exist → their replacement. */
const REPLACED_PROVIDER_IDS: Record<string, string> = {
  "cartodb-dark": "esri-dark",
};

/** The id to use for a stored provider id (maps removed providers to their replacement). */
export function currentProviderId(id: string): string {
  return Object.hasOwn(REPLACED_PROVIDER_IDS, id) ? REPLACED_PROVIDER_IDS[id] : id;
}

export const DEFAULT_PROVIDER_ID = "osm";

// ── Custom maps (dynamic entries, desktop only) ─────────────────────────────────────────────────
// The static list above never changes; the custom maps found in the custom-map folder are kept in
// this registry (filled by `stores/customMaps.ts` after each scan) and resolved by the same lookup.

/** Provider-id prefix of a custom map: `custom:<folder name>`. */
export const CUSTOM_PROVIDER_PREFIX = "custom:";
/** Pseudo URL scheme the tile loader routes to the local-file command instead of the network. */
export const CUSTOM_TILE_SCHEME = "custom://";

let customProviders: MapProvider[] = [];

export function isCustomProviderId(id: string): boolean {
  return id.startsWith(CUSTOM_PROVIDER_PREFIX);
}

/** Pseudo URL template of a custom map's tiles (the folder name is URI-encoded, so a `{`, `/` or `%`
 *  in it cannot collide with the placeholders or the path). */
export function customTileTemplate(name: string): string {
  return `${CUSTOM_TILE_SCHEME}${encodeURIComponent(name)}/{z}/{x}/{y}`;
}

/** True for a custom map's (template or filled-in) tile URL — local, never cached in IndexedDB. */
export function isCustomTileUrl(url: string): boolean {
  return url.startsWith(CUSTOM_TILE_SCHEME);
}

/** Parse a filled-in custom tile URL (`custom://<map>/<z>/<x>/<y>`); `null` if it is not one. */
export function parseCustomTileUrl(url: string): { map: string; z: number; x: number; y: number } | null {
  if (!url.startsWith(CUSTOM_TILE_SCHEME)) return null;
  const parts = url.slice(CUSTOM_TILE_SCHEME.length).split("/");
  if (parts.length !== 4) return null;
  const [z, x, y] = parts.slice(1).map((p) => (/^\d+$/.test(p) ? Number(p) : NaN));
  if (!Number.isInteger(z) || !Number.isInteger(x) || !Number.isInteger(y)) return null;
  let map: string;
  try { map = decodeURIComponent(parts[0]); } catch { return null; }
  return map ? { map, z, x, y } : null;
}

/** Build the provider entry of a custom map. No attribution (the user's own tiles). The 2D zoom range
 *  reaches the higher of 20 (like the built-in providers) and two map zooms above the one that shows
 *  the map's highest level natively; above that Leaflet scales the top level up (`maxNativeZoom`, see
 *  `customLayerZoom`). The 3D view stops at the highest level that exists (Cesium levels are file
 *  levels; the tile size goes to Cesium as `tileWidth`/`tileHeight`). */
export function customProvider(spec: CustomMapSpec): MapProvider {
  return {
    id: `${CUSTOM_PROVIDER_PREFIX}${spec.name}`,
    label: spec.name,
    url: customTileTemplate(spec.name),
    attribution: "",
    maxZoom: customLayerZoom(spec.maxZoom, spec.tileSize).maxZoom,
    cesiumMaxZoom: spec.maxZoom,
    custom: spec,
  };
}

/** Replace the registered custom maps (called by the custom-map store after a scan). */
export function setCustomProviders(list: MapProvider[]): void {
  customProviders = list;
}

/** The registered custom maps, in folder-name order. */
export function getCustomProviders(): MapProvider[] {
  return customProviders;
}

/** Resolve a provider id — built-in or custom. An unknown id (e.g. a custom map whose folder is gone)
 *  falls back to the first built-in provider. */
export function getProviderById(id: string): MapProvider {
  const current = currentProviderId(id);
  return (
    MAP_PROVIDERS.find((p) => p.id === current) ??
    customProviders.find((p) => p.id === id) ??
    MAP_PROVIDERS[0]
  );
}
