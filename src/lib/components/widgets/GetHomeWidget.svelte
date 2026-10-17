<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Tailwind GET HOME widget — the tagline as an instrument. Shows live distance-to-home and,
     on a full hold, sends RTL through the same tested vehicleControl.rtl() path as the control
     panel's RTL button. Disabled unless a MAVLink vehicle is connected AND armed (an RTL mode
     change only means something in the air; on the bench it stays visibly inert). -->
<script lang="ts">
  import type { TelemetryData } from "$lib/stores/telemetry";
  import type { InterfaceSettings } from "$lib/stores/settings";
  import { homePosition, type HomePosition } from "$lib/stores/home";
  import { haversineDistance } from "$lib/utils/geo";
  import { convertDistance, formatConverted } from "$lib/utils/units";
  import { get } from "svelte/store";
  import HoldToConfirm from '$lib/components/panel/HoldToConfirm.svelte';
  import { controlAvailable, isArmed, busyAction, rtl } from '$lib/controllers/vehicleControl';

  let {
    telem,
    size = 9,
    interfaceSettings = { speedUnit: 'kmh', altitudeUnit: 'm', distanceUnit: 'metric', verticalSpeedUnit: 'ms', temperatureUnit: 'c' },
  }: {
    telem: TelemetryData;
    size?: number;
    interfaceSettings?: InterfaceSettings;
  } = $props();

  let home = $state<HomePosition>(get(homePosition));
  homePosition.subscribe((h) => { home = h; });

  let distance = $derived(
    home.set && telem.lastUpdate
      ? haversineDistance(telem.lat, telem.lon, home.lat, home.lon)
      : 0
  );
  let distanceText = $derived(() => {
    const converted = convertDistance(distance, interfaceSettings.distanceUnit);
    const digits = converted.unit === 'm' || converted.unit === 'ft'
      ? 0
      : converted.value < 10 ? 2 : converted.value < 100 ? 1 : 0;
    return formatConverted(converted, digits);
  });

  let canSend = $derived($controlAvailable && $isArmed);
</script>

<div class="gh-card" style="--ws: {size}px">
  <HoldToConfirm
    variant="warning"
    duration={1200}
    busy={$busyAction === 'rtl'}
    disabled={!canSend}
    title={canSend ? 'Hold to send RTL (Return to Launch)' : 'GET HOME — connects when a vehicle is armed'}
    onconfirm={() => rtl()}
  >
    <span class="gh-inner">
      <span class="gh-title">GET HOME</span>
      <span class="gh-dist">{home.set && telem.lastUpdate ? distanceText() : '—'}</span>
      <span class="gh-hint">{canSend ? 'HOLD FOR RTL' : 'RTL'}</span>
    </span>
  </HoldToConfirm>
</div>

<style>
  .gh-card {
    width: var(--ws);
    height: var(--ws);
    box-sizing: border-box;
    /* Tailwind brand orange — deliberately distinct from the app's blue/gold controls so the
       eye finds it instantly. */
    --htc-border: #e8862e;
    --htc-bg: rgba(232, 134, 46, 0.14);
    --htc-fg: #ffd9ae;
    --htc-fill: rgba(232, 134, 46, 0.5);
  }
  .gh-card :global(.htc) {
    height: 100%;
    border-radius: calc(var(--ws) * 0.08);
    border-width: 2px;
  }
  .gh-inner {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: calc(var(--ws) * 0.03);
    line-height: 1.1;
    white-space: normal;
  }
  .gh-title {
    font-size: calc(var(--ws) * 0.17);
    font-weight: 800;
    letter-spacing: 0.04em;
    color: #ffb066;
  }
  .gh-dist {
    font-size: calc(var(--ws) * 0.19);
    font-weight: 700;
    color: #f5f0e8;
    font-variant-numeric: tabular-nums;
  }
  .gh-hint {
    font-size: calc(var(--ws) * 0.09);
    font-weight: 600;
    letter-spacing: 0.08em;
    color: rgba(255, 217, 174, 0.65);
  }
</style>
