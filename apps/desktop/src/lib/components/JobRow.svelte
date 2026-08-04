<script lang="ts">
  import type { Job } from '../types';
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { toast } from '../stores/toast.svelte';
  import { stageLabel, fmtDuration, overallPct } from '../format';
  import ProgressBar from './ProgressBar.svelte';
  import StageIcon from './StageIcon.svelte';

  type Props = { job: Job };
  let { job }: Props = $props();

  const isActive = $derived(jobsStore.activeId === job.id);
  const isTerminal = $derived(
    job.stage === 'done' || job.stage === 'failed' || job.stage === 'cancelled'
  );

  function select() { jobsStore.select(job.id); }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      select();
    }
  }

  let saving = $state(false);
  let revealing = $state(false);

  async function openJobFolder() {
    revealing = true;
    try {
      const workdir = await api.getJobWorkdir(job.id);
      await api.revealInFolder(workdir);
    } catch (err) {
      toast.error('Не удалось открыть папку: ' + (err as Error).message);
    } finally {
      revealing = false;
    }
  }

  async function saveJob() {
    if (saving) return;
    const destDir = await api.pickFolder('Куда сохранить копию задачи');
    if (!destDir) return;
    saving = true;
    try {
      const savedPath = await api.saveJob(job.id, destDir);
      toast.success(`Сохранено: ${savedPath}`);
    } catch (err) {
      toast.error('Ошибка сохранения: ' + (err as Error).message);
    } finally {
      saving = false;
    }
  }

  async function cancelJob(e: MouseEvent) {
    e.stopPropagation();
    try {
      await jobsStore.cancel(job.id);
      toast.info('Задача отменяется…');
    } catch (err) {
      toast.error((err as Error).message);
    }
  }

  const stageClass = $derived(
    job.stage === 'done' ? 'ok' :
    job.stage === 'failed' ? 'err' :
    job.stage === 'cancelled' ? 'muted' : 'info',
  );

  const host = $derived.by(() => {
    if (job.source === 'local_file') return '';
    try { return new URL(job.url).hostname; } catch { return ''; }
  });

  const createdDate = $derived.by(() => {
    try {
      const d = new Date(job.createdAt);
      const today = new Date();
      const sameDay = d.toDateString() === today.toDateString();
      if (sameDay) return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      return d.toLocaleDateString([], { day: 'numeric', month: 'short' });
    } catch { return ''; }
  });

  const pctDisplay = $derived(Math.round(overallPct(job) * 100));

  /** Позиция в очереди: сколько не-терминальных задач создано раньше. */
  const queuePos = $derived(
    job.stage === 'queued'
      ? jobsStore.jobs.filter((j) => {
          if (j.id === job.id) return false;
          if (j.stage === 'done' || j.stage === 'failed' || j.stage === 'cancelled') return false;
          return j.createdAt < job.createdAt;
        }).length
      : 0
  );

  async function retryJob(e: MouseEvent) {
    e.stopPropagation();
    try {
      if (job.source === 'url') await jobsStore.add(job.url);
      else await jobsStore.addLocal(job.url);
      toast.info('Задача добавлена повторно');
    } catch (err) {
      toast.error((err as Error).message);
    }
  }
</script>

<div
  class="row"
  class:active={isActive}
  class:done={job.stage === 'done'}
  class:failed={job.stage === 'failed'}
  class:cancelled={job.stage === 'cancelled'}
  role="button"
  tabindex="0"
  aria-label={`Задача: ${job.media?.title ?? job.url}`}
  onclick={select}
  onkeydown={onKey}
>
  <div class="top">
    {#if job.media?.thumbnail}
      <img class="thumb" src={job.media.thumbnail} alt="" loading="lazy" referrerpolicy="no-referrer" />
    {/if}
    <span class="title">{job.media?.title ?? job.url}</span>
    <span class="badge {stageClass}"><StageIcon stage={job.stage} /> {stageLabel(job.stage)}</span>
  </div>
  <div class="progress-row">
    <ProgressBar pct={overallPct(job)} />
    <span class="pct">{pctDisplay}%</span>
  </div>
  {#if job.progress.label}
    <div class="label" title={job.progress.label}>{job.progress.label}</div>
  {/if}
  {#if job.progress.speed || job.progress.eta}
    <div class="speed">
      {#if job.progress.speed}<span class="speed-val">{job.progress.speed}</span>{/if}
      {#if job.progress.eta}<span class="eta">ETA {job.progress.eta}</span>{/if}
    </div>
  {/if}
  {#if job.stage === 'done' && job.transcriptPreview}
    <div class="preview" title={job.transcriptPreview}>{job.transcriptPreview.slice(0, 160)}…</div>
  {/if}
  <div class="meta">
    <span class="dim">
      {#if job.source === 'local_file'}
        <svg class="meta-icon" width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 5c0-.6.4-1 1-1h3.5l1.5 2H13c.6 0 1 .4 1 1v6c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1V5z"/></svg>
      {/if}
      {host}
    </span>
    {#if job.media?.durationSec}<span class="dim">{fmtDuration(job.media.durationSec)}</span>{/if}
    {#if queuePos > 0}<span class="dim">в очереди: {queuePos}</span>{/if}
    {#if createdDate}<span class="dim date">{createdDate}</span>{/if}
    {#if job.error}<span class="err" title={job.error}>{job.error.slice(0, 60)}</span>{/if}
    {#if isTerminal}
      <button class="action-btn" type="button" onclick={openJobFolder} disabled={revealing} title="Открыть папку с файлами задачи">
        {#if revealing}
          …
        {:else}
          <svg class="btn-icon" width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 5c0-.6.4-1 1-1h3.5l1.5 2H13c.6 0 1 .4 1 1v6c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1V5z"/></svg>
          папка
        {/if}
      </button>
      {#if job.stage === 'failed' || job.stage === 'cancelled'}
        <button class="action-btn retry" type="button" onclick={retryJob} title="Повторить ту же задачу">
          <svg class="btn-icon" width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13.5 8a5.5 5.5 0 1 1-1.61-3.89"/><path d="M13.5 1.5V5h-3.5"/></svg>
          повторить
        </button>
      {/if}
      <button class="action-btn save" type="button" onclick={saveJob} disabled={saving} title="Сохранить копию папки в выбранное место">
        {#if saving}
          …
        {:else}
          <svg class="btn-icon" width="11" height="11" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 2.5h10v11H3z"/><path d="M5.5 2.5V6h5V2.5"/><path d="M5 9.5h6V13H5z"/></svg>
          сохранить
        {/if}
      </button>
    {/if}
    {#if !isTerminal}
      <button class="cancel-btn" type="button" onclick={cancelJob} aria-label="Отменить задачу" title="Отменить">×</button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex; flex-direction: column; gap: 4px;
    width: 100%; text-align: left;
    background: transparent;
    border: 0; border-left: 2px solid transparent;
    padding: 8px 10px;
    color: inherit; cursor: pointer;
    border-radius: 4px;
    transition: background 80ms;
  }
  .row:hover { background: var(--surface-2); }
  .row:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  .row.active { background: var(--surface-2); border-left-color: var(--accent); }
  .row.done { border-left-color: var(--ok); }
  .row.failed { border-left-color: var(--err); }
  .row.cancelled { border-left-color: var(--border); opacity: 0.7; }
  .top { display: flex; justify-content: space-between; align-items: center; gap: 6px; }
  .thumb {
    width: 30px; height: 18px; object-fit: cover;
    border-radius: 2px; flex-shrink: 0;
    background: var(--surface-3);
  }
  .title {
    font-size: 12px; font-weight: 500;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  .badge {
    font-size: 10px; padding: 1px 6px; border-radius: 3px;
    text-transform: uppercase; letter-spacing: 0.04em; flex-shrink: 0;
    background: var(--surface-3);
    display: flex; align-items: center; gap: 3px;
  }
  .badge.ok { color: var(--ok); }
  .badge.err { color: var(--err); }
  .badge.muted { color: var(--muted); }
  .badge.info { color: var(--accent); }
  .progress-row { display: flex; align-items: center; gap: 6px; }
  .pct {
    font-size: 11px; font-family: var(--mono); min-width: 30px;
    color: var(--muted); text-align: right;
  }
  .label {
    font-size: 11px; color: var(--fg);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .meta { display: flex; gap: 6px; align-items: center; font-size: 11px; flex-wrap: wrap; }
  .speed { display: flex; gap: 8px; align-items: center; font-size: 11px; }
  .speed-val {
    color: var(--accent); font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .eta {
    color: var(--warn); font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .preview {
    font-family: var(--mono); font-size: 10px; color: var(--muted);
    line-height: 1.45; max-height: 2.9em; overflow: hidden;
    white-space: pre-wrap; word-break: break-word;
  }
  .dim { color: var(--muted); }
  .date { margin-left: auto; }
  .err { color: var(--err); }
  .link { color: var(--accent); cursor: pointer; background: none; border: 0; padding: 0; font: inherit; font-size: 11px; }
  .link:focus-visible { outline: 1px solid var(--accent); border-radius: 2px; }
  .cancel-btn {
    background: transparent; border: 0; color: var(--muted);
    cursor: pointer; padding: 0 4px; font-size: 14px; line-height: 1;
    border-radius: 2px;
  }
  .cancel-btn:hover { color: var(--err); background: var(--surface-3); }
  .cancel-btn:focus-visible { outline: 1px solid var(--err); }
  .action-btn {
    background: transparent; border: 1px solid var(--border); color: var(--muted);
    cursor: pointer; padding: 2px 7px; font-size: 11px; line-height: 1.4;
    border-radius: 3px; font-family: inherit;
    display: inline-flex; align-items: center; gap: 3px;
    transition: background 80ms, color 80ms;
  }
  .btn-icon { flex-shrink: 0; }
  .meta-icon { vertical-align: -1px; margin-right: 2px; }
  .action-btn:hover:not(:disabled) { background: var(--surface-2); color: var(--fg); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .action-btn.save:hover:not(:disabled) { color: var(--accent); border-color: var(--accent); }
  .action-btn.retry:hover:not(:disabled) { color: var(--warn); border-color: var(--warn); }
  .action-btn:focus-visible { outline: 1px solid var(--accent); }
</style>
