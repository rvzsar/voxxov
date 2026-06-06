<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { toast } from '../stores/toast.svelte';
  import JobRow from './JobRow.svelte';

  function clearDone() {
    // UI-side: пройтись по jobs и пометить удалёнными нет возможности
    // (удаление jobs — отдельный бэкенд-команд, см. TODO). Поэтому
    // здесь только сворачиваем: выделить только не-завершённые.
    // Сейчас просто показываем счётчик и тост.
    const done = jobsStore.jobs.filter(j => j.stage === 'done' || j.stage === 'failed' || j.stage === 'cancelled').length;
    if (done === 0) {
      toast.info('Нет завершённых задач');
      return;
    }
    toast.info(`Завершённых задач: ${done}. Очистка будет в Sprint 4 (SQLite).`);
  }
</script>

<div class="list">
  <div class="head">
    <span class="label">Задачи</span>
    <span class="count">{jobsStore.jobs.length}</span>
    {#if jobsStore.jobs.length > 0}
      <button class="clear-btn" type="button" onclick={clearDone} title="Показать информацию о завершённых">…</button>
    {/if}
  </div>
  {#if jobsStore.jobs.length === 0}
    <div class="empty">Нет задач — вставьте ссылку выше</div>
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
  .head { display: flex; align-items: center; gap: 6px; padding-bottom: 4px; }
  .label { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .count {
    font-size: 10px; padding: 0 5px; border-radius: 3px;
    background: var(--surface-3); color: var(--muted);
  }
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
  .empty { color: var(--muted); font-size: 12px; padding: 20px; text-align: center; }
</style>
