<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Marc Hoffmann (b14ckyy)
-->
<script lang="ts">
  // Link-health strip for the FPV panel (Tailwind): rolling 60 s sparklines (hand-rolled
  // SVG, no chart lib) pinned to the bottom of the video surface. Every number is REAL or
  // a dash — nothing is estimated:
  //  • Video bitrate — measured by the backend from bytes actually received on the RTP/UDP
  //    socket (the native client's live counters, the same ones the Debug Monitor shows).
  //  • RTP loss % — per-second loss from the depacketizer's sequence counters
  //    (received vs lost over each interval; a second with no packets is a GAP in the
  //    graph, not a zero).
  //  • Telemetry RSSI / LQ — the FC link's own link stats (`telemetry.link`, from CRSF /
  //    MSP / MAVLink as the protocol provides). MAVLink RADIO_STATUS packet-loss % is not
  //    decoded by the pinned dialect anywhere in this app, so telemetry loss is NOT shown
  //    rather than invented. wfb-ng's FEC/antenna stats live behind its own service
  //    interface, which this app does not speak — also not shown.
  import { t } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { nativeRtspStats } from '$lib/stores/video';
  import { telemetry } from '$lib/stores/telemetry';

  const WINDOW = 60; // seconds of history
  const W = 72;
  const H = 16;

  let bitrateHist = $state<(number | null)[]>([]);
  let lossHist = $state<(number | null)[]>([]);
  let rssiHist = $state<(number | null)[]>([]);
  let kbps = $state<number | null>(null);
  let lossPct = $state<number | null>(null);
  /** Telemetry throughput from the always-on backend link meter (`link-stats` event). */
  let teleMsgRx = $state<number | null>(null);

  let lastRecv = -1;
  let lastLost = -1;

  function push(hist: (number | null)[], v: number | null): (number | null)[] {
    const next = hist.length >= WINDOW ? hist.slice(hist.length - WINDOW + 1) : hist.slice();
    next.push(v);
    return next;
  }

  $effect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<{ msg_per_sec_rx: number }>('link-stats', (e) => {
      teleMsgRx = e.payload.msg_per_sec_rx;
    }).then((u) => (unlisten = u));
    const id = setInterval(() => {
      const ns = get(nativeRtspStats);
      const tm = get(telemetry);

      kbps = ns ? Math.round(ns.kbps) : null;
      bitrateHist = push(bitrateHist, kbps);

      if (ns && lastRecv >= 0) {
        const dRecv = ns.rtpReceived - lastRecv;
        const dLost = ns.rtpLost - lastLost;
        lossPct = dRecv + dLost > 0 ? (100 * dLost) / (dRecv + dLost) : null;
      } else {
        lossPct = null;
      }
      if (ns) {
        lastRecv = ns.rtpReceived;
        lastLost = ns.rtpLost;
      }
      lossHist = push(lossHist, lossPct);

      rssiHist = push(rssiHist, tm.link.rssiPercent);
    }, 1000);
    return () => {
      clearInterval(id);
      unlisten?.();
    };
  });

  /** Polyline point-strings, one per contiguous non-null run (a null is a gap, never a 0). */
  function segments(values: (number | null)[], fixedMax?: number): string[] {
    const nums = values.filter((v): v is number => v !== null);
    if (nums.length === 0) return [];
    const hi = fixedMax ?? Math.max(...nums, 1);
    const out: string[] = [];
    let cur: string[] = [];
    values.forEach((v, i) => {
      if (v === null) {
        if (cur.length > 1) out.push(cur.join(' '));
        cur = [];
        return;
      }
      const x = values.length > 1 ? (i / (values.length - 1)) * W : W / 2;
      const y = H - 1.5 - (Math.min(v, hi) / hi) * (H - 3);
      cur.push(`${x.toFixed(1)},${y.toFixed(1)}`);
    });
    if (cur.length > 1) out.push(cur.join(' '));
    return out;
  }

  const fmtKbps = (v: number | null): string =>
    v === null ? '–' : v >= 1000 ? `${(v / 1000).toFixed(1)} Mb/s` : `${v} kb/s`;
  const fmtPct = (v: number | null): string => (v === null ? '–' : `${v.toFixed(v >= 10 ? 0 : 1)} %`);
</script>

<div class="link-strip" aria-hidden="true">
  <span class="ls-group">
    <span class="ls-label">{$t('video.link.video')}</span>
    <svg class="spark" viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      {#each segments(bitrateHist) as seg}<polyline points={seg} />{/each}
    </svg>
    <span class="ls-val">{fmtKbps(kbps)}</span>
    <span class="ls-sub">{$t('video.link.rtpLoss')}</span>
    <svg class="spark loss" viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      {#each segments(lossHist, 25) as seg}<polyline points={seg} />{/each}
    </svg>
    <span class="ls-val">{fmtPct(lossPct)}</span>
  </span>
  <span class="ls-sep"></span>
  <span class="ls-group">
    <span class="ls-label">{$t('video.link.telemetry')}</span>
    <span class="ls-sub">{$t('video.link.rssi')}</span>
    <svg class="spark" viewBox="0 0 {W} {H}" preserveAspectRatio="none">
      {#each segments(rssiHist, 100) as seg}<polyline points={seg} />{/each}
    </svg>
    <span class="ls-val">{fmtPct($telemetry.link.rssiPercent)}</span>
    <span class="ls-sub">{$t('video.link.lq')}</span>
    <span class="ls-val">{$telemetry.link.lq === null ? '–' : `${Math.round($telemetry.link.lq)} %`}</span>
    <span class="ls-sub">rx</span>
    <span class="ls-val">{teleMsgRx === null ? '–' : `${teleMsgRx.toFixed(0)}/s`}</span>
  </span>
</div>

<style>
  .link-strip {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 8px;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.72), rgba(0, 0, 0, 0.35) 80%, transparent);
    pointer-events: none;
    z-index: 6;
    font-family: 'Consolas', 'JetBrains Mono', monospace;
    font-size: 10.5px;
    letter-spacing: 0.03em;
    color: #cfe6f2;
    white-space: nowrap;
    overflow: hidden;
  }
  .ls-group { display: inline-flex; align-items: center; gap: 5px; min-width: 0; }
  .ls-label { font-weight: 700; color: #9ad0e8; text-transform: uppercase; }
  .ls-sub { color: #8fa8b5; }
  .ls-val { font-variant-numeric: tabular-nums; }
  .ls-sep { width: 1px; height: 12px; background: rgba(255, 255, 255, 0.25); flex: 0 0 auto; }
  .spark {
    width: 54px;
    height: 13px;
    flex: 0 0 auto;
  }
  .spark polyline {
    fill: none;
    stroke: #4fc47a;
    stroke-width: 1.4;
    vector-effect: non-scaling-stroke;
  }
  .spark.loss polyline { stroke: #ffb02e; }
</style>
