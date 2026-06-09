<script lang="ts">
  import { api } from '../api';
  import { jobsStore } from '../stores/jobs.svelte';
  import { toast } from '../stores/toast.svelte';
  import { fmtBytes } from '../format';
  import type { FileInfo } from '../types';

  let folderPath = $state('');
  let files = $state<FileInfo[]>([]);
  let selected = $state<Set<string>>(new Set());
  let scanning = $state(false);
  let processing = $state(false);
  let processed = $state(0);
  let processingTotal = $state(0);

  async function pickFolder() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({ directory: true, multiple: false });
    if (typeof result === 'string') { folderPath = result; await doScan(); }
  }

  async function doScan() {
    if (!folderPath) return;
    scanning = true; files = []; selected = new Set();
    try {
      files = await api.scanFolder(folderPath);
      selected = new Set(files.map(f => f.path));
      if (files.length > 0) toast.info(`${files.length} файлов`);
    } catch (e) { toast.error((e as Error).message); }
    finally { scanning = false; }
  }

  function toggle(path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path); else next.add(path);
    selected = next;
  }
  function toggleAll() {
    selected = selected.size === files.length ? new Set() : new Set(files.map(f => f.path));
  }

  async function processAll() {
    const paths = files.filter(f => selected.has(f.path)).map(f => f.path);
    if (paths.length === 0) { toast.error('Выберите файлы'); return; }
    processing = true;
    processed = 0;
    processingTotal = paths.length;
    let ok = 0, fail = 0;
    try {
      for (const p of paths) {
        try { await jobsStore.addLocal(p); ok++; }
        catch (e) { fail++; toast.error((e as Error).message); }
        processed++;
      }
      if (ok > 0) toast.success(`${ok} файлов в очереди`);
      if (fail > 0) toast.error(`${fail} ошибок`);
    } finally { processing = false; }
  }

  const allSelected = $derived(files.length > 0 && selected.size === files.length);
  const totalSize = $derived(files.filter(f => selected.has(f.path)).reduce((s, f) => s + f.sizeBytes, 0));
  const progressPct = $derived(processingTotal > 0 ? processed / processingTotal : 0);
</script>

<div class="local">
  <div class="pick-row">
    <button class="btn primary" type="button" onclick={pickFolder} disabled={scanning || processing}>
      {scanning ? 'Сканирование…' : 'Выбрать папку'}
    </button>
    {#if folderPath}<span class="path" title={folderPath}>{folderPath}</span>{/if}
  </div>

  {#if scanning}
    <div class="empty">Сканирование…</div>
  {:else if folderPath && files.length === 0}
    <div class="empty">В папке нет аудио/видео файлов.</div>
  {:else if !folderPath}
    <div class="empty">
      <p>Выберите папку с аудио/видео файлами.</p>
      <p class="dim">Рекурсивный поиск: wav, mp3, flac, mp4, mkv, webm и др.</p>
    </div>
  {:else}
    <div class="toolbar">
      <label class="check"><input type="checkbox" checked={allSelected} onchange={toggleAll} />Все ({files.length})</label>
      <span class="dim">{fmtBytes(totalSize)}</span>
      <button class="btn primary" type="button" onclick={processAll} disabled={processing || selected.size === 0}>
        {processing
          ? `${processed} / ${processingTotal}`
          : `Обработать (${selected.size})`}
      </button>
    </div>
    {#if processing}
      <div class="progress" role="progressbar" aria-valuenow={Math.round(progressPct * 100)} aria-valuemin="0" aria-valuemax="100">
        <div class="bar"><div class="fill" style="width: {progressPct * 100}%"></div></div>
        <span class="dim pct">{Math.round(progressPct * 100)}%</span>
      </div>
    {/if}
    <ul class="files" role="list">
      {#each files as file (file.path)}
        <li>
          <label class="file-row">
            <input type="checkbox" checked={selected.has(file.path)} disabled={processing} onchange={() => toggle(file.path)} />
            <span class="fname" title={file.path}>{file.name}</span>
            <span class="fext">.{file.extension}</span>
            <span class="fsize">{fmtBytes(file.sizeBytes)}</span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .local { display: flex; flex-direction: column; gap: 10px; min-height: 0; overflow: hidden; }
  .pick-row { display: flex; align-items: center; gap: 10px; flex-shrink: 0; }
  .path { font-size: 11px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; font-family: var(--mono); }
  .toolbar {
    display: flex; align-items: center; gap: 10px; flex-shrink: 0;
    background: var(--surface-1); border: 1px solid var(--border);
    border-radius: 4px; padding: 6px 10px;
  }
  .check { display: flex; align-items: center; gap: 4px; font-size: 12px; cursor: pointer; }
  .check input { cursor: pointer; }
  .dim { color: var(--muted); font-size: 11px; }
  .progress { display: flex; align-items: center; gap: 8px; }
  .bar { flex: 1; height: 4px; background: var(--surface-3); border-radius: 2px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent); transition: width 120ms; }
  .pct { min-width: 32px; text-align: right; font-family: var(--mono); font-size: 11px; }
  .files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 1px; overflow-y: auto; min-height: 0; flex: 1; }
  .file-row {
    display: flex; align-items: center; gap: 6px; padding: 4px 8px;
    border-radius: 3px; cursor: pointer; font-size: 12px; transition: background 80ms;
  }
  .file-row:hover { background: var(--surface-2); }
  .file-row input { cursor: pointer; flex-shrink: 0; }
  .file-row input:disabled { cursor: not-allowed; }
  .fname { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .fext { color: var(--muted); font-size: 11px; min-width: 40px; font-family: var(--mono); }
  .fsize { color: var(--muted); font-size: 11px; min-width: 56px; text-align: right; font-family: var(--mono); }
  .empty {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    color: var(--muted); text-align: center; padding: 32px;
    border: 1px dashed var(--border); border-radius: 4px; flex: 1; font-size: 13px;
  }
  .btn {
    background: var(--surface-2); border: 1px solid var(--border); border-radius: 4px;
    padding: 5px 12px; cursor: pointer; font-size: 12px; color: var(--fg);
  }
  .btn:hover { background: var(--surface-3); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
