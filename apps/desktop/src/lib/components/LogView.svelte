<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { toast } from '../stores/toast.svelte';
  import { stageLabel, fmtDuration, overallPct } from '../format';
  import StageIcon from './StageIcon.svelte';

  const job = $derived(jobsStore.active());
  const logs = $derived(jobsStore.logsFor(jobsStore.activeId));

  let logPre: HTMLPreElement | null = $state(null);
  let previewExpanded = $state(false);

  let userScrolled = $state(false);
  function onLogScroll() {
    if (!logPre) return;
    const atBottom =
      logPre.scrollHeight - logPre.scrollTop - logPre.clientHeight < 20;
    userScrolled = !atBottom;
  }
  $effect(() => {
    void logs.length;
    if (logPre && !userScrolled) {
      logPre.scrollTop = logPre.scrollHeight;
    }
  });

  async function openInFolder() {
    if (job?.transcriptPath) {
      try { await api.revealInFolder(job.transcriptPath); } catch (e) {
        toast.error('Не удалось открыть папку');
      }
    }
  }

  async function copyTranscript() {
    if (!job?.transcriptPreview) return;
    try {
      await navigator.clipboard.writeText(job.transcriptPreview);
      toast.success('Скопировано в буфер');
    } catch {
      toast.error('Не удалось скопировать');
    }
  }

  const previewShort = $derived.by(() => {
    const t = job?.transcriptPreview?.trim() ?? '';
    if (previewExpanded || t.length <= 280) return t;
    return [...t].slice(0, 280).join('') + '…';
  });
  const canExpand = $derived((job?.transcriptPreview?.trim().length ?? 0) > 280);

  const isActive = $derived(
    job && !['done', 'failed', 'cancelled'].includes(job.stage)
  );
</script>

{#if !job}
  <div class="empty">
    <p>Выберите задачу для просмотра деталей.</p>
  </div>
{:else}
  <div class="detail">
    <div class="head">
      <div class="info">
        <div class="title">{job.media?.title ?? job.url}</div>
        <div class="url">{job.url}</div>
      </div>
      <span class="badge"><StageIcon stage={job.stage} /> {stageLabel(job.stage)}</span>
    </div>

    <div class="meta">
      <span class="dim">Создано:</span> {new Date(job.createdAt).toLocaleString()}
      {#if job.finishedAt}
        <span class="dim"> · Завершено:</span> {new Date(job.finishedAt).toLocaleString()}
      {/if}
      {#if job.media?.uploader}
        <span class="dim"> · Канал:</span> {job.media.uploader}
      {/if}
      {#if job.media?.durationSec}
        <span class="dim"> · Длительность:</span> {fmtDuration(job.media.durationSec)}
      {/if}
    </div>

    {#if isActive}
      <div class="activity">
        <div class="activity-header">
          <span class="activity-icon"><StageIcon stage={job.stage} size={18} /></span>
          <span class="activity-label">{job.progress.label || stageLabel(job.stage)}</span>
        </div>
        <div class="activity-bar">
          <div class="activity-fill" style="width: {(overallPct(job) * 100).toFixed(1)}%"></div>
        </div>
        <div class="activity-stats">
          <span class="activity-pct">{Math.round(overallPct(job) * 100)}%</span>
          {#if job.progress.speed}
            <span class="activity-speed">{job.progress.speed}</span>
          {/if}
          {#if job.progress.eta}
            <span class="activity-eta">ETA {job.progress.eta}</span>
          {/if}
        </div>
      </div>
    {:else}
      <div class="progress">
        <div class="pct">{Math.round(overallPct(job) * 100)}%</div>
        <div class="bar"><div class="fill" style="width: {(overallPct(job) * 100).toFixed(1)}%"></div></div>
        <div class="lbl" title={job.progress.label}>{job.progress.label}</div>
      </div>
    {/if}

    {#if job.error}
      <pre class="err">{job.error}</pre>
    {/if}

    {#if job.transcriptPreview?.trim()}
      <div class="preview">
        <div class="ph">
          <span class="label">Превью</span>
          <span class="ph-actions">
            <button class="link" type="button" onclick={copyTranscript}>копировать</button>
            {#if job.transcriptPath}
              <span class="sep">·</span>
              <button class="link" type="button" onclick={openInFolder}>открыть файл</button>
            {/if}
          </span>
        </div>
        <pre>{previewShort}</pre>
        {#if canExpand}
          <button class="more" type="button" onclick={() => (previewExpanded = !previewExpanded)}>
            {previewExpanded ? 'свернуть' : 'показать полностью'}
          </button>
        {/if}
      </div>
    {/if}

    {#if logs.length > 0}
      <div class="logs">
        <div class="ph">
          <span class="label">Лог ({logs.length}{userScrolled ? ', новее внизу ↓' : ''})</span>
          <button class="link" type="button" onclick={() => { userScrolled = false; if (logPre) logPre.scrollTop = logPre.scrollHeight; }} title="Прокрутить вниз">↓</button>
        </div>
        <pre class="log-pre" bind:this={logPre} onscroll={onLogScroll}>{logs.join('\n')}</pre>
      </div>
    {/if}
  </div>
{/if}

<style>
  .empty { height: 100%; display: flex; align-items: center; justify-content: center; color: var(--muted); font-size: 13px; }
  .detail { display: flex; flex-direction: column; gap: 12px; padding: 2px; font-size: 12px; }
  .head { display: flex; justify-content: space-between; align-items: flex-start; gap: 10px; }
  .info { min-width: 0; }
  .title { font-size: 14px; font-weight: 600; }
  .url { color: var(--muted); font-size: 11px; font-family: var(--mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge {
    font-size: 10px; padding: 2px 8px; border-radius: 3px;
    text-transform: uppercase; letter-spacing: 0.04em;
    background: var(--surface-3); color: var(--muted); flex-shrink: 0;
    display: flex; align-items: center; gap: 4px;
  }
  .meta { font-size: 12px; color: var(--fg); }
  .dim { color: var(--muted); }

  .activity {
    background: var(--surface-1);
    border: 1px solid var(--accent);
    border-radius: 6px;
    padding: 12px;
    display: flex; flex-direction: column; gap: 8px;
  }
  .activity-header {
    display: flex; align-items: center; gap: 8px;
  }
  .activity-icon { display: flex; align-items: center; color: var(--accent); }
  .activity-label {
    font-size: 13px; font-weight: 500; color: var(--fg);
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .activity-bar {
    height: 8px; background: var(--surface-3);
    border-radius: 4px; overflow: hidden;
  }
  .activity-fill {
    height: 100%; background: var(--accent);
    border-radius: 4px;
    transition: width 150ms ease;
  }
  .activity-stats {
    display: flex; gap: 12px; align-items: center;
    font-size: 12px;
  }
  .activity-pct {
    font-family: var(--mono); font-weight: 600;
    color: var(--accent); min-width: 36px;
  }
  .activity-speed {
    color: var(--accent); font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .activity-eta {
    color: var(--warn); font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .progress { display: flex; align-items: center; gap: 8px; }  .pct { font-size: 12px; font-family: var(--mono); min-width: 32px; color: var(--muted); }
  .bar { flex: 1; height: 4px; background: var(--surface-3); border-radius: 2px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent); transition: width 120ms; }
  .lbl { font-size: 11px; color: var(--muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px; }

  pre {
    background: var(--surface-2); border: 1px solid var(--border);
    border-radius: 4px; padding: 8px 10px; margin: 0;
    white-space: pre-wrap; word-wrap: break-word;
    font-family: var(--mono); font-size: 12px; line-height: 1.5;
    max-height: 300px; overflow: auto;
  }
  pre.err { color: var(--err); border-color: var(--err); }
  pre.log-pre { max-height: 240px; font-size: 11px; color: var(--muted); }
  .preview, .logs { display: flex; flex-direction: column; gap: 4px; }
  .ph { display: flex; justify-content: space-between; align-items: center; }
  .ph-actions { display: flex; gap: 6px; align-items: center; }
  .sep { color: var(--muted); }
  .label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .link { background: none; border: 0; color: var(--accent); cursor: pointer; font: inherit; font-size: 11px; padding: 0; }
  .link:hover { text-decoration: underline; }
  .more {
    align-self: flex-start; background: none; border: 0;
    color: var(--accent); cursor: pointer; font-size: 11px; padding: 0;
  }
  .more:hover { text-decoration: underline; }
</style>
