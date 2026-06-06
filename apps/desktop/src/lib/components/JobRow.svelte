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

  async function openResult(e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    if (job.transcriptPath) {
      try { await api.revealInFolder(job.transcriptPath); } catch (err) {
        toast.error('Не удалось открыть папку');
      }
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
    {#if job.transcriptPath}
      <span class="link" role="button" tabindex="0" onclick={openResult} onkeydown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); openResult(e); } }}>открыть</span>
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
</style>
