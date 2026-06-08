<script lang="ts">
  import { toast } from '../stores/toast.svelte';
</script>

<div class="toaster" aria-live="polite">
  {#each toast.items as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>

<style>
  .toaster {
    position: fixed; right: 12px; bottom: 34px;
    display: flex; flex-direction: column; gap: 6px;
    z-index: 1000; pointer-events: none;
  }
  .toast {
    background: var(--surface-2); border: 1px solid var(--border);
    border-radius: 4px; padding: 6px 10px;
    font-size: 12px; min-width: 200px; max-width: 340px;
    animation: slide 120ms ease;
  }
  .toast.info { border-left: 2px solid var(--accent); }
  .toast.success { border-left: 2px solid var(--ok); color: var(--ok); }
  .toast.error { border-left: 2px solid var(--err); color: var(--err); }
  @keyframes slide { from { transform: translateY(6px); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
</style>
