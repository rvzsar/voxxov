<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import JobRow from './JobRow.svelte';
</script>

<div class="list">
  <div class="head">
    <h2>Задачи</h2>
    <span class="count">{jobsStore.jobs.length}</span>
  </div>
  {#if jobsStore.jobs.length === 0}
    <div class="empty">
      <p>Пока пусто.</p>
      <p class="dim">Вставь ссылку выше и нажми <kbd>Enter</kbd>.</p>
    </div>
  {:else}
    <ul>
      {#each jobsStore.jobs as job (job.id)}
        <li><JobRow {job} /></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 10px; min-height: 0; flex: 1; }
  .head { display: flex; align-items: baseline; gap: 8px; }
  h2 { font-size: 13px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted, #8a93a3); margin: 0; }
  .count {
    font-size: 11px; padding: 1px 6px; border-radius: 999px;
    background: var(--surface-2, #1c1f24); border: 1px solid var(--border, #2a2e35);
    color: var(--muted, #8a93a3);
  }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; overflow-y: auto; }
  .empty { color: var(--muted, #8a93a3); padding: 16px; text-align: center; border: 1px dashed var(--border, #2a2e35); border-radius: 10px; }
  .dim { color: var(--muted, #8a93a3); font-size: 12px; }
  kbd { background: var(--surface-2, #1c1f24); border: 1px solid var(--border, #2a2e35); border-radius: 4px; padding: 1px 4px; font-size: 11px; }
</style>
