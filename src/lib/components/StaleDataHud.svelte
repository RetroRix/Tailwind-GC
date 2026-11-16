<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Rick Quast
-->

<!-- Stale-data HUD: when the link drops, the last known position / heading / battery stay on screen
     greyed under a NO LINK bar with the time since the last data arrived, instead of every widget
     blanking at once (rivals batch, 2026-11-16). Data comes from `staleSnapshot` in the telemetry
     store, captured at disconnect; it clears itself the moment fresh data flows again. -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { t } from 'svelte-i18n';
  import { staleSnapshot } from '$lib/stores/telemetry';
  import { connection } from '$lib/stores/connection';

  let now = $state(Date.now());
  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(() => { timer = setInterval(() => { now = Date.now(); }, 1000); });
  onDestroy(() => { if (timer) clearInterval(timer); });

  let snap = $derived($staleSnapshot);
  let visible = $derived(snap !== null && $connection.status !== 'connected');
  let d = $derived(snap?.data);

  function elapsedText(lostAt: number): string {
    const s = Math.max(0, Math.floor((now - lostAt) / 1000));
    const m = Math.floor(s / 60);
    const h = Math.floor(m / 60);
    if (h > 0) return `${h}:${String(m % 60).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
    return `${m}:${String(s % 60).padStart(2, '0')}`;
  }

  function fmtCoord(v: number, pos: string, neg: string): string {
    return `${v >= 0 ? pos : neg} ${Math.abs(v).toFixed(5)}`;
  }
</script>

{#if visible && snap && d}
  <div class="stale-hud" role="alert">
    <div class="stale-bar">
      <span class="stale-title">{$t('hud.noLink')}</span>
      <span class="stale-elapsed">{$t('hud.lastData')} {elapsedText(snap.lostAt)}</span>
    </div>
    <div class="stale-values">
      {#if d.fixType > 0}
        <span>{fmtCoord(d.lat, 'N', 'S')} · {fmtCoord(d.lon, 'E', 'W')}</span>
        <span>{$t('hud.heading')} {Math.round(((d.yaw % 360) + 360) % 360)}°</span>
        <span>{$t('hud.altitude')} {d.altMsl.toFixed(0)} m</span>
        <span>{d.groundSpeed.toFixed(0)} m/s</span>
        <span>{d.numSat} SAT</span>
      {/if}
      <span>{$t('hud.battery')} {d.batteryPercentage}% · {d.voltage.toFixed(1)} V</span>
    </div>
  </div>
{/if}

<style>
  .stale-hud {
    position: absolute;
    top: 62px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 1200;
    min-width: 340px;
    max-width: 92%;
    border-radius: 8px;
    overflow: hidden;
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.5);
    font-family: inherit;
    pointer-events: none;
  }
  .stale-bar {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    background: #c0392b;
    color: #fff;
    padding: 5px 12px;
  }
  .stale-title {
    font-weight: 800;
    letter-spacing: 0.12em;
    font-size: 13px;
  }
  .stale-elapsed {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    opacity: 0.95;
  }
  .stale-values {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    justify-content: center;
    background: rgba(24, 24, 24, 0.88);
    color: #9a9a9a;
    padding: 5px 12px 6px;
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
</style>
