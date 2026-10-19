<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Tailwind AOA widget — angle of attack (ArduPilot NAMED_VALUE_FLOAT "AOA"/"SSA", needs an
     airspeed sensor on the FC) with a stall-margin colour scale, plus a low-airspeed warning
     for fixed-wing: AOA is the physical stall cue, airspeed alone lies in wind. Zones: green
     below 10°, amber 10–14°, red from 14° (typical critical AOA ≈ 15°). The low-speed warning
     fires while armed when airspeed drops under 10 m/s — check `LOW_AS_MS` before flying
     something much faster or slower. -->
<script lang="ts">
  import type { TelemetryData } from "$lib/stores/telemetry";
  import type { InterfaceSettings } from "$lib/stores/settings";
  import { convertSpeed, formatConverted } from "$lib/utils/units";

  const AOA_AMBER_DEG = 10;
  const AOA_RED_DEG = 14;
  const LOW_AS_MS = 10;

  let {
    telem,
    size = 9,
    interfaceSettings = { speedUnit: 'kmh', altitudeUnit: 'm', distanceUnit: 'metric', verticalSpeedUnit: 'ms', temperatureUnit: 'c' },
  }: {
    telem: TelemetryData;
    size?: number;
    interfaceSettings?: InterfaceSettings;
  } = $props();

  let armed = $derived((telem.armingFlags & 0x04) !== 0);
  let zone = $derived(
    telem.aoaDeg == null ? 'none'
    : telem.aoaDeg >= AOA_RED_DEG ? 'red'
    : telem.aoaDeg >= AOA_AMBER_DEG ? 'amber'
    : 'green'
  );
  let lowSpeed = $derived(armed && telem.airspeed > 0.5 && telem.airspeed < LOW_AS_MS);
  let asText = $derived(() => formatConverted(convertSpeed(telem.airspeed, interfaceSettings.speedUnit), 0));
</script>

<div class="widget-card" class:warn={zone === 'red' || lowSpeed} style="--ws: {size}px">
  <span class="w-label">AOA</span>

  {#if telem.aoaDeg != null}
    <span class="w-aoa {zone}">{telem.aoaDeg.toFixed(1)}°</span>
    {#if telem.ssaDeg != null}
      <span class="w-ssa">SSA {telem.ssaDeg.toFixed(1)}°</span>
    {/if}
    {#if zone === 'red'}
      <span class="w-flag">STALL</span>
    {:else if lowSpeed}
      <span class="w-flag">LOW SPD {asText()}</span>
    {/if}
  {:else if lowSpeed}
    <span class="w-aoa red">{asText()}</span>
    <span class="w-flag">LOW SPD</span>
  {:else}
    <span class="w-nodata">—</span>
    <span class="w-ssa">no AOA data</span>
  {/if}
</div>

<style>
  .widget-card {
    width: var(--ws);
    height: var(--ws);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    background: rgba(30, 30, 30, 0.75);
    backdrop-filter: var(--glass-blur, blur(6px));
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: calc(var(--ws) * 0.08);
    gap: calc(var(--ws) * 0.02);
    box-sizing: border-box;
    padding: calc(var(--ws) * 0.05) calc(var(--ws) * 0.06) calc(var(--ws) * 0.04);
  }
  .widget-card.warn {
    border-color: rgba(255, 70, 70, 0.75);
    animation: aoa-pulse 0.9s ease-in-out infinite;
  }
  @keyframes aoa-pulse {
    50% { background: rgba(120, 22, 22, 0.8); }
  }
  .w-label {
    font-size: calc(var(--ws) * 0.13);
    font-weight: 600;
    color: #37a8db;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .w-aoa {
    font-size: calc(var(--ws) * 0.24);
    font-weight: 700;
    color: #e0e0e0;
    font-variant-numeric: tabular-nums;
    line-height: 1.05;
  }
  .w-aoa.green { color: #8fd14f; }
  .w-aoa.amber { color: #ffb52e; }
  .w-aoa.red { color: #ff5252; }
  .w-ssa {
    font-size: calc(var(--ws) * 0.105);
    color: #888;
    font-variant-numeric: tabular-nums;
  }
  .w-flag {
    font-size: calc(var(--ws) * 0.115);
    font-weight: 800;
    letter-spacing: 0.06em;
    color: #ff6b6b;
    font-variant-numeric: tabular-nums;
  }
  .w-nodata {
    font-size: calc(var(--ws) * 0.22);
    color: #555;
    font-weight: 600;
  }
</style>
