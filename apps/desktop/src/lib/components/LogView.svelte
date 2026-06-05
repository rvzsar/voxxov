<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { api } from '../api';
  import { stageLabel } from '../format';

  const job = $derived(jobsStore.active());

  async function openInFolder() {
    if (job?.transcriptPath) {
      try { await api.revealInFolder(job.transcriptPath); } catch {}
    }
  }
</script>

{#if !job}
  <div class="empty">
    <div class="big">🎧</div>
    <p>Выберите задачу слева, чтобы увидеть детали.</p>
    <p class="dim">Или добавьте новую, вставив ссылку на видео.</p>
  </div>
{:else}
  <div class="detail">
    <div class="head">
      <div>
        <div class="title">{job.media?.title ?? job.url}</div>
        <a class="url" href={job.url} target="_blank" rel="noreferrer">{job.url}</a>
      </div>
      <span class="stage">{stageLabel(job.stage)}</span>
    </div>

    <div class="meta">
      <div><span class="dim">Создано:</span> {new Date(job.createdAt).toLocaleString()}</div>
      {#if job.finishedAt}
        <div><span class="dim">Завершено:</span> {new Date(job.finishedAt).toLocaleString()}</div>
      {/if}
      {#if job.media?.uploader}
        <div><span class="dim">Канал:</span> {job.media.uploader}</div>
      {/if}
    </div>

    <div class="progress">
      <div class="lbl">{job.progress.label}</div>
      <div class="bar"><div class="fill" style="width: {(job.progress.pct * 100).toFixed(1)}%"></div></div>
    </div>

    {#if job.error}
      <pre class="err">Ошибка: {job.error}</pre>
    {/if}

    {#if job.transcriptPreview}
      <div class="preview">
        <div class="ph">
          <h3>Превью</h3>
          {#if job.transcriptPath}
            <button class="link" onclick={openInFolder}>открыть файл</button>
          {/if}
        </div>
        <pre>{job.transcriptPreview}</pre>
      </div>
    {/if}
  </div>
{/if}

<style>
  .empty {
    height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center;
    color: var(--muted, #8a93a3); text-align: center; padding: 40px;
  }
  .big { font-size: 48px; margin-bottom: 8px; }
  .dim { color: var(--muted, #8a93a3); font-size: 12px; }
  .detail { display: flex; flex-direction: column; gap: 16px; padding: 4px; }
  .head { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .title { font-size: 16px; font-weight: 700; }
  .url { color: var(--muted, #8a93a3); font-size: 12px; text-decoration: none; }
  .url:hover { color: var(--accent, #5b8def); }
  .stage {
    font-size: 11px; padding: 4px 10px; border-radius: 999px; text-transform: uppercase;
    background: var(--surface-2, #1c1f24); border: 1px solid var(--border, #2a2e35); color: var(--muted, #8a93a3);
  }
  .meta { display: flex; flex-direction: column; gap: 2px; font-size: 12px; }
  .progress { display: flex; flex-direction: column; gap: 4px; }
  .lbl { font-size: 12px; color: var(--muted, #8a93a3); }
  .bar { height: 8px; background: var(--surface-2, #1c1f24); border: 1px solid var(--border, #2a2e35); border-radius: 999px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent, #5b8def); transition: width 120ms; }
  pre { background: var(--surface-2, #1c1f24); border: 1px solid var(--border, #2a2e35); border-radius: 8px; padding: 10px 12px; margin: 0; white-space: pre-wrap; word-wrap: break-word; font: inherit; font-size: 13px; line-height: 1.5; max-height: 360px; overflow: auto; }
  pre.err { color: var(--err, #e5484d); border-color: color-mix(in oklab, var(--err, #e5484d) 40%, transparent); }
  .preview { display: flex; flex-direction: column; gap: 6px; }
  .ph { display: flex; justify-content: space-between; align-items: center; }
  h3 { margin: 0; font-size: 12px; text-transform: uppercase; color: var(--muted, #8a93a3); letter-spacing: 0.06em; }
  .link { background: none; border: 0; color: var(--accent, #5b8def); cursor: pointer; font: inherit; }
</style>
