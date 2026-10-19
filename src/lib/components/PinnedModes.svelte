<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- Tailwind pinned flight modes — the pilot's chosen modes as one-tap buttons in the status
     bar (TitanPlanner-style). The pin list lives in settings (`pinnedModes`, edited from the
     Control panel's Modes section); what's rendered is pins ∩ the connected vehicle's
     non-stick modes, so the same list works across airframes. Mode switches go through the
     same vehicleControl.setMode path as the Control panel. Renders nothing when no MAVLink
     vehicle is connected or no pins apply. -->
<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import { autopilotSystem } from '$lib/stores/autopilotContext';
  import { arduVehicleClass } from '$lib/stores/missionArdupilot';
  import { modesFor } from '$lib/helpers/mavModes';
  import { controlAvailable, activeMode, setMode } from '$lib/controllers/vehicleControl';

  const sys = $derived($autopilotSystem);
  const cls = $derived($arduVehicleClass);
  const pins = $derived($settings.pinnedModes ?? []);
  const pinned = $derived(
    modesFor(sys, cls).filter((m) => !m.stick && pins.includes(m.key))
  );
  const activeKey = $derived($activeMode?.key ?? '');
</script>

{#if $controlAvailable && pinned.length > 0}
  <div class="pinned-modes" role="group" aria-label="Pinned flight modes">
    {#each pinned as m}
      <button
        class="pm-btn"
        class:active={activeKey === m.key}
        onclick={() => void setMode(m)}
        title="Switch to {m.name}"
      >{m.name}</button>
    {/each}
  </div>
{/if}

<style>
  .pinned-modes {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .pm-btn {
    padding: 2px 9px;
    border-radius: 4px;
    border: 1px solid rgba(55, 168, 219, 0.45);
    background: rgba(55, 168, 219, 0.1);
    color: #cfe8f4;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    cursor: pointer;
    white-space: nowrap;
  }
  .pm-btn:hover { filter: brightness(1.25); }
  .pm-btn.active {
    background: #37a8db;
    color: #06222f;
    border-color: #37a8db;
  }
</style>
