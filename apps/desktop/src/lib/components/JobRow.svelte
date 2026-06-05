<script lang="ts">
  import type { Job } from '../types';
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { stageLabel, fmtBytes } from '../format';
  import ProgressBar from './ProgressBar.svelte';

  type Props = { job: Job };
  let { job }: Props = $props();

  const isActive = $derived(jobsStore.activeId === job.id);
  const isRunning = $derived(
    job.stage !== 'done' && job.stage !== 'failed' && job.stage !== 'cancelled',
  );

  function open() {
    jobsStore.select(job.id);
  }

  async function openResult() {
    if (job.transcriptPath) {
      try {
        await api.revealInFolder(job.transcriptPath);
      } catch (e) {
        console.warn(e);
      }
    }
  }

  const stageClass = $derived(
    job.stage === 'done' ? 'ok' :
    job.stage === 'failed' ? 'err' :
    job.stage === 'cancelled' ? 'muted' :
    'info',
  );
</script>

<button class="row {isActive ? 'active' : ''}" class:done={job.stage === 'done'} class:failed={job.stage === 'failed'} onclick={open}>
  <div class="head">
    <span class="title">{job.media?.title ?? job.url}</span>
    <span class="stage {stageClass}">{stageLabel(job.stage)}</span>
  </div>
  <ProgressBar
    pct={job.progress.pct}
    label={`${Math.round(job.progress.pct * 100)}% · ${job.progress.label}${job.progress.speed ? ' · ' + job.progress.speed : ''}${job.progress.eta ? ' · ETA ' + job.progress.eta : ''}`}
  />
  <div class="meta">
    <span class="dim" title={job.url}>{new URL(job.url).hostname}</span>
    {#if job.media?.durationSec}
      <span class="dim">· {fmtBytes(0)}???</span>
    {/if}
    {#if job.error}
      <span class="err" title={job.error}>{job.error.slice(0, 80)}</span>
    {/if}
    {#if job.transcriptPath}
      <button class="link" onclick={(e) => { e.stopPropagation(); openResult(); }}>открыть</button>
    {/if}
  </div>
</button>

<style>
  .row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    text-align: left;
    background: var(--surface-1, #14171c);
    border: 1px solid var(--border, #2a2e35);
    border-radius: 10px;
    padding: 10px 12px;
    color: inherit;
    cursor: pointer;
    transition: background 80ms, border-color 80ms;
  }
  .row:hover { background: var(--surface-2, #1c1f24); }
  .row.active { border-color: var(--accent, #5b8def); }
  .row.done { border-left: 3px solid var(--ok, #38c172); }
  .row.failed { border-left: 3px solid var(--err, #e5484d); }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .title {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .stage {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border, #2a2e35);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .stage.ok    { color: var(--ok, #38c172); border-color: color-mix(in oklab, var(--ok, #38c172) 40%, transparent); }
  .stage.err   { color: var(--err, #e5484d); border-color: color-mix(in oklab, var(--err, #e5484d) 40%, transparent); }
  .stage.muted { color: var(--muted, #8a93a3); }
  .stage.info  { color: var(--accent, #5b8def); border-color: color-mix(in oklab, var(--accent, #5b8def) 40%, transparent); }
  .meta {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 12px;
  }
  .dim { color: var(--muted, #8a93a3); }
  .err { color: var(--err, #e5484d); }
  .link {
    background: none;
    border: 0;
    color: var(--accent, #5b8def);
    cursor: pointer;
    padding: 0;
    font: inherit;
  }
</style>
