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
    position: fixed; right: 16px; bottom: 16px;
    display: flex; flex-direction: column; gap: 8px;
    z-index: 1000; pointer-events: none;
  }
  .toast {
    background: var(--surface-2, #1c1f24);
    border: 1px solid var(--border, #2a2e35);
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 13px;
    box-shadow: 0 8px 32px rgba(0,0,0,0.4);
    min-width: 220px; max-width: 360px;
    animation: slide 160ms ease;
  }
  .toast.info    { border-color: var(--accent, #5b8def); }
  .toast.success { border-color: var(--ok, #38c172); color: var(--ok, #38c172); }
  .toast.error   { border-color: var(--err, #e5484d); color: var(--err, #e5484d); }
  @keyframes slide {
    from { transform: translateY(8px); opacity: 0; }
    to   { transform: translateY(0); opacity: 1; }
  }
</style>
