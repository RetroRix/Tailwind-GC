<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Log Viewer — standalone viewer for ArduPilot .bin / MAVLink .tlog files (no logbook
     import): generic field graph (hand-rolled SVG), GPS track on a Leaflet map coloured by
     flight-mode segment, and a replay playhead that drives the map marker + value readout.
     Replay is a pure viewer state — it never touches the live telemetry stores or pretends
     to be a connection. Data comes from the `flightlog_open` command (flightlog/viewer.rs). -->
<script lang="ts">
  import { t } from 'svelte-i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import L from 'leaflet';
  import 'leaflet/dist/leaflet.css';
  import PanelShell from '$lib/components/panel/PanelShell.svelte';
  import Button from '$lib/components/panel/Button.svelte';
  import { settings } from '$lib/stores/settings';
  import { getProviderById } from '$lib/config/mapProviders';
  import { modeLabel, modeColor } from '$lib/helpers/flightModeRegistry';
  import { anyCase } from '$lib/helpers/fileFilters';
  import {
    fieldKey, valueAtTime, modeAtTime, nearestIndex, seriesRange, niceTicks,
    buildLinePath, formatDuration, formatValue, formatDistance, formatBytes,
    type ViewerDataset, type ViewerField, type ViewerType,
  } from '$lib/helpers/logViewerMath';

  const MAX_SERIES = 3;
  const SERIES_COLORS = ['#37a8db', '#e8c820', '#e91e9c'];
  const SPEEDS = [0.5, 1, 2, 4, 8, 16, 30];
  const NO_MODE_COLOR = '#37a8db';

  let dataset = $state<ViewerDataset | null>(null);
  let loading = $state(false);
  let errorMsg = $state('');
  let selected = $state<string[]>([]);
  let filterText = $state('');
  let playheadS = $state(0);
  let playing = $state(false);
  let speed = $state(1);

  // ── Derived view data ──
  const duration = $derived(dataset?.durationS ?? 0);
  interface SelectedSeries { key: string; type: ViewerType; field: ViewerField; color: string }
  const selectedSeries = $derived.by<SelectedSeries[]>(() => {
    if (!dataset) return [];
    const out: SelectedSeries[] = [];
    for (const key of selected) {
      for (const type of dataset.types) {
        const field = type.fields.find((f) => fieldKey(type.name, f.name) === key);
        if (field) {
          out.push({ key, type, field, color: SERIES_COLORS[out.length % SERIES_COLORS.length] });
          break;
        }
      }
    }
    return out;
  });
  const visibleTypes = $derived.by(() => {
    if (!dataset) return [];
    const q = filterText.trim().toLowerCase();
    if (!q) return dataset.types;
    return dataset.types
      .filter((ty) => ty.name.toLowerCase().includes(q) || ty.fields.some((f) => f.name.toLowerCase().includes(q)))
      .map((ty) => (ty.name.toLowerCase().includes(q)
        ? ty
        : { ...ty, fields: ty.fields.filter((f) => f.name.toLowerCase().includes(q)) }));
  });
  const trackTimes = $derived(dataset?.track.map((p) => p.tS) ?? []);
  const currentTrackPoint = $derived.by(() => {
    if (!dataset || dataset.track.length === 0) return null;
    const i = nearestIndex(trackTimes, playheadS);
    return i >= 0 ? dataset.track[i] : null;
  });
  const currentMode = $derived(dataset ? modeAtTime(dataset.modes, playheadS) : null);

  // ── Chart geometry ──
  const CH = { w: 680, h: 230, padL: 48, padR: 10, padT: 10, padB: 22 };
  const plot = { x: CH.padL, y: CH.padT, w: CH.w - CH.padL - CH.padR, h: CH.h - CH.padT - CH.padB };
  const firstRange = $derived(selectedSeries.length > 0 ? seriesRange(selectedSeries[0].field.values) : null);
  const yTicks = $derived(firstRange ? niceTicks(firstRange[0], firstRange[1], 4) : []);
  const xTicks = $derived(duration > 0 ? niceTicks(0, duration, 5) : []);
  const playheadX = $derived(duration > 0 ? plot.x + (playheadS / duration) * plot.w : plot.x);
  function yPos(v: number): number {
    if (!firstRange) return plot.y + plot.h;
    return plot.y + plot.h - ((v - firstRange[0]) / (firstRange[1] - firstRange[0])) * plot.h;
  }

  // ── File open ──
  function defaultSelection(ds: ViewerDataset): string[] {
    const gps = ds.types.find((ty) => ty.baseName === 'GPS' && (ty.instance === null || ty.instance === 0))
      ?? ds.types.find((ty) => ty.baseName === 'GPS');
    const alt = gps?.fields.find((f) => f.name === 'Alt');
    if (gps && alt) return [fieldKey(gps.name, alt.name)];
    const first = ds.types.find((ty) => ty.fields.length > 0);
    return first ? [fieldKey(first.name, first.fields[0].name)] : [];
  }

  async function openFile() {
    try {
      const picked = await openDialog({
        multiple: false,
        filters: [{ name: 'Flight logs', extensions: anyCase(['bin', 'tlog']) }],
      });
      if (!picked) return;
      const path = Array.isArray(picked) ? picked[0] : picked;
      loading = true;
      errorMsg = '';
      playing = false;
      const ds = await invoke<ViewerDataset>('flightlog_open', { filePath: path });
      dataset = ds;
      playheadS = 0;
      selected = defaultSelection(ds);
      filterText = '';
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loading = false;
    }
  }

  function toggleField(key: string, checked: boolean) {
    if (checked) {
      if (selected.length < MAX_SERIES && !selected.includes(key)) selected = [...selected, key];
    } else {
      selected = selected.filter((k) => k !== key);
    }
  }

  // ── Playback clock (viewer-local; rAF-driven, stops at the end) ──
  $effect(() => {
    if (!playing) return;
    void speed; // changing speed restarts the loop with the new rate
    let raf = 0;
    let last = performance.now();
    const step = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      const next = playheadS + dt * speed;
      if (next >= duration) {
        playheadS = duration;
        playing = false;
        return;
      }
      playheadS = next;
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  });

  function togglePlay() {
    if (!dataset || duration <= 0) return;
    if (!playing && playheadS >= duration) playheadS = 0; // replay from the top
    playing = !playing;
  }

  function seekTimeline(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0 || duration <= 0) return;
    playheadS = Math.min(duration, Math.max(0, ((e.clientX - rect.left) / rect.width) * duration));
  }

  // ── Leaflet track map ──
  let mapEl = $state<HTMLDivElement | undefined>(undefined);
  let map: L.Map | null = null;
  let trackLayer: L.LayerGroup | null = null;
  let marker: L.CircleMarker | null = null;

  function rebuildTrack() {
    if (!map || !dataset) return;
    if (trackLayer) {
      trackLayer.remove();
      trackLayer = null;
      marker = null;
    }
    const pts = dataset.track;
    if (pts.length === 0) return;
    trackLayer = L.layerGroup().addTo(map);
    // One polyline per run of equal mode — segments are coloured by flight mode.
    let runStart = 0;
    const flush = (endExclusive: number) => {
      const run = pts.slice(runStart, endExclusive);
      if (run.length >= 2) {
        L.polyline(
          run.map((p) => [p.lat, p.lon] as [number, number]),
          { color: run[0].modeId ? modeColor(run[0].modeId) : NO_MODE_COLOR, weight: 3 },
        ).addTo(trackLayer!);
      }
    };
    for (let i = 1; i <= pts.length; i++) {
      if (i === pts.length || pts[i].modeId !== pts[runStart].modeId) {
        flush(i);
        runStart = i;
      }
    }
    if (pts.length === 1) {
      L.circleMarker([pts[0].lat, pts[0].lon], { radius: 4, color: NO_MODE_COLOR }).addTo(trackLayer);
    }
    marker = L.circleMarker([pts[0].lat, pts[0].lon], {
      radius: 6, color: '#ffffff', weight: 2, fillColor: '#e60000', fillOpacity: 1,
    }).addTo(trackLayer);
    map.fitBounds(L.latLngBounds(pts.map((p) => [p.lat, p.lon] as [number, number])).pad(0.06));
    map.invalidateSize();
  }

  $effect(() => {
    // The map div is re-created when the track (or dataset) changes; a Leaflet instance
    // bound to a detached container must be rebuilt on the new element.
    if (map && mapEl && map.getContainer() !== mapEl) {
      map.remove();
      map = null;
      trackLayer = null;
      marker = null;
    }
    if (mapEl && !map) {
      const provider = getProviderById($settings.mapProvider);
      map = L.map(mapEl, { attributionControl: true });
      L.tileLayer(provider.url, { attribution: provider.attribution, maxZoom: provider.maxZoom }).addTo(map);
      map.setView([0, 0], 2);
    }
    if (map && dataset) rebuildTrack();
  });

  // Playhead → marker position (separate effect so playback doesn't rebuild the track).
  $effect(() => {
    const p = currentTrackPoint;
    if (marker && p) marker.setLatLng([p.lat, p.lon]);
  });

  $effect(() => {
    return () => {
      if (map) {
        map.remove();
        map = null;
        trackLayer = null;
        marker = null;
      }
    };
  });
</script>

<PanelShell variant="fullscreen" title={$t('logViewer.title')}>
  {#snippet headerActions()}
    <Button icon="folder" onclick={() => void openFile()} disabled={loading}>
      {$t('logViewer.openFile')}
    </Button>
  {/snippet}

  {#snippet params()}
    <div class="lv-params">
      {#if loading}
        <p class="lv-hint">{$t('logViewer.loading')}</p>
      {:else if !dataset}
        <p class="lv-hint">{$t('logViewer.openHint')}</p>
      {/if}
      {#if errorMsg}
        <p class="lv-error">{errorMsg}</p>
      {/if}

      {#if dataset}
        <div class="lv-file">
          <span class="lv-file-name" title={dataset.fileName}>{dataset.fileName}</span>
          <span class="lv-file-meta">
            {dataset.sourceFormat === 'dataflash' ? $t('logViewer.formatBin') : $t('logViewer.formatTlog')}
            · {formatBytes(dataset.fileSizeBytes)}{dataset.vehicleVariant ? ` · ${dataset.vehicleVariant}` : ''}
          </span>
        </div>

        <div class="lv-summary">
          <div><span>{$t('logViewer.summaryDuration')}</span><b>{formatDuration(dataset.durationS)}</b></div>
          <div><span>{$t('logViewer.summaryMessages')}</span><b>{dataset.summary.messageCount.toLocaleString()}</b></div>
          <div><span>{$t('logViewer.summaryTypes')}</span><b>{dataset.types.length}</b></div>
          <div><span>{$t('logViewer.summaryGpsPoints')}</span><b>{dataset.summary.gpsPointCount.toLocaleString()}</b></div>
          <div><span>{$t('logViewer.summaryMaxAlt')}</span><b>{dataset.summary.maxAltM != null ? `${formatValue(dataset.summary.maxAltM)} m` : '—'}</b></div>
          <div><span>{$t('logViewer.summaryDistance')}</span><b>{formatDistance(dataset.summary.distanceM)}</b></div>
        </div>
        {#if dataset.truncated}
          <p class="lv-warn">{$t('logViewer.truncatedNote')}</p>
        {/if}

        <div class="lv-sec-row">
          <span class="lv-sec">{$t('logViewer.fieldsTitle')}</span>
          <span class="lv-count-note">{$t('logViewer.fieldsSelected', { values: { count: selected.length, max: MAX_SERIES } })}</span>
        </div>
        <input
          class="lv-search"
          type="text"
          placeholder={$t('logViewer.fieldsSearch')}
          bind:value={filterText}
        />
        {#if dataset.types.every((ty) => ty.fields.length === 0)}
          <p class="lv-hint">{$t('logViewer.noFields')}</p>
        {/if}
        <div class="lv-types">
          {#each visibleTypes as type (type.name)}
            <div class="lv-type">
              <div class="lv-type-head"><span>{type.name}</span><span class="lv-type-count">{type.count.toLocaleString()}</span></div>
              {#each type.fields as field (field.name)}
                {@const key = fieldKey(type.name, field.name)}
                <label class="lv-field" class:lv-field-off={!selected.includes(key) && selected.length >= MAX_SERIES}>
                  <input
                    type="checkbox"
                    checked={selected.includes(key)}
                    disabled={!selected.includes(key) && selected.length >= MAX_SERIES}
                    onchange={(e) => toggleField(key, (e.currentTarget as HTMLInputElement).checked)}
                  />
                  <span class="lv-field-name">{field.name}</span>
                  {#if field.unit}<span class="lv-unit">{field.unit}</span>{/if}
                </label>
              {/each}
            </div>
          {/each}
        </div>
        <p class="lv-hint">{$t('logViewer.maxSeriesNote', { values: { max: MAX_SERIES } })}</p>
      {/if}
    </div>
  {/snippet}

  {#snippet body()}
    <div class="lv-body">
      {#if !dataset}
        <div class="lv-empty">
          <p>{$t('logViewer.openHint')}</p>
          <Button icon="folder" onclick={() => void openFile()} disabled={loading}>
            {$t('logViewer.openFile')}
          </Button>
          {#if loading}<p class="lv-hint">{$t('logViewer.loading')}</p>{/if}
          {#if errorMsg}<p class="lv-error">{errorMsg}</p>{/if}
        </div>
      {:else}
        <span class="lv-sec">{$t('logViewer.mapTitle')}</span>
        {#if dataset.track.length === 0}
          <p class="lv-hint">{$t('logViewer.mapNoTrack')}</p>
        {:else}
          <div class="lv-map" bind:this={mapEl}></div>
        {/if}

        <span class="lv-sec">{$t('logViewer.chartTitle')}</span>
        {#if selectedSeries.length === 0}
          <p class="lv-hint">{$t('logViewer.chartEmpty')}</p>
        {:else}
          <svg class="lv-chart" viewBox="0 0 {CH.w} {CH.h}" role="img">
            {#each yTicks as tick (tick)}
              <line x1={plot.x} x2={plot.x + plot.w} y1={yPos(tick)} y2={yPos(tick)} class="lv-grid" />
              <text x={plot.x - 5} y={yPos(tick) + 3} class="lv-axis" text-anchor="end">{formatValue(tick)}</text>
            {/each}
            {#each xTicks as tick (tick)}
              <text x={plot.x + (duration > 0 ? (tick / duration) * plot.w : 0)} y={CH.h - 6} class="lv-axis" text-anchor="middle">{formatDuration(tick)}</text>
            {/each}
            {#each selectedSeries as s (s.key)}
              <path
                d={buildLinePath(s.field.timesS, s.field.values, plot, duration, ...seriesRange(s.field.values))}
                fill="none"
                stroke={s.color}
                stroke-width="1.5"
              />
            {/each}
            <line x1={playheadX} x2={playheadX} y1={plot.y} y2={plot.y + plot.h} class="lv-playhead-line" />
          </svg>
          <div class="lv-legend">
            {#each selectedSeries as s (s.key)}
              {@const range = seriesRange(s.field.values)}
              <span class="lv-chip">
                <span class="lv-dot" style="background:{s.color}"></span>
                {s.type.name} · {s.field.name}{s.field.unit ? ` (${s.field.unit})` : ''}:
                <b>{formatValue(valueAtTime(s.field.timesS, s.field.values, playheadS))}</b>
                <span class="lv-range">{formatValue(range[0])}…{formatValue(range[1])}</span>
              </span>
            {/each}
          </div>
          {#if selectedSeries.length > 1}
            <p class="lv-hint">{$t('logViewer.multiScaleHint')}</p>
          {/if}
        {/if}

        <span class="lv-sec">{$t('logViewer.replayTitle')}</span>
        {#if dataset.modes.length > 0}
          <div
            class="lv-timeline"
            role="slider"
            aria-label={$t('logViewer.replayTitle')}
            aria-valuemin="0"
            aria-valuemax={Math.round(duration)}
            aria-valuenow={Math.round(playheadS)}
            tabindex="0"
            onclick={seekTimeline}
            onkeydown={(e) => {
              if (e.key === 'ArrowRight') playheadS = Math.min(duration, playheadS + 1);
              if (e.key === 'ArrowLeft') playheadS = Math.max(0, playheadS - 1);
            }}
          >
            {#each dataset.modes as seg (seg.startS)}
              <div
                class="lv-seg"
                style="left:{duration > 0 ? (seg.startS / duration) * 100 : 0}%;width:{duration > 0 ? ((seg.endS - seg.startS) / duration) * 100 : 0}%;background:{modeColor(seg.modeId)}"
                title="{modeLabel(seg.modeId)} · {formatDuration(seg.startS)}–{formatDuration(seg.endS)}"
              ></div>
            {/each}
            <div class="lv-playhead" style="left:{duration > 0 ? (playheadS / duration) * 100 : 0}%"></div>
          </div>
        {/if}
        <input
          class="lv-scrub"
          type="range"
          min="0"
          max={Math.max(duration, 0.01)}
          step="0.05"
          bind:value={playheadS}
          oninput={() => { playing = false; }}
        />
        <div class="lv-controls">
          <Button onclick={togglePlay} disabled={duration <= 0}>
            {playing ? $t('logViewer.pause') : $t('logViewer.play')}
          </Button>
          <span class="lv-time">{formatDuration(playheadS)} / {formatDuration(duration)}</span>
          <label class="lv-speed">
            {$t('logViewer.speed')}
            <select bind:value={speed}>
              {#each SPEEDS as s (s)}<option value={s}>{s}×</option>{/each}
            </select>
          </label>
        </div>
        <div class="lv-readout">
          <span>{$t('logViewer.readoutMode')}: <b>{currentMode ? modeLabel(currentMode.modeId) : '—'}</b></span>
          <span>
            {$t('logViewer.readoutPosition')}:
            <b>{currentTrackPoint ? `${currentTrackPoint.lat.toFixed(6)}, ${currentTrackPoint.lon.toFixed(6)}` : '—'}</b>
          </span>
          <span>
            {$t('logViewer.readoutAlt')}:
            <b>{currentTrackPoint?.altM != null ? `${formatValue(currentTrackPoint.altM)} m` : '—'}</b>
          </span>
        </div>
      {/if}
    </div>
  {/snippet}
</PanelShell>

<style>
  .lv-params { display: flex; flex-direction: column; gap: 8px; }
  .lv-hint { margin: 0; font-size: 12px; color: #888; }
  .lv-warn { margin: 0; font-size: 12px; color: #ffb52e; }
  .lv-error { margin: 0; font-size: 12px; color: #ff7a7a; white-space: pre-wrap; }
  .lv-file { display: flex; flex-direction: column; gap: 2px; }
  .lv-file-name { font-size: 13px; font-weight: 600; color: #e8e8e8; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lv-file-meta { font-size: 11px; color: #888; }
  .lv-summary { display: grid; grid-template-columns: 1fr 1fr; gap: 4px 10px; }
  .lv-summary div { display: flex; flex-direction: column; }
  .lv-summary span { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; color: #888; }
  .lv-summary b { font-size: 13px; color: #e8e8e8; font-weight: 600; }
  .lv-sec { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: #37a8db; }
  .lv-sec-row { display: flex; align-items: baseline; justify-content: space-between; margin-top: 4px; }
  .lv-count-note { font-size: 11px; color: #888; }
  .lv-search { height: 26px; padding: 0 6px; background: #434343; border: 1px solid #555; border-radius: 4px; color: #e0e0e0; font-size: 12px; width: 100%; box-sizing: border-box; }
  .lv-types { display: flex; flex-direction: column; gap: 8px; overflow: auto; }
  .lv-type-head { display: flex; justify-content: space-between; font-size: 12px; font-weight: 600; color: #e8e8e8; border-bottom: 1px solid rgba(255,255,255,0.08); padding-bottom: 2px; margin-bottom: 3px; }
  .lv-type-count { color: #888; font-weight: 400; }
  .lv-field { display: flex; align-items: center; gap: 6px; font-size: 12px; color: #ccc; padding: 1px 0; cursor: pointer; }
  .lv-field-off { opacity: 0.45; cursor: default; }
  .lv-field-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lv-unit { color: #888; font-size: 11px; }

  .lv-body { display: flex; flex-direction: column; gap: 8px; padding: 4px 2px; overflow: auto; }
  .lv-empty { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; color: #bbb; font-size: 13px; }
  .lv-map { height: 300px; border-radius: 6px; overflow: hidden; background: #1a1a1a; }
  .lv-chart { width: 100%; height: auto; background: rgba(255,255,255,0.03); border-radius: 6px; }
  .lv-grid { stroke: rgba(255,255,255,0.09); stroke-width: 1; }
  .lv-axis { fill: #999; font-size: 10px; }
  .lv-playhead-line { stroke: #ff5252; stroke-width: 1.5; }
  .lv-legend { display: flex; flex-wrap: wrap; gap: 6px 14px; font-size: 12px; color: #ccc; }
  .lv-chip { display: inline-flex; align-items: center; gap: 5px; }
  .lv-dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .lv-range { color: #777; font-size: 11px; }
  .lv-timeline { position: relative; height: 18px; border-radius: 4px; overflow: hidden; background: rgba(255,255,255,0.08); cursor: pointer; }
  .lv-seg { position: absolute; top: 0; bottom: 0; opacity: 0.85; }
  .lv-playhead { position: absolute; top: 0; bottom: 0; width: 2px; background: #ff5252; transform: translateX(-1px); pointer-events: none; }
  .lv-scrub { width: 100%; accent-color: #37a8db; }
  .lv-controls { display: flex; align-items: center; gap: 12px; }
  .lv-time { font-size: 12px; color: #ccc; font-variant-numeric: tabular-nums; }
  .lv-speed { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: #ccc; }
  .lv-speed select { height: 26px; padding: 0 6px; background: #434343; border: 1px solid #555; border-radius: 4px; color: #e0e0e0; font-size: 12px; }
  .lv-readout { display: flex; flex-wrap: wrap; gap: 6px 18px; font-size: 12px; color: #aaa; }
  .lv-readout b { color: #e8e8e8; font-weight: 600; }
</style>
