// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Marc Hoffmann (b14ckyy)

// Offline map areas + .mappack export/import (Tailwind).
//
// An "area" is a named rectangle of map tiles for one provider, downloaded ahead of
// time into the regular IndexedDB tile cache so the field laptop/phone works with no
// signal. Areas are metadata only (bounds + provider + zoom range) — the tiles live in
// the cache like any browsed tile, so LRU eviction applies to them too; the panel's
// coverage % is the honest, recountable truth of what's actually on disk.
//
// A .mappack is a plain zip: `manifest.json` (the area entries + the cache URL of
// every tile) plus `tiles/<provider>/<z>/<y>/<x>.<ext>` in the same layout as the
// on-disk caches, TitanPlanner-style, so a pack can also be unpacked by hand.

import { writable, get } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import {
  hasCachedTile,
  getCachedTileData,
  putCachedTile,
  deleteCachedTile,
  tileCacheStats,
} from "$lib/cache/tileCache";
import { loadTileBytes } from "$lib/cache/tileLoader";
import { buildZip, readZip } from "$lib/cache/zipStore";

export interface LatLonBounds {
  north: number;
  south: number;
  east: number;
  west: number;
}

export interface OfflineArea {
  id: string;
  name: string;
  providerId: string;
  urlTemplate: string;
  bounds: LatLonBounds;
  minZoom: number;
  maxZoom: number;
  createdAt: number;
}

const STORAGE_KEY = "tailwind-offline-areas";
/** Refuse absurd downloads (a whole state at z18) — the panel explains the cap. */
export const MAX_AREA_TILES = 20000;

function loadAreas(): OfflineArea[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) return JSON.parse(raw) as OfflineArea[];
  } catch {
    // fall through to empty
  }
  return [];
}

export const offlineAreas = writable<OfflineArea[]>(loadAreas());

function persist(list: OfflineArea[]): void {
  offlineAreas.set(list);
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list));
  } catch {
    // storage full/blocked — areas survive in memory for this session
  }
}

export function addArea(area: OfflineArea): void {
  persist([...get(offlineAreas), area]);
}

export function removeArea(id: string): void {
  persist(get(offlineAreas).filter((a) => a.id !== id));
}

export function newAreaId(): string {
  return `area-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

/** Bounds of a square-ish area of `radiusKm` around a center point. */
export function boundsAround(center: [number, number], radiusKm: number): LatLonBounds {
  const [lat, lon] = center;
  const dLat = radiusKm / 111.32;
  const dLon = radiusKm / (111.32 * Math.max(0.2, Math.cos((lat * Math.PI) / 180)));
  return {
    north: Math.min(85, lat + dLat),
    south: Math.max(-85, lat - dLat),
    east: lon + dLon,
    west: lon - dLon,
  };
}

// ── Slippy-map tile math ────────────────────────────────────────────

function lonToTileX(lon: number, z: number): number {
  return Math.floor(((lon + 180) / 360) * 2 ** z);
}

function latToTileY(lat: number, z: number): number {
  const rad = (lat * Math.PI) / 180;
  return Math.floor(((1 - Math.log(Math.tan(rad) + 1 / Math.cos(rad)) / Math.PI) / 2) * 2 ** z);
}

export interface TileRef {
  z: number;
  x: number;
  y: number;
  url: string;
}

/** Fill a provider URL template exactly the way the live map layer does (Leaflet
 *  default subdomains "abc", `{r}` dropped) so downloaded tiles hit the same cache keys. */
export function buildTileUrl(template: string, x: number, y: number, z: number): string {
  const sub = "abc"[(x + y + z) % 3];
  return template
    .replace("{s}", sub)
    .replace("{z}", String(z))
    .replace("{x}", String(x))
    .replace("{y}", String(y))
    .replace("{r}", "");
}

export function enumerateTiles(area: Pick<OfflineArea, "bounds" | "minZoom" | "maxZoom" | "urlTemplate">): TileRef[] {
  const out: TileRef[] = [];
  for (let z = area.minZoom; z <= area.maxZoom; z++) {
    const n = 2 ** z;
    const x0 = Math.max(0, lonToTileX(area.bounds.west, z));
    const x1 = Math.min(n - 1, lonToTileX(area.bounds.east, z));
    const y0 = Math.max(0, latToTileY(area.bounds.north, z));
    const y1 = Math.min(n - 1, latToTileY(area.bounds.south, z));
    for (let x = x0; x <= x1; x++) {
      for (let y = y0; y <= y1; y++) {
        out.push({ z, x, y, url: buildTileUrl(area.urlTemplate, x, y, z) });
      }
    }
  }
  return out;
}

export function countTiles(bounds: LatLonBounds, minZoom: number, maxZoom: number): number {
  return enumerateTiles({ bounds, minZoom, maxZoom, urlTemplate: "" }).length;
}

// ── Coverage / download ─────────────────────────────────────────────

export async function areaCoverage(area: OfflineArea): Promise<{ present: number; total: number }> {
  const tiles = enumerateTiles(area);
  let present = 0;
  const CHUNK = 64;
  for (let i = 0; i < tiles.length; i += CHUNK) {
    const slice = tiles.slice(i, i + CHUNK);
    const results = await Promise.all(slice.map((t) => hasCachedTile(t.url)));
    present += results.filter(Boolean).length;
  }
  return { present, total: tiles.length };
}

export interface DownloadProgress {
  done: number;
  total: number;
  downloaded: number;
  skipped: number;
  failed: number;
}

export interface CancelFlag {
  cancelled: boolean;
}

export async function downloadArea(
  area: OfflineArea,
  onProgress: (p: DownloadProgress) => void,
  cancel: CancelFlag,
): Promise<DownloadProgress> {
  if (get(tileCacheStats).maxBytes === 0) {
    throw new Error("cache-disabled");
  }
  const tiles = enumerateTiles(area);
  const progress: DownloadProgress = { done: 0, total: tiles.length, downloaded: 0, skipped: 0, failed: 0 };
  let next = 0;

  async function worker(): Promise<void> {
    while (!cancel.cancelled) {
      const idx = next++;
      if (idx >= tiles.length) return;
      const tile = tiles[idx];
      try {
        if (await hasCachedTile(tile.url)) {
          progress.skipped++;
        } else {
          const bytes = await loadTileBytes(tile.url, area.urlTemplate);
          await putCachedTile(tile.url, bytes);
          progress.downloaded++;
        }
      } catch {
        progress.failed++;
      }
      progress.done++;
      onProgress({ ...progress });
    }
  }

  await Promise.all(Array.from({ length: 8 }, () => worker()));
  return progress;
}

/** Delete an area record, optionally deleting its tiles too (tiles shared with other
 *  areas or normal browsing go as well — coverage recounts tell the truth afterwards). */
export async function deleteArea(area: OfflineArea, withTiles: boolean): Promise<void> {
  if (withTiles) {
    for (const tile of enumerateTiles(area)) {
      await deleteCachedTile(tile.url);
    }
  }
  removeArea(area.id);
}

// ── .mappack export / import ────────────────────────────────────────

interface PackManifest {
  format: "tailwind-mappack";
  version: 1;
  areas: OfflineArea[];
  tiles: { path: string; url: string }[];
}

function tileExt(url: string): string {
  const m = /\.(png|jpe?g)(?:[?#]|$)/i.exec(url);
  return m ? `.${m[1].toLowerCase()}` : "";
}

function tilePath(area: OfflineArea, tile: TileRef): string {
  return `tiles/${area.providerId}/${tile.z}/${tile.y}/${tile.x}${tileExt(tile.url)}`;
}

const CHUNK_BYTES = 3 * 1024 * 1024; // multiple of 3 → clean base64 chunk boundaries

function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  const STEP = 0x8000;
  for (let i = 0; i < bytes.length; i += STEP) {
    bin += String.fromCharCode(...bytes.subarray(i, i + STEP));
  }
  return btoa(bin);
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

async function writeFileInChunks(path: string, data: Uint8Array): Promise<void> {
  for (let off = 0; off < data.length; off += CHUNK_BYTES) {
    const chunk = data.subarray(off, Math.min(data.length, off + CHUNK_BYTES));
    await invoke("binary_file_write_chunk", {
      path,
      chunkBase64: bytesToBase64(chunk),
      append: off > 0,
    });
  }
  if (data.length === 0) {
    await invoke("binary_file_write_chunk", { path, chunkBase64: "", append: false });
  }
}

async function readFileInChunks(path: string): Promise<Uint8Array> {
  const size = await invoke<number>("binary_file_size", { path });
  const out = new Uint8Array(size);
  for (let off = 0; off < size; off += CHUNK_BYTES) {
    const len = Math.min(CHUNK_BYTES, size - off);
    const b64 = await invoke<string>("binary_file_read_chunk", { path, offset: off, len });
    out.set(base64ToBytes(b64), off);
  }
  return out;
}

/** Export every cached tile of exactly this area as a .mappack file. */
export async function exportArea(
  area: OfflineArea,
  path: string,
  onProgress: (done: number, total: number) => void,
): Promise<{ packed: number; missing: number }> {
  const tiles = enumerateTiles(area);
  const manifest: PackManifest = { format: "tailwind-mappack", version: 1, areas: [area], tiles: [] };
  const entries: { path: string; data: Uint8Array }[] = [];
  let packed = 0;
  let missing = 0;
  for (let i = 0; i < tiles.length; i++) {
    const tile = tiles[i];
    const data = await getCachedTileData(tile.url);
    if (data) {
      const p = tilePath(area, tile);
      manifest.tiles.push({ path: p, url: tile.url });
      entries.push({ path: p, data: new Uint8Array(data) });
      packed++;
    } else {
      missing++;
    }
    onProgress(i + 1, tiles.length);
  }
  const manifestBytes = new TextEncoder().encode(JSON.stringify(manifest, null, 1));
  const zip = buildZip([{ path: "manifest.json", data: manifestBytes }, ...entries]);
  await writeFileInChunks(path, zip);
  return { packed, missing };
}

/** Import a .mappack: seed its tiles into the cache and register its areas. */
export async function importPack(
  path: string,
  onProgress: (done: number, total: number) => void,
): Promise<{ areas: string[]; tiles: number }> {
  if (get(tileCacheStats).maxBytes === 0) {
    throw new Error("cache-disabled");
  }
  const bytes = await readFileInChunks(path);
  const entries = await readZip(bytes);
  const manifestEntry = entries.find((e) => e.path === "manifest.json");
  if (!manifestEntry) throw new Error("no-manifest");
  const manifest = JSON.parse(new TextDecoder().decode(manifestEntry.data)) as PackManifest;
  if (manifest.format !== "tailwind-mappack" || !Array.isArray(manifest.tiles)) {
    throw new Error("bad-manifest");
  }
  const byPath = new Map(entries.map((e) => [e.path, e.data]));
  let done = 0;
  for (const t of manifest.tiles) {
    const data = byPath.get(t.path);
    if (data) {
      await putCachedTile(t.url, data.buffer as ArrayBuffer);
    }
    done++;
    onProgress(done, manifest.tiles.length);
  }
  const existing = new Set(get(offlineAreas).map((a) => a.id));
  const importedNames: string[] = [];
  for (const a of manifest.areas ?? []) {
    addArea(existing.has(a.id) ? { ...a, id: newAreaId() } : a);
    importedNames.push(a.name);
  }
  return { areas: importedNames, tiles: manifest.tiles.length };
}
