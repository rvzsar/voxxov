<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { stageLabel } from '../format';

  const job = $derived(jobsStore.active());
  const logs = $derived(jobsStore.logsFor(jobsStore.activeId));

  async function openInFolder() {
    if (job?.transcriptPath) {
      try { await api.revealInFolder(job.transcriptPath); } catch (e) {
        console.error('revealInFolder failed:', e);
      }
    }
  }
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
      <span class="badge">{stageLabel(job.stage)}</span>
    </div>

    <div class="meta">
      <span class="dim">Создано:</span> {new Date(job.createdAt).toLocaleString()}
      {#if job.finishedAt}
        <span class="dim"> · Завершено:</span> {new Date(job.finishedAt).toLocaleString()}
      {/if}
      {#if job.media?.uploader}
        <span class="dim"> · Канал:</span> {job.media.uploader}
      {/if}
    </div>

    <div class="progress">
      <div class="pct">{Math.round((job.progress.pct || 0) * 100)}%</div>
      <div class="bar"><div class="fill" style="width: {((job.progress.pct || 0) * 100).toFixed(1)}%"></div></div>
      <div class="lbl">{job.progress.label}</div>
    </div>

    {#if job.error}
      <pre class="err">{job.error}</pre>
    {/if}

    {#if job.transcriptPreview?.trim()}
      <div class="preview">
        <div class="ph">
          <span class="label">Превью</span>
          {#if job.transcriptPath}
            <button class="link" onclick={openInFolder}>открыть файл</button>
          {/if}
        </div>
        <pre>{job.transcriptPreview}</pre>
      </div>
    {/if}

    {#if logs.length > 0}
      <div class="logs">
        <div class="ph">
          <span class="label">Лог ({logs.length})</span>
        </div>
        <pre class="log-pre">{logs.join('\n')}</pre>
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
  }
  .meta { font-size: 12px; color: var(--fg); }
  .dim { color: var(--muted); }
  .progress { display: flex; align-items: center; gap: 8px; }
  .pct { font-size: 12px; font-family: var(--mono); min-width: 32px; color: var(--muted); }
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
  .label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .link { background: none; border: 0; color: var(--accent); cursor: pointer; font: inherit; font-size: 11px; }
</style>
