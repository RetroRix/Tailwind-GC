<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Offline Maps (Tailwind) — download map areas ahead of time for flying sites with no
     signal, see what's actually cached, and share areas as .mappack files. Areas are
     rectangles around the current map center for the active base-map provider; tiles go
     into the regular tile cache, so the cache size limit in Settings applies. -->
<script lang="ts">
  import { t } from 'svelte-i18n';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import PanelShell from '$lib/components/panel/PanelShell.svelte';
  import Button from '$lib/components/panel/Button.svelte';
  import { settings } from '$lib/stores/settings';
  import { getProviderById } from '$lib/config/mapProviders';
  import { tileCacheStats } from '$lib/cache/tileCache';
  import {
    offlineAreas, addArea, newAreaId, boundsAround, countTiles, areaCoverage,
    downloadArea, deleteArea, exportArea, importPack,
    MAX_AREA_TILES, type OfflineArea, type DownloadProgress, type CancelFlag,
  } from '$lib/cache/offlineAreas';

  const MIN_ZOOM = 11;

  let radiusKm = $state(5);
  let maxZoom = $state(16);
  let areaName = $state('');

  const provider = $derived(getProviderById($settings.mapProvider));
  const center = $derived($settings.map.center);
  const estTiles = $derived(countTiles(boundsAround(center, radiusKm), MIN_ZOOM, Math.min(maxZoom, provider.maxZoom)));
  const cacheOff = $derived($tileCacheStats.maxBytes === 0);

  let busy = $state('');
  let progress = $state<DownloadProgress | null>(null);
  let cancelFlag: CancelFlag = { cancelled: false };
  let message = $state('');
  let coverages = $state(new Map<string, { present: number; total: number }>());

  async function refreshCoverage() {
    const next = new Map<string, { present: number; total: number }>();
    for (const a of $offlineAreas) {
      next.set(a.id, await areaCoverage(a));
    }
    coverages = next;
  }

  // Coverage recount on open (the store subscription fires once immediately).
  $effect(() => {
    void $offlineAreas;
    void refreshCoverage();
  });

  function defaultName(): string {
    return `${provider.label} · ${radiusKm} km · ${new Date().toLocaleDateString()}`;
  }

  async function startDownload() {
    const zMax = Math.min(maxZoom, provider.maxZoom);
    if (estTiles > MAX_AREA_TILES) {
      message = $t('offline.tooBig', { values: { count: estTiles, max: MAX_AREA_TILES } });
      return;
    }
    const area: OfflineArea = {
      id: newAreaId(),
      name: areaName.trim() || defaultName(),
      providerId: provider.id,
      urlTemplate: provider.url,
      bounds: boundsAround(center, radiusKm),
      minZoom: MIN_ZOOM,
      maxZoom: zMax,
      createdAt: Date.now(),
    };
    addArea(area);
    busy = 'download';
    message = '';
    cancelFlag = { cancelled: false };
    progress = { done: 0, total: estTiles, downloaded: 0, skipped: 0, failed: 0 };
    try {
      const res = await downloadArea(area, (p) => (progress = p), cancelFlag);
      message = cancelFlag.cancelled
        ? $t('offline.downloadCancelled')
        : $t('offline.downloadDone', { values: { downloaded: res.downloaded, skipped: res.skipped, failed: res.failed } });
    } catch (e) {
      message = String(e) === 'cache-disabled' ? $t('offline.cacheDisabled') : String(e);
    } finally {
      busy = '';
      progress = null;
      void refreshCoverage();
    }
  }

  async function doExport(area: OfflineArea) {
    const path = await save({
      defaultPath: `${area.name.replace(/[^\w\- ]+/g, '').trim() || 'offline-area'}.mappack`,
      filters: [{ name: 'Map pack', extensions: ['mappack'] }],
    });
    if (!path) return;
    busy = `export:${area.id}`;
    message = '';
    try {
      const res = await exportArea(area, path, () => {});
      message = $t('offline.exportDone', { values: { packed: res.packed, missing: res.missing } });
    } catch (e) {
      message = String(e);
    } finally {
      busy = '';
    }
  }

  async function doImport() {
    const picked = await open({
      multiple: false,
      filters: [{ name: 'Map pack', extensions: ['mappack', 'zip'] }],
    });
    if (!picked || typeof picked !== 'string') return;
    busy = 'import';
    message = '';
    try {
      const res = await importPack(picked, () => {});
      message = $t('offline.importDone', { values: { areas: res.areas.join(', '), tiles: res.tiles } });
    } catch (e) {
      const s = String(e);
      message = s === 'cache-disabled' ? $t('offline.cacheDisabled')
        : s === 'no-manifest' ? $t('offline.noManifest')
        : s === 'bad-manifest' ? $t('offline.badManifest')
        : s;
    } finally {
      busy = '';
      void refreshCoverage();
    }
  }

  let confirmDelete = $state<string | null>(null);
  let deleteTiles = $state(false);

  async function doDelete(area: OfflineArea) {
    busy = `delete:${area.id}`;
    try {
      await deleteArea(area, deleteTiles);
    } finally {
      busy = '';
      confirmDelete = null;
      deleteTiles = false;
    }
  }

  const fmtMB = (b: number) => `${(b / (1024 * 1024)).toFixed(0)} MB`;
</script>

<PanelShell variant="compact" title={$t('offline.title')}>
  {#snippet body()}
    <div class="om">
      <p class="om-cache">
        {$t('offline.cacheStatus', { values: { used: fmtMB($tileCacheStats.usedBytes), max: fmtMB($tileCacheStats.maxBytes), count: $tileCacheStats.tileCount } })}
      </p>
      {#if cacheOff}
        <p class="om-warn">{$t('offline.cacheDisabled')}</p>
      {/if}

      <span class="om-sec">{$t('offline.newArea')}</span>
      <p class="om-hint">{$t('offline.centeredOn', { values: { lat: center[0].toFixed(4), lon: center[1].toFixed(4), provider: provider.label } })}</p>
      <div class="om-row">
        <label>{$t('offline.radius')}
          <select bind:value={radiusKm}>
            <option value={2}>2 km</option>
            <option value={5}>5 km</option>
            <option value={10}>10 km</option>
            <option value={25}>25 km</option>
          </select>
        </label>
        <label>{$t('offline.maxZoom')}
          <select bind:value={maxZoom}>
            {#each [14, 15, 16, 17, 18] as z}
              <option value={z}>z{z}</option>
            {/each}
          </select>
        </label>
      </div>
      <input class="om-name" type="text" bind:value={areaName} placeholder={defaultName()} />
      <p class="om-hint">{$t('offline.estimate', { values: { count: estTiles } })}</p>
      {#if busy === 'download' && progress}
        <div class="om-progress">
          <div class="om-bar"><div class="om-fill" style="width: {progress.total ? (progress.done / progress.total) * 100 : 0}%"></div></div>
          <span>{progress.done}/{progress.total}</span>
          <Button variant="danger" size="sm" onclick={() => (cancelFlag.cancelled = true)}>{$t('offline.cancel')}</Button>
        </div>
      {:else}
        <Button variant="data" full disabled={cacheOff || busy !== ''} onclick={startDownload}>⬇ {$t('offline.download')}</Button>
      {/if}

      <span class="om-sec">{$t('offline.areas')}</span>
      {#if $offlineAreas.length === 0}
        <p class="om-hint">{$t('offline.none')}</p>
      {/if}
      {#each $offlineAreas as area}
        {@const cov = coverages.get(area.id)}
        <div class="om-area">
          <div class="om-area-head">
            <span class="om-area-name">{area.name}</span>
            <span class="om-area-meta">z{area.minZoom}–{area.maxZoom}</span>
          </div>
          <div class="om-area-cov">
            {#if cov}
              {cov.present}/{cov.total} ({cov.total ? Math.round((cov.present / cov.total) * 100) : 0}%)
              <div class="om-bar"><div class="om-fill" style="width: {cov.total ? (cov.present / cov.total) * 100 : 0}%"></div></div>
            {:else}
              …
            {/if}
          </div>
          {#if confirmDelete === area.id}
            <div class="om-row">
              <label class="om-check"><input type="checkbox" bind:checked={deleteTiles} /> {$t('offline.deleteTiles')}</label>
              <Button variant="danger" size="sm" disabled={busy !== ''} onclick={() => doDelete(area)}>{$t('offline.deleteConfirm')}</Button>
              <Button variant="standard" size="sm" onclick={() => (confirmDelete = null)}>{$t('offline.cancel')}</Button>
            </div>
          {:else}
            <div class="om-row">
              <Button variant="standard" size="sm" disabled={busy !== ''} onclick={() => doExport(area)}>{$t('offline.export')}</Button>
              <Button variant="standard" size="sm" disabled={busy !== ''} onclick={() => refreshCoverage()}>{$t('offline.refresh')}</Button>
              <Button variant="danger" size="sm" disabled={busy !== ''} onclick={() => (confirmDelete = area.id)}>{$t('offline.delete')}</Button>
            </div>
          {/if}
        </div>
      {/each}

      <Button variant="standard" full disabled={busy !== ''} onclick={doImport}>📦 {$t('offline.import')}</Button>

      {#if message}
        <p class="om-msg">{message}</p>
      {/if}
    </div>
  {/snippet}
</PanelShell>

<style>
  .om { display: flex; flex-direction: column; gap: 8px; padding: 4px 2px; }
  .om-cache { margin: 0; font-size: 12px; color: #bbb; }
  .om-warn { margin: 0; font-size: 12px; color: #ffb52e; }
  .om-sec { margin-top: 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: #37a8db; }
  .om-hint { margin: 0; font-size: 12px; color: #888; }
  .om-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; font-size: 12px; color: #ccc; }
  .om-row select, .om-name {
    height: 26px; padding: 0 6px; background: #434343; border: 1px solid #555;
    border-radius: 4px; color: #e0e0e0; font-size: 12px;
  }
  .om-name { width: 100%; box-sizing: border-box; }
  .om-progress { display: flex; align-items: center; gap: 8px; font-size: 12px; color: #ccc; }
  .om-bar { flex: 1; height: 6px; background: rgba(255,255,255,0.12); border-radius: 3px; overflow: hidden; min-width: 60px; }
  .om-fill { height: 100%; background: #37a8db; transition: width 0.2s; }
  .om-area { border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; padding: 8px; display: flex; flex-direction: column; gap: 6px; }
  .om-area-head { display: flex; justify-content: space-between; gap: 8px; }
  .om-area-name { font-size: 13px; font-weight: 600; color: #e8e8e8; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .om-area-meta { font-size: 11px; color: #888; }
  .om-area-cov { display: flex; align-items: center; gap: 8px; font-size: 12px; color: #bbb; }
  .om-check { display: flex; align-items: center; gap: 4px; }
  .om-msg { margin: 0; font-size: 12px; color: #9fd49f; }
</style>
