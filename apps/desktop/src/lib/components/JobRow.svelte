<script lang="ts">
  import type { Job } from '../types';
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { stageLabel, fmtDuration } from '../format';
  import ProgressBar from './ProgressBar.svelte';

  type Props = { job: Job };
  let { job }: Props = $props();

  const isActive = $derived(jobsStore.activeId === job.id);

  function open() { jobsStore.select(job.id); }

  async function openResult(e: MouseEvent | KeyboardEvent) {
    e.stopPropagation();
    if (job.transcriptPath) {
      try { await api.revealInFolder(job.transcriptPath); } catch {}
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
</script>

<button class="row" class:active={isActive} class:done={job.stage === 'done'} class:failed={job.stage === 'failed'} onclick={open}>
  <div class="top">
    <span class="title">{job.media?.title ?? job.url}</span>
    <span class="badge {stageClass}">{stageLabel(job.stage)}</span>
  </div>
  <ProgressBar pct={job.progress.pct} />
  <div class="meta">
    <span class="dim">{job.source === 'local_file' ? '📁' : ''}{host}</span>
    {#if job.media?.durationSec}<span class="dim">{fmtDuration(job.media.durationSec)}</span>{/if}
    {#if job.error}<span class="err" title={job.error}>{job.error.slice(0, 60)}</span>{/if}
    {#if job.transcriptPath}<span class="link" role="button" tabindex="0" onclick={openResult} onkeydown={(e) => { if (e.key === 'Enter') openResult(e); }}>открыть</span>{/if}
  </div>
</button>

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
  .row.active { background: var(--surface-2); border-left-color: var(--accent); }
  .row.done { border-left-color: var(--ok); }
  .row.failed { border-left-color: var(--err); }
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
  .meta { display: flex; gap: 6px; align-items: center; font-size: 11px; }
  .dim { color: var(--muted); }
  .err { color: var(--err); }
  .link { color: var(--accent); cursor: pointer; background: none; border: 0; padding: 0; font: inherit; font-size: 11px; }
</style>
