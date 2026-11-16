<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (C) 2026 Rick Quast
-->

<!-- Parameters panel (rivals batch, 2026-11-16): download the FC's full parameter list, read it
     with labels (bitmask bits / enum names next to the number), and exchange it as a
     QGroundControl `.params` file. Deliberately NOT an editor: values change only by loading a
     `.params` file whose differing entries are shown and counted before anything is written,
     and writes are refused while armed. -->
<script lang="ts">
  import { t } from 'svelte-i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import PanelShell from '$lib/components/panel/PanelShell.svelte';
  import Button from '$lib/components/panel/Button.svelte';
  import { fcParams, fcParamsComplete, type FcParam } from '$lib/stores/parameters';
  import { telemetry } from '$lib/stores/telemetry';
  import { arduVehicleClass } from '$lib/stores/missionArdupilot';
  import { describeParam, formatParamValue } from '$lib/helpers/paramMeta';
  import { serializeQgcParams, parseQgcParams } from '$lib/helpers/qgcParams';

  interface RawParam { name: string; value: number; param_type: number }
  interface DownloadResult { params: RawParam[]; expected: number; complete: boolean }
  interface PendingWrite { param: FcParam; current: number | null }

  let downloading = $state(false);
  let writing = $state(false);
  let writeProgress = $state('');
  let error = $state('');
  let notice = $state('');
  let pending = $state<PendingWrite[] | null>(null);
  let pendingSkipped = $state(0);

  let armed = $derived(($telemetry.armingFlags & 0x04) !== 0);
  let rows = $derived($fcParams ? [...$fcParams].sort((a, b) => a.name.localeCompare(b.name)) : []);

  async function download() {
    downloading = true;
    error = '';
    notice = '';
    pending = null;
    try {
      const res = await invoke<DownloadResult>('mav_download_params');
      fcParams.set(res.params.map((p) => ({ name: p.name, value: p.value, paramType: p.param_type })));
      fcParamsComplete.set(res.complete);
      notice = res.complete
        ? $t('params.downloaded', { values: { count: res.params.length } })
        : $t('params.downloadedPartial', { values: { count: res.params.length, expected: res.expected } });
    } catch (e) {
      error = String(e);
    } finally {
      downloading = false;
    }
  }

  async function saveToFile() {
    if (!$fcParams) return;
    error = '';
    const path = await save({
      defaultPath: 'vehicle.params',
      filters: [{ name: 'QGroundControl parameters', extensions: ['params'] }],
    });
    if (!path) return;
    try {
      await invoke<void>('write_text_file', { path, content: serializeQgcParams($fcParams) });
      notice = $t('params.saved', { values: { count: $fcParams.length } });
    } catch (e) {
      error = String(e);
    }
  }

  async function loadFromFile() {
    error = '';
    notice = '';
    pending = null;
    const path = await open({
      multiple: false,
      filters: [{ name: 'QGroundControl parameters', extensions: ['params'] }],
    });
    if (!path || typeof path !== 'string') return;
    try {
      const text = await invoke<string>('read_text_file', { path });
      const parsed = parseQgcParams(text);
      if (parsed.params.length === 0) {
        error = $t('params.loadEmpty');
        return;
      }
      const current = new Map(($fcParams ?? []).map((p) => [p.name, p.value]));
      const diffs: PendingWrite[] = [];
      let skipped = 0;
      for (const p of parsed.params) {
        const cur = current.get(p.name);
        if (cur === undefined) { skipped++; continue; } // unknown to this FC — never write blind
        if (Math.abs(cur - p.value) > 1e-6 * Math.max(1, Math.abs(cur))) {
          diffs.push({ param: p, current: cur });
        }
      }
      pendingSkipped = skipped + parsed.badLines;
      pending = diffs;
      if (diffs.length === 0) {
        notice = $t('params.loadNoChanges', { values: { count: parsed.params.length } });
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function writePending() {
    if (!pending || armed) return;
    writing = true;
    error = '';
    let done = 0;
    const written = new Map<string, number>();
    try {
      for (const d of pending) {
        await invoke<void>('mav_set_param', { name: d.param.name, value: d.param.value });
        written.set(d.param.name, d.param.value);
        done++;
        writeProgress = `${done} / ${pending.length}`;
      }
      // Reflect the writes in the snapshot so the table (and a later save) matches the FC.
      fcParams.update((list) =>
        list ? list.map((p) => (written.has(p.name) ? { ...p, value: written.get(p.name)! } : p)) : list
      );
      notice = $t('params.written', { values: { count: done } });
      pending = null;
    } catch (e) {
      error = $t('params.writeFailed', { values: { count: done, message: String(e) } });
    } finally {
      writing = false;
      writeProgress = '';
    }
  }
</script>

<PanelShell variant="compact" title={$t('params.title')}>
  {#snippet body()}
    <div class="pp">
      <p class="pp-hint">{$t('params.hint')}</p>

      <div class="pp-row">
        <Button onclick={download} disabled={downloading || writing}>
          {downloading ? $t('params.downloading') : $t('params.download')}
        </Button>
        <Button onclick={saveToFile} disabled={!$fcParams || downloading || writing}>{$t('params.save')}</Button>
        <Button onclick={loadFromFile} disabled={downloading || writing}>{$t('params.load')}</Button>
      </div>

      {#if error}
        <p class="pp-error">{error}</p>
      {/if}
      {#if notice}
        <p class="pp-notice">{notice}</p>
      {/if}
      {#if $fcParams && !$fcParamsComplete}
        <p class="pp-warn">{$t('params.incomplete')}</p>
      {/if}

      {#if pending}
        <div class="pp-pending">
          <p>
            {$t('params.pendingSummary', { values: { count: pending.length } })}
            {#if pendingSkipped > 0}{$t('params.pendingSkipped', { values: { count: pendingSkipped } })}{/if}
          </p>
          {#if armed}
            <p class="pp-error">{$t('params.armedBlock')}</p>
          {/if}
          <div class="pp-row">
            <Button onclick={writePending} disabled={writing || armed || pending.length === 0}>
              {writing ? `${$t('params.writing')} ${writeProgress}` : $t('params.write')}
            </Button>
            <Button onclick={() => { pending = null; }} disabled={writing}>{$t('params.discard')}</Button>
          </div>
          <ul class="pp-diffs">
            {#each pending.slice(0, 50) as d}
              <li><code>{d.param.name}</code>: {formatParamValue(d.current ?? 0, d.param.paramType)} → {formatParamValue(d.param.value, d.param.paramType)}</li>
            {/each}
            {#if pending.length > 50}
              <li>… +{pending.length - 50}</li>
            {/if}
          </ul>
        </div>
      {/if}

      {#if rows.length > 0}
        <table class="pp-table">
          <thead>
            <tr><th>{$t('params.colName')}</th><th>{$t('params.colValue')}</th><th></th></tr>
          </thead>
          <tbody>
            {#each rows as p}
              <tr>
                <td><code>{p.name}</code></td>
                <td class="pp-val">{formatParamValue(p.value, p.paramType)}</td>
                <td class="pp-desc">{describeParam(p.name, p.value, $arduVehicleClass) ?? ''}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if !$fcParams}
        <p class="pp-hint">{$t('params.notDownloaded')}</p>
      {/if}
    </div>
  {/snippet}
</PanelShell>

<style>
  .pp { display: flex; flex-direction: column; gap: 10px; padding: 4px 2px; }
  .pp-hint { color: #9fb3bd; font-size: 12px; margin: 0; }
  .pp-row { display: flex; gap: 8px; flex-wrap: wrap; }
  .pp-error { color: #e74c3c; font-size: 12.5px; margin: 0; }
  .pp-warn { color: #f39c12; font-size: 12.5px; margin: 0; }
  .pp-notice { color: #27ae60; font-size: 12.5px; margin: 0; }
  .pp-pending { border: 1px solid rgba(243, 156, 18, 0.5); border-radius: 6px; padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; }
  .pp-pending p { margin: 0; font-size: 12.5px; }
  .pp-diffs { margin: 0; padding-left: 16px; font-size: 12px; color: #c8c8c8; max-height: 180px; overflow-y: auto; }
  .pp-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .pp-table th { text-align: left; color: #9fb3bd; font-weight: 600; padding: 3px 6px; border-bottom: 1px solid rgba(255,255,255,0.12); }
  .pp-table td { padding: 3px 6px; border-bottom: 1px solid rgba(255,255,255,0.05); vertical-align: top; }
  .pp-val { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .pp-desc { color: #9fb3bd; }
  code { font-family: ui-monospace, monospace; }
</style>
