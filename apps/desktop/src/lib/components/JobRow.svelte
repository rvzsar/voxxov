<script lang="ts">
  import type { Job } from '../types';
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { toast } from '../stores/toast.svelte';
  import { stageLabel, fmtDuration } from '../format';
  import ProgressBar from './ProgressBar.svelte';

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
      // "21:34" or "21:34 5 мар"
      const today = new Date();
      const sameDay = d.toDateString() === today.toDateString();
      if (sameDay) return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
      return d.toLocaleDateString([], { day: 'numeric', month: 'short' });
    } catch { return ''; }
  });
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
    <span class="title">{job.media?.title ?? job.url}</span>
    <span class="badge {stageClass}">{stageLabel(job.stage)}</span>
  </div>
  <ProgressBar pct={job.progress.pct} />
  <div class="meta">
    <span class="dim">{job.source === 'local_file' ? '📁' : ''}{host}</span>
    {#if job.media?.durationSec}<span class="dim">{fmtDuration(job.media.durationSec)}</span>{/if}
    {#if createdDate}<span class="dim date">{createdDate}</span>{/if}
    {#if job.error}<span class="err" title={job.error}>{job.error.slice(0, 60)}</span>{/if}
    {#if isTerminal}
      <button class="action-btn" type="button" onclick={openJobFolder} disabled={revealing} title="Открыть папку с файлами задачи">
        {revealing ? '…' : '📁 папка'}
      </button>
      <button class="action-btn save" type="button" onclick={saveJob} disabled={saving} title="Сохранить копию папки в выбранное место">
        {saving ? '…' : '💾 сохранить'}
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
    padding: 6px 8px;
    color: inherit; cursor: pointer;
    border-radius: 3px;
    transition: background 80ms;
  }
  .row:hover { background: var(--surface-2); }
  .row:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
  .row.active { background: var(--surface-2); border-left-color: var(--accent); }
  .row.done { border-left-color: var(--ok); }
  .row.failed { border-left-color: var(--err); }
  .row.cancelled { border-left-color: var(--border); opacity: 0.7; }
  .top { display: flex; justify-content: space-between; align-items: center; gap: 6px; }
  .title {
    font-size: 12px; font-weight: 500;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  .badge {
    font-size: 10px; padding: 1px 6px; border-radius: 3px;
    text-transform: uppercase; letter-spacing: 0.04em; flex-shrink: 0;
    background: var(--surface-3);
  }
  .badge.ok { color: var(--ok); }
  .badge.err { color: var(--err); }
  .badge.muted { color: var(--muted); }
  .badge.info { color: var(--accent); }
  .meta { display: flex; gap: 6px; align-items: center; font-size: 11px; flex-wrap: wrap; }
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
    cursor: pointer; padding: 1px 6px; font-size: 11px; line-height: 1.4;
    border-radius: 3px; font-family: inherit;
    transition: background 80ms, color 80ms;
  }
  .action-btn:hover:not(:disabled) { background: var(--surface-2); color: var(--fg); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .action-btn.save:hover:not(:disabled) { color: var(--accent); border-color: var(--accent); }
  .action-btn:focus-visible { outline: 1px solid var(--accent); }
</style>
