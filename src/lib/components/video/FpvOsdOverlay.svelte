<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->
<script lang="ts">
  // Ground-rendered OSD for the FPV link (Tailwind, wfb-ng panel): DOM/SVG drawn OVER the
  // video surface from the app's live telemetry stores — the same data the widgets read.
  // Because nothing here is baked into the video, the overlay inherently survives video
  // breakup: when frames stop (reconnect, radio hole) the surface underneath falls back to
  // its placeholder while this layer stays up, and once telemetry itself goes quiet the
  // whole layer switches to its stale styling (dimmed + STALE chip) instead of pretending
  // the last values are current.
  //
  // The artificial horizon is FpvHud — the same component the 3D map's FPV camera mode
  // renders — reused as-is. Its pitch ladder is conformal to a camera FOV; over a video
  // feed the camera's true FOV is unknown, so the ladder's pitch SCALE is approximate
  // (90° assumed, a typical FPV camera) while roll/heading track exactly. In `compact`
  // mode (the small widget tile) the horizon is skipped — the tile is a quick-look
  // surface — and only the corner readouts show.
  import { t } from 'svelte-i18n';
  import { telemetry } from '$lib/stores/telemetry';
  import { homePosition } from '$lib/stores/home';
  import { settings } from '$lib/stores/settings';
  import { convertAltitude, convertDistance, convertSpeed, speedDigits } from '$lib/utils/units';
  import FpvHud from '$lib/components/FpvHud.svelte';

  let { compact = false }: { compact?: boolean } = $props();

  // Staleness clock: telemetry carries its own lastUpdate; a 500 ms tick is enough to flip
  // the stale styling within a second of the link going quiet.
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(id);
  });

  const telem = $derived($telemetry);
  const stale = $derived(!telem.statusSeen || now - telem.lastUpdate > 3000);
  const armed = $derived(telem.statusSeen && (telem.armingFlags & 0x04) !== 0);
  const mode = $derived(telem.flightMode.primary ? telem.flightMode.primary.toUpperCase() : '–');

  // Display units live under settings.interface (the Map3D HUD reads them the same way).
  const speedUnit = $derived($settings.interface?.speedUnit ?? 'kmh');
  const altitudeUnit = $derived($settings.interface?.altitudeUnit ?? 'm');
  const distanceUnit = $derived($settings.interface?.distanceUnit ?? 'metric');

  // Airspeed when the FC actually reports one (the Map3D HUD's rule), else ground speed.
  const speedIsAir = $derived(telem.airspeed > 0);
  const spd = $derived(convertSpeed(speedIsAir ? telem.airspeed : telem.groundSpeed, speedUnit));
  const alt = $derived(convertAltitude(telem.altitude, altitudeUnit));

  const battText = $derived(
    telem.statusSeen && telem.voltage > 0
      ? `${telem.voltage.toFixed(1)} V · ${Math.round(telem.batteryPercentage)} %`
      : '–',
  );
  const gpsText = $derived.by(() => {
    if (telem.fixType < 2) return $t('video.osd.noGps');
    return `${telem.fixType >= 3 ? '3D' : '2D'} · ${telem.numSat} SAT`;
  });

  /** Distance to home (haversine on the FC home + current fix); null when either is unknown. */
  const homeText = $derived.by(() => {
    const h = $homePosition;
    if (!h.set || telem.fixType < 2 || (telem.lat === 0 && telem.lon === 0)) return null;
    const dLat = ((telem.lat - h.lat) * Math.PI) / 180;
    const dLon = ((telem.lon - h.lon) * Math.PI) / 180;
    const a =
      Math.sin(dLat / 2) ** 2 +
      Math.cos((h.lat * Math.PI) / 180) * Math.cos((telem.lat * Math.PI) / 180) * Math.sin(dLon / 2) ** 2;
    const m = 2 * 6_371_000 * Math.asin(Math.min(1, Math.sqrt(a)));
    const d = convertDistance(m, distanceUnit);
    return `${$t('video.osd.home')} ${d.value >= 100 ? Math.round(d.value) : d.value.toFixed(1)} ${d.unit}`;
  });
</script>

<div class="fpv-osd" class:stale aria-hidden="true">
  {#if !compact}
    <FpvHud
      heading={telem.yaw}
      pitch={telem.pitch}
      roll={telem.roll}
      speed={spd.value}
      speedUnit={spd.unit}
      speedDigits={speedDigits(speedUnit)}
      speedLabel={speedIsAir ? 'ASPD' : 'SPD'}
      altitude={alt.value}
      altitudeUnit={alt.unit}
      fov={90}
    />
  {/if}

  <!-- Top-left: flight mode + armed state -->
  <div class="osd-corner tl">
    <span class="osd-mode">{mode}</span>
    <span class="osd-armed" class:on={armed}>{armed ? $t('video.osd.armed') : $t('video.osd.disarmed')}</span>
  </div>

  <!-- Top-right: GPS fix + satellites (+ distance home when known) -->
  <div class="osd-corner tr">
    <span>{gpsText}</span>
    {#if homeText}<span>{homeText}</span>{/if}
  </div>

  <!-- Bottom-left: battery -->
  <div class="osd-corner bl">
    <span>🔋 {battText}</span>
  </div>

  <!-- Bottom-right: the numeric pair the FpvHud cluster also draws — in compact mode (no
       HUD) these corners are the only altitude/speed readout, so they always render. -->
  <div class="osd-corner br">
    <span>{spd.value.toFixed(speedDigits(speedUnit))} {spd.unit}</span>
    <span>{alt.value >= 100 ? Math.round(alt.value) : alt.value.toFixed(1)} {alt.unit}</span>
  </div>

  {#if stale}
    <div class="osd-stale">{$t('video.osd.stale')}</div>
  {/if}
</div>

<style>
  .fpv-osd {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Below the reconnect overlay (z 5) — its Stop button and message win when shown. */
    z-index: 4;
    font-family: 'Consolas', 'JetBrains Mono', monospace;
    transition: opacity 0.3s ease;
  }
  /* Last-known values stay readable but visibly aged — never presented as live. */
  .fpv-osd.stale {
    opacity: 0.45;
    filter: saturate(0.4);
  }
  .osd-corner {
    position: absolute;
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding: 6px 9px;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #3cff8e;
    text-shadow: 0 0 3px rgba(60, 255, 140, 0.5), 0 1px 2px rgba(0, 0, 0, 0.95);
    z-index: 21; /* above FpvHud's own layers inside this stacking context */
    white-space: nowrap;
  }
  .osd-corner.tl { top: 0; left: 0; }
  .osd-corner.tr { top: 0; right: 0; }
  /* Bottom corners ride above the link-health strip (20 px) that shares the surface. */
  .osd-corner.bl { bottom: 20px; left: 0; }
  .osd-corner.br { bottom: 20px; right: 0; }
  .osd-armed { color: #8a8a8a; }
  .osd-armed.on { color: #ff5347; }
  .osd-stale {
    position: absolute;
    top: 6px;
    left: 50%;
    transform: translateX(-50%);
    padding: 2px 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.12em;
    color: #ffb02e;
    background: rgba(0, 0, 0, 0.55);
    border: 1px solid rgba(255, 176, 46, 0.6);
    border-radius: 3px;
    z-index: 21;
  }
</style>
