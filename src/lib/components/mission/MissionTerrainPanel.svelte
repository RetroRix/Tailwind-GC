<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->

<!-- MissionTerrainPanel.svelte
     "Terrain" section of the mission panel (INAV + ArduPilot/PX4): samples the
     terrain along every leg of the current mission via the Copernicus
     `terrain_profile` backend command, draws a profile chart (terrain vs the
     planned altitude resolved to AMSL, legs below the pilot-set minimum
     clearance in red, honest gaps where there is no terrain data), and offers
     a raise-only "Hold clearance" rewrite that preserves each waypoint's own
     altitude mode/frame and lands as ONE undo step. Nothing is rewritten
     until the pilot reviews the old → new summary and clicks Apply.
-->
<script lang="ts">
  import { get } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from 'svelte-i18n';
  import {
    mission, launchPoint, missionUpdateWp, beginUndoGroup, endUndoGroup,
    hasLocation, isModifier, altFromM,
    WpAction, ALT_MODE_REL, ALT_MODE_AMSL, ALT_MODE_AGL,
    type Waypoint,
  } from '$lib/stores/mission';
  import {
    arduMission, arduUpdateWp, arduBeginUndoGroup, arduEndUndoGroup,
    MAV_CMD_NAV_WAYPOINT, MAV_CMD_NAV_LOITER_UNLIM, MAV_CMD_NAV_LOITER_TURNS, MAV_CMD_NAV_LOITER_TIME,
    MAV_FRAME_GLOBAL, MAV_FRAME_GLOBAL_RELATIVE_ALT,
    type ArduWaypoint,
  } from '$lib/stores/missionArdupilot';
  import { autopilotSystem } from '$lib/stores/autopilotContext';
  import { homePosition } from '$lib/stores/home';
  import { settings, type InterfaceSettings } from '$lib/stores/settings';
  import { convertAltitude, convertDistance, formatConverted } from '$lib/utils/units';
  import NumberStepper from '$lib/components/NumberStepper.svelte';
  import {
    computeHoldClearance, haversineM,
    type AltSemantics, type ClearanceWp, type IncomingLeg,
  } from '$lib/helpers/terrainClearance';

  let { editing = false }: { editing?: boolean } = $props();

  const isInav = $derived($autopilotSystem === 'inav');

  const IFACE_FALLBACK: InterfaceSettings = {
    speedUnit: 'kmh', altitudeUnit: 'm', distanceUnit: 'metric', verticalSpeedUnit: 'ms', temperatureUnit: 'c',
  };
  const iface = $derived($settings.interface ?? IFACE_FALLBACK);

  /** INAV actions that are flown-to navigation targets (Land/POI excluded: not en-route altitudes). */
  const INAV_NAV_ACTIONS = new Set<number>([WpAction.Waypoint, WpAction.PosholdUnlim, WpAction.PosholdTime]);
  const ARDU_NAV_CMDS = new Set([
    MAV_CMD_NAV_WAYPOINT, MAV_CMD_NAV_LOITER_UNLIM, MAV_CMD_NAV_LOITER_TURNS, MAV_CMD_NAV_LOITER_TIME,
  ]);

  function inavMode(wp: Waypoint): AltSemantics {
    const m = wp.alt_mode ?? ((wp.p3 & 1) ? ALT_MODE_AMSL : ALT_MODE_REL);
    return m === ALT_MODE_AGL ? 'agl' : m === ALT_MODE_AMSL ? 'amsl' : 'relative';
  }
  function arduMode(wp: ArduWaypoint): AltSemantics {
    return wp.frame === MAV_FRAME_GLOBAL ? 'amsl' : wp.frame === MAV_FRAME_GLOBAL_RELATIVE_ALT ? 'relative' : 'agl';
  }
  function modeLabel(mode: AltSemantics): string {
    return mode === 'amsl' ? 'AMSL' : mode === 'relative' ? 'REL' : isInav ? 'AGL' : 'TRN';
  }

  /** Navigation waypoints of the active mission, normalised across both models. */
  const navWps = $derived.by<ClearanceWp[]>(() => {
    const out: ClearanceWp[] = [];
    if (isInav) {
      let dn = 0;
      $mission.waypoints.forEach((wp, i) => {
        if (!isModifier(wp.action)) dn++;
        if (!INAV_NAV_ACTIONS.has(wp.action) || !hasLocation(wp.action)) return;
        if (wp.lat === 0 && wp.lon === 0) return;
        out.push({
          srcIndex: i, number: dn, lat: wp.lat / 1e7, lon: wp.lon / 1e7,
          altM: wp.altitude / 100, mode: inavMode(wp),
        });
      });
    } else {
      $arduMission.forEach((wp, i) => {
        if (!ARDU_NAV_CMDS.has(wp.command)) return;
        if (wp.lat === 0 && wp.lon === 0) return;
        out.push({
          srcIndex: i, number: i + 1, lat: wp.lat / 1e7, lon: wp.lon / 1e7,
          altM: wp.alt, mode: arduMode(wp),
        });
      });
    }
    return out;
  });

  /** Home/launch reference point: INAV planning launch point / FC home for ArduPilot. */
  const homePt = $derived.by<{ lat: number; lon: number } | null>(() => {
    if (isInav) return $launchPoint ? { lat: $launchPoint.lat, lon: $launchPoint.lng } : null;
    return $homePosition.set ? { lat: $homePosition.lat, lon: $homePosition.lon } : null;
  });

  // ── Sampling ────────────────────────────────────────────────────────
  interface RawSample { dist_m: number; lat: number; lon: number; elev_m: number | null; }
  interface SampledData {
    legs: IncomingLeg[];
    grounds: (number | null)[];
    /** Terrain elevation at the home/launch point (null = no home or no data there). */
    homeElev: number | null;
    /** Reference elevation actually used for 'relative' altitudes. */
    refElev: number | null;
    /** refElev came from WP1's terrain because home was unknown / had no data. */
    refFallback: boolean;
    /** Stitched terrain samples over the whole route (cumulative distance). */
    chart: { dist: number; elev: number | null }[];
    /** Cumulative distance of each nav waypoint. */
    wpDists: number[];
    totalDist: number;
    anyTerrain: boolean;
  }

  const SAMPLE_SPACING_M = 30;      // within the 25–50 m band
  const MAX_SAMPLES_PER_LEG = 500;  // spacing grows on very long legs instead of firing huge profiles

  let clearanceM = $state(30);
  let sampled = $state<SampledData | null>(null);
  let sampling = $state(false);
  let samplingLabel = $state('');
  let applyStatus = $state('');
  let applying = $state(false);
  let computeToken = 0;

  // Auto-clear the transient "applied" line after 10 s (mirrors the panels' status line).
  $effect(() => {
    if (!applyStatus) return;
    const id = setTimeout(() => { applyStatus = ''; }, 10000);
    return () => clearTimeout(id);
  });

  $effect(() => {
    const wps = navWps;
    const home = homePt;
    if (wps.length === 0 || (wps.length < 2 && !home)) {
      sampled = null;
      sampling = false;
      return;
    }
    const sig = `${home ? `${home.lat},${home.lon}` : '-'}|${wps
      .map((w) => `${w.srcIndex}:${w.lat},${w.lon},${w.altM},${w.mode}`)
      .join('|')}`;
    const token = ++computeToken;
    sampling = true;
    samplingLabel = $t('missionTerrain.sampling');
    const timer = setTimeout(async () => {
      try {
        const data = await sampleRoute(wps, home, token, sig);
        if (data && token === computeToken) {
          sampled = data;
          sampling = false;
        }
      } catch (e) {
        console.error('[mission-terrain] sampling failed', e);
        if (token === computeToken) {
          sampled = null;
          sampling = false;
        }
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  let lastSig = '';
  async function sampleRoute(
    wps: ClearanceWp[],
    home: { lat: number; lon: number } | null,
    token: number,
    sig: string,
  ): Promise<SampledData | null> {
    if (sig === lastSig && sampled) return sampled;
    // Point elevations (home + every waypoint) in ONE batched IPC call.
    const pts: [number, number][] = [];
    if (home) pts.push([home.lat, home.lon]);
    for (const w of wps) pts.push([w.lat, w.lon]);
    const elevs = await invoke<(number | null)[]>('terrain_elevations', { points: pts });
    if (token !== computeToken) return null;
    const homeElev = home ? (elevs[0] ?? null) : null;
    const grounds = wps.map((_, i) => elevs[(home ? 1 : 0) + i] ?? null);

    // Per-leg profiles, awaited sequentially (legs are few; samples per leg capped).
    const route: { lat: number; lon: number }[] = home ? [home, ...wps] : [...wps];
    const legs: IncomingLeg[] = [];
    const chart: { dist: number; elev: number | null }[] = [];
    const wpDists: number[] = [];
    let offset = 0;
    if (!home) {
      legs.push({ lengthM: 0, samples: [{ distM: 0, elevM: grounds[0] }] });
      chart.push({ dist: 0, elev: grounds[0] });
      wpDists.push(0);
    }
    const legTotal = route.length - 1;
    for (let k = 1; k < route.length; k++) {
      const a = route[k - 1];
      const b = route[k];
      const len = haversineM(a.lat, a.lon, b.lat, b.lon);
      samplingLabel = $t('missionTerrain.samplingLeg', { values: { current: k, total: legTotal } });
      const spacing = Math.max(SAMPLE_SPACING_M, len / MAX_SAMPLES_PER_LEG);
      const raw = await invoke<RawSample[]>('terrain_profile', {
        points: [[a.lat, a.lon], [b.lat, b.lon]],
        spacingM: spacing,
      });
      if (token !== computeToken) return null;
      legs.push({ lengthM: len, samples: raw.map((s) => ({ distM: s.dist_m, elevM: s.elev_m })) });
      for (const s of raw) {
        if (chart.length > 0 && s.dist_m === 0) continue; // joint point already stitched
        chart.push({ dist: offset + s.dist_m, elev: s.elev_m });
      }
      offset += len;
      wpDists.push(offset);
    }

    const refElev = homeElev ?? grounds[0] ?? null;
    const data: SampledData = {
      legs,
      grounds,
      homeElev,
      refElev,
      refFallback: homeElev == null && grounds[0] != null,
      chart,
      wpDists,
      totalDist: offset,
      anyTerrain: chart.some((s) => s.elev != null) || grounds.some((g) => g != null),
    };
    lastSig = sig;
    return data;
  }

  // ── Rewrite computation (pure helper over the sampled data) ─────────
  const rewrite = $derived(
    sampled ? computeHoldClearance(navWps, sampled.legs, sampled.grounds, sampled.refElev, clearanceM) : null,
  );

  /** Planned path points (distance, AMSL) — starts on the ground at home when a home exists. */
  const pathPts = $derived.by<{ dist: number; altMsl: number | null }[]>(() => {
    if (!sampled || !rewrite) return [];
    const sd = sampled;
    const pts: { dist: number; altMsl: number | null }[] = [];
    if (homePt) pts.push({ dist: 0, altMsl: sd.homeElev });
    navWps.forEach((w, i) => pts.push({ dist: sd.wpDists[i], altMsl: rewrite.currentMsl[i] }));
    return pts;
  });

  /** Path points after the rewrite (preview). */
  const previewPts = $derived.by<{ dist: number; altMsl: number | null }[] | null>(() => {
    if (!sampled || !rewrite || rewrite.changes.length === 0) return null;
    const byIdx = new Map(rewrite.changes.map((c) => [c.srcIndex, c.newAltMsl]));
    return pathPts.map((p, i) => {
      if (i === 0 && homePt) return p;
      const wp = navWps[homePt ? i - 1 : i];
      const alt = wp ? byIdx.get(wp.srcIndex) : undefined;
      return alt != null ? { dist: p.dist, altMsl: alt } : p;
    });
  });

  /** Planned AMSL altitude interpolated at a route distance (null across a gap). */
  function pathAltAt(pts: { dist: number; altMsl: number | null }[], d: number): number | null {
    if (pts.length === 0) return null;
    if (d <= pts[0].dist) return pts[0].altMsl;
    const last = pts[pts.length - 1];
    if (d >= last.dist) return last.altMsl;
    for (let i = 1; i < pts.length; i++) {
      if (d <= pts[i].dist) {
        const a = pts[i - 1];
        const b = pts[i];
        if (a.altMsl == null || b.altMsl == null) return null;
        const span = b.dist - a.dist || 1;
        return a.altMsl + ((b.altMsl - a.altMsl) * (d - a.dist)) / span;
      }
    }
    return last.altMsl;
  }

  /** Per-sample clearance + the en-route window (climb-out / descent trimmed, like the
   *  fullscreen terrain panel: only the portion that ever reaches the clearance counts). */
  const sampleClearance = $derived.by<{ clr: (number | null)[]; first: number; last: number; min: number | null }>(() => {
    if (!sampled) return { clr: [], first: 0, last: -1, min: null };
    const clr = sampled.chart.map((s) => {
      if (s.elev == null) return null;
      const pa = pathAltAt(pathPts, s.dist);
      return pa == null ? null : pa - s.elev;
    });
    let first = -1;
    let last = -1;
    for (let i = 0; i < clr.length; i++) {
      if (clr[i] != null && (clr[i] as number) >= clearanceM) {
        if (first < 0) first = i;
        last = i;
      }
    }
    if (first < 0) { first = 0; last = clr.length - 1; } // never reaches clearance → judge the whole route
    let min: number | null = null;
    for (let i = first; i <= last && i < clr.length; i++) {
      const c = clr[i];
      if (c != null && (min == null || c < min)) min = c;
    }
    return { clr, first, last, min };
  });

  // ── Chart geometry (hand-rolled SVG, no chart dependency) ───────────
  const CW = 320;
  const CH = 176;
  const PAD = { l: 36, r: 6, t: 10, b: 18 };
  const chartGeom = $derived.by(() => {
    if (!sampled) return null;
    let lo = Infinity;
    let hi = -Infinity;
    for (const s of sampled.chart) {
      if (s.elev != null) { lo = Math.min(lo, s.elev); hi = Math.max(hi, s.elev); }
    }
    for (const p of [...pathPts, ...(previewPts ?? [])]) {
      if (p.altMsl != null) { lo = Math.min(lo, p.altMsl); hi = Math.max(hi, p.altMsl); }
    }
    if (!isFinite(lo) || !isFinite(hi)) { lo = 0; hi = 100; }
    if (hi - lo < 10) { hi += 5; lo -= 5; }
    const padY = (hi - lo) * 0.08;
    lo -= padY; hi += padY;
    const total = Math.max(sampled.totalDist, 1);
    const x = (d: number) => PAD.l + (d / total) * (CW - PAD.l - PAD.r);
    const y = (v: number) => PAD.t + (1 - (v - lo) / (hi - lo)) * (CH - PAD.t - PAD.b);
    return { lo, hi, x, y };
  });

  /** Terrain area segments — split at no-data samples so gaps stay gaps. */
  const terrainSegs = $derived.by<string[]>(() => {
    if (!sampled || !chartGeom) return [];
    const g = chartGeom;
    const segs: string[] = [];
    let run: { dist: number; elev: number }[] = [];
    const flush = () => {
      if (run.length > 1) {
        const line = run.map((s) => `${g.x(s.dist).toFixed(1)},${g.y(s.elev).toFixed(1)}`).join(' ');
        const firstX = g.x(run[0].dist).toFixed(1);
        const lastX = g.x(run[run.length - 1].dist).toFixed(1);
        const base = g.y(chartGeom.lo).toFixed(1);
        segs.push(`M${firstX},${base} L${line.replaceAll(' ', ' L')} L${lastX},${base} Z`);
      }
      run = [];
    };
    for (const s of sampled.chart) {
      if (s.elev == null) flush();
      else run.push({ dist: s.dist, elev: s.elev });
    }
    flush();
    return segs;
  });

  /** Planned-path line runs, coloured per segment: red below clearance, grey where
   *  terrain is unknown, blue otherwise. */
  const pathRuns = $derived.by<{ pts: string; cls: string }[]>(() => {
    if (!sampled || !chartGeom) return [];
    const g = chartGeom;
    const { clr, first, last } = sampleClearance;
    const runs: { pts: string; cls: string }[] = [];
    let cur: { cls: string; pts: [number, number][] } | null = null;
    const flush = () => {
      if (cur && cur.pts.length > 1) {
        runs.push({ pts: cur.pts.map(([px, py]) => `${px.toFixed(1)},${py.toFixed(1)}`).join(' '), cls: cur.cls });
      }
      cur = null;
    };
    for (let i = 0; i < sampled.chart.length; i++) {
      const s = sampled.chart[i];
      const pa = pathAltAt(pathPts, s.dist);
      if (pa == null) { flush(); continue; }
      const c = clr[i];
      const cls = c == null ? 'unknown' : i >= first && i <= last && c < clearanceM ? 'below' : 'ok';
      const pt: [number, number] = [g.x(s.dist), g.y(pa)];
      if (!cur || cur.cls !== cls) {
        const join: [number, number] | null = cur ? cur.pts[cur.pts.length - 1] : null;
        flush();
        cur = { cls, pts: join ? [join, pt] : [pt] };
      } else {
        cur.pts.push(pt);
      }
    }
    flush();
    return runs;
  });

  const previewLine = $derived.by<string | null>(() => {
    if (!previewPts || !chartGeom) return null;
    const g = chartGeom;
    const parts: string[] = [];
    let run: string[] = [];
    for (const p of previewPts) {
      if (p.altMsl == null) {
        if (run.length > 1) parts.push(run.join(' '));
        run = [];
      } else {
        run.push(`${g.x(p.dist).toFixed(1)},${g.y(p.altMsl).toFixed(1)}`);
      }
    }
    if (run.length > 1) parts.push(run.join(' '));
    return parts.length ? parts.join(' M') : null;
  });

  // ── Apply (one undo step via the stores' group mechanism) ──────────
  async function applyRewrite() {
    const r = rewrite;
    if (!r || r.changes.length === 0 || !editing || applying) return;
    applying = true;
    try {
      if (isInav) {
        beginUndoGroup();
        try {
          for (const ch of r.changes) {
            const wp = get(mission).waypoints[ch.srcIndex];
            if (!wp) continue;
            await missionUpdateWp(ch.srcIndex, { ...wp, altitude: altFromM(ch.newAltM) });
          }
        } finally {
          endUndoGroup();
        }
      } else {
        arduBeginUndoGroup();
        try {
          for (const ch of r.changes) {
            const wp = get(arduMission)[ch.srcIndex];
            if (!wp) continue;
            arduUpdateWp(ch.srcIndex, { ...wp, alt: ch.newAltM });
          }
        } finally {
          arduEndUndoGroup();
        }
      }
      applyStatus = $t('missionTerrain.applied', { values: { count: r.changes.length, clearance: clearanceM } });
    } catch (e) {
      console.error('[mission-terrain] apply failed', e);
    } finally {
      applying = false;
    }
  }

  // ── Formatting ──────────────────────────────────────────────────────
  function fmtAlt(m: number): string {
    const c = convertAltitude(m, iface.altitudeUnit);
    return `${Math.round(c.value)} ${c.unit}`;
  }
  function fmtStored(m: number): string {
    return `${m.toFixed(1)} m`;
  }
  function fmtDist(m: number): string {
    return formatConverted(convertDistance(m, iface.distanceUnit), m >= 1000 ? 2 : 0);
  }
  const insufficient = $derived(navWps.length === 0 || (navWps.length < 2 && !homePt));
</script>

<div class="mt-panel">
  <div class="mt-head">{$t('missionTerrain.title')}</div>

  <div class="mt-ctrl">
    <span>{$t('missionTerrain.minClearance')}</span>
    <NumberStepper bind:value={clearanceM} min={0} max={500} step={5} decimals={0} unit="m" />
  </div>

  {#if insufficient}
    <p class="mt-note">{$t('missionTerrain.insufficient')}</p>
  {:else if sampling}
    <p class="mt-note">{samplingLabel}</p>
  {:else if sampled && !sampled.anyTerrain}
    <p class="mt-note mt-warn">{$t('missionTerrain.noTerrain')}</p>
  {:else if sampled && chartGeom}
    <svg class="mt-chart" viewBox="0 0 {CW} {CH}" role="img" aria-label={$t('missionTerrain.title')}>
      {#each terrainSegs as d}
        <path {d} class="mt-terrain" />
      {/each}
      {#if previewLine}
        <path d="M{previewLine}" class="mt-preview" />
      {/if}
      {#each pathRuns as run}
        <polyline points={run.pts} class="mt-path mt-path-{run.cls}" />
      {/each}
      {#each pathPts as p}
        {#if p.altMsl != null}
          <circle cx={chartGeom.x(p.dist)} cy={chartGeom.y(p.altMsl)} r="2.4" class="mt-dot" />
        {/if}
      {/each}
      {#each navWps as w, i}
        {#if rewrite && rewrite.currentMsl[i] != null}
          <text x={chartGeom.x(sampled.wpDists[i])} y={chartGeom.y(rewrite.currentMsl[i] as number) - 5} class="mt-wpnum">{w.number}</text>
        {/if}
      {/each}
      <text x={PAD.l} y={CH - 4} class="mt-axis">{fmtAlt(chartGeom.lo)}</text>
      <text x={PAD.l} y={PAD.t + 2} class="mt-axis">{fmtAlt(chartGeom.hi)}</text>
      <text x={CW - PAD.r} y={CH - 4} class="mt-axis mt-axis-r">{fmtDist(sampled.totalDist)}</text>
    </svg>

    <div class="mt-legend">
      <span class="lg"><i class="sw terrain"></i>{$t('missionTerrain.legendTerrain')}</span>
      <span class="lg"><i class="sw path"></i>{$t('missionTerrain.legendPath')}</span>
      <span class="lg"><i class="sw below"></i>{$t('missionTerrain.legendBelow')}</span>
      <span class="lg"><i class="sw unknown"></i>{$t('missionTerrain.legendUnknown')}</span>
      {#if previewLine}<span class="lg"><i class="sw preview"></i>{$t('missionTerrain.legendPreview')}</span>{/if}
    </div>

    <div class="mt-stats">
      <span>{$t('missionTerrain.minClearanceNow')}: <b class:warn={sampleClearance.min != null && sampleClearance.min < clearanceM}>{sampleClearance.min != null ? fmtAlt(sampleClearance.min) : '—'}</b></span>
      <span>{$t('missionTerrain.distance')}: <b>{fmtDist(sampled.totalDist)}</b></span>
    </div>

    {#if sampled.refFallback}
      <p class="mt-note mt-warn">{$t('missionTerrain.refFallback')}</p>
    {:else if sampled.refElev == null && navWps.some((w) => w.mode === 'relative')}
      <p class="mt-note mt-warn">{$t('missionTerrain.refUnknown')}</p>
    {/if}

    {#if rewrite}
      <div class="mt-hold">
        <span class="mt-hold-head">{$t('missionTerrain.holdClearance')}</span>
        {#if rewrite.changes.length > 0}
          <p class="mt-note">{$t('missionTerrain.changesTitle', { values: { count: rewrite.changes.length } })}</p>
          <table class="mt-table">
            <tbody>
              {#each rewrite.changes as ch}
                <tr>
                  <td class="mt-wp">WP {ch.number}</td>
                  <td>{fmtStored(ch.oldAltM)} → <b>{fmtStored(ch.newAltM)}</b></td>
                  <td class="mt-mode">{modeLabel(ch.mode)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if !editing}<p class="mt-note mt-warn">{$t('missionTerrain.needEdit')}</p>{/if}
          <button class="mt-apply" disabled={!editing || applying} onclick={applyRewrite}>
            {$t('missionTerrain.apply')}
          </button>
        {:else if sampled.anyTerrain}
          <p class="mt-note mt-ok">✓ {$t('missionTerrain.allClear', { values: { clearance: clearanceM } })}</p>
        {/if}
        {#if rewrite.skipped.length > 0}
          <p class="mt-note">{$t('missionTerrain.skippedTitle')}</p>
          <ul class="mt-skipped">
            {#each rewrite.skipped as sk}
              <li>WP {sk.number} — {$t(sk.reason === 'noData' ? 'missionTerrain.skipNoData' : 'missionTerrain.skipNoRef')}</li>
            {/each}
          </ul>
        {/if}
        {#if applyStatus}<p class="mt-note mt-ok">{applyStatus}</p>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .mt-panel { padding: 8px; display: flex; flex-direction: column; gap: 8px; font-size: 12px; color: #ccc; }
  .mt-head { font-size: 13px; font-weight: 700; color: #37a8db; }
  .mt-ctrl { display: flex; align-items: center; justify-content: space-between; gap: 8px; color: #b8b8b8; }
  .mt-chart { width: 100%; height: auto; background: #191919; border: 1px solid #333; border-radius: 4px; }
  .mt-terrain { fill: rgba(110, 91, 70, 0.55); stroke: #9a7d5c; stroke-width: 1; }
  .mt-path { fill: none; stroke-width: 1.8; }
  .mt-path-ok { stroke: #37a8db; }
  .mt-path-below { stroke: #e74c3c; }
  .mt-path-unknown { stroke: #777; stroke-dasharray: 3 2; }
  .mt-preview { fill: none; stroke: #2ecc71; stroke-width: 1.4; stroke-dasharray: 5 3; }
  .mt-dot { fill: #37a8db; stroke: #0e2a3a; stroke-width: 0.8; }
  .mt-wpnum { fill: #e0e0e0; font-size: 8px; text-anchor: middle; }
  .mt-axis { fill: #777; font-size: 8px; }
  .mt-axis-r { text-anchor: end; }
  .mt-legend { display: flex; flex-wrap: wrap; gap: 4px 10px; font-size: 10px; color: #949494; }
  .mt-legend .lg { display: flex; align-items: center; gap: 5px; }
  .mt-legend .sw { width: 12px; height: 3px; border-radius: 2px; display: inline-block; }
  .sw.terrain { background: #9a7d5c; height: 7px; }
  .sw.path { background: #37a8db; }
  .sw.below { background: #e74c3c; }
  .sw.unknown { background: #777; }
  .sw.preview { background: repeating-linear-gradient(90deg, #2ecc71 0 4px, transparent 4px 7px); }
  .mt-stats { display: flex; justify-content: space-between; gap: 8px; color: #949494; font-size: 11px; }
  .mt-stats b { color: #e0e0e0; font-variant-numeric: tabular-nums; }
  .mt-stats b.warn { color: #e74c3c; }
  .mt-note { margin: 0; font-size: 11px; color: #949494; line-height: 1.4; }
  .mt-warn { color: #e0a030; }
  .mt-ok { color: #2ecc71; }
  .mt-hold { border-top: 1px solid rgba(255, 255, 255, 0.08); padding-top: 8px; display: flex; flex-direction: column; gap: 6px; }
  .mt-hold-head { font-size: 12px; font-weight: 600; color: #cdd6db; }
  .mt-table { width: 100%; border-collapse: collapse; font-size: 11.5px; }
  .mt-table td { padding: 2px 4px; border-bottom: 1px solid #2a2a2a; font-variant-numeric: tabular-nums; }
  .mt-table .mt-wp { color: #37a8db; font-weight: 600; width: 52px; }
  .mt-table .mt-mode { color: #8bc34a; text-align: right; width: 44px; }
  .mt-skipped { margin: 0; padding-left: 16px; font-size: 11px; color: #949494; line-height: 1.45; }
  .mt-apply {
    background: rgba(46, 204, 113, 0.2); border: 1px solid #2ecc71; color: #2ecc71;
    border-radius: 6px; padding: 7px 10px; font-size: 12px; font-weight: 600; cursor: pointer;
  }
  .mt-apply:hover:not(:disabled) { background: rgba(46, 204, 113, 0.32); }
  .mt-apply:disabled { opacity: 0.45; cursor: default; }
</style>
