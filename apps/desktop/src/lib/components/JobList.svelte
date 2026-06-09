<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { toast } from '../stores/toast.svelte';
  import JobRow from './JobRow.svelte';

  // Счётчики по статусу для summary-строки.
  const activeCount = $derived(
    jobsStore.jobs.filter(j =>
      j.stage === 'queued' || j.stage === 'fetching_metadata' ||
      j.stage === 'downloading' || j.stage === 'extracting_audio' ||
      j.stage === 'transcribing'
    ).length
  );
  const doneCount = $derived(
    jobsStore.jobs.filter(j => j.stage === 'done').length
  );
  const failCount = $derived(
    jobsStore.jobs.filter(j => j.stage === 'failed' || j.stage === 'cancelled').length
  );

  async function clearDone() {
    const done = doneCount + failCount;
    if (done === 0) {
      toast.info('Нет завершённых задач');
      return;
    }
    if (!confirm(`Удалить ${done} завершённых задач? Активные останутся.`)) return;
    try {
      await jobsStore.clearDone();
      toast.success(`Удалено ${done} задач`);
    } catch (e) {
      toast.error((e as Error).message);
    }
  }
</script>

<div class="list">
  <div class="head">
    <span class="label">Задачи</span>
    <span class="count">{jobsStore.jobs.length}</span>
    {#if jobsStore.jobs.length > 0}
      <span class="summary" aria-label="Сводка по задачам">
        {#if activeCount > 0}<span class="badge in-progress">{activeCount} в работе</span>{/if}
        {#if doneCount > 0}<span class="badge done">{doneCount} готово</span>{/if}
        {#if failCount > 0}<span class="badge err">{failCount} ошибок</span>{/if}
      </span>
      <button class="clear-btn" type="button" onclick={clearDone} title="Удалить завершённые и ошибочные задачи">очистить</button>
    {/if}
  </div>
  {#if jobsStore.jobs.length === 0}
    <div class="empty">
      <p class="empty-title">Задач пока нет</p>
      <p class="dim">Вставьте YouTube-ссылку или выберите папку с локальными аудио/видео файлами выше.</p>
    </div>
  {:else}
    <ul role="list" aria-label="Список задач">
      {#each jobsStore.jobs as job (job.id)}
        <li><JobRow {job} /></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 6px; min-height: 0; flex: 1; }
  .head { display: flex; align-items: center; gap: 6px; padding-bottom: 4px; flex-wrap: wrap; }
  .label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .count {
    font-size: 10px; padding: 0 5px; border-radius: 3px;
    background: var(--surface-3); color: var(--muted);
  }
  .summary { display: flex; gap: 4px; margin-left: 4px; flex-wrap: wrap; }
  .badge {
    font-size: 10px; padding: 1px 5px; border-radius: 3px;
    background: var(--surface-3); color: var(--muted);
  }
  .badge.in-progress { color: var(--accent); }
  .badge.done { color: var(--ok); }
  .badge.err { color: var(--err); }
  .clear-btn {
    margin-left: auto; background: transparent; border: 0;
    color: var(--muted); cursor: pointer; padding: 0 4px;
    font-size: 14px; line-height: 1; border-radius: 2px;
  }
  .clear-btn:hover { color: var(--fg); background: var(--surface-2); }
  ul {
    list-style: none; margin: 0; padding: 0;
    display: flex; flex-direction: column; gap: 2px;
    overflow-y: auto; flex: 1;
  }
  .empty {
    color: var(--muted); font-size: 12px; padding: 24px 20px;
    text-align: center; display: flex; flex-direction: column; gap: 6px;
  }
  .empty-title { font-size: 13px; color: var(--fg); }
  .dim { color: var(--muted); font-size: 11px; }
</style>
