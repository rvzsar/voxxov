<script lang="ts">
  import { jobsStore } from '../stores/jobs.svelte';
  import { toast } from '../stores/toast.svelte';
  import { isProbablyUrl } from '../format';

  let value = $state('');
  let busy = $state(false);

  async function submit() {
    const url = value.trim();
    if (!url) return;
    if (!isProbablyUrl(url)) {
      toast.error('Не URL: ' + url.slice(0, 40));
      return;
    }
    busy = true;
    try {
      await jobsStore.add(url);
      value = '';
      toast.success('Добавлено в очередь');
    } catch (e) {
      toast.error((e as Error).message);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); submit(); }
  }
</script>

<form class="url-bar" onsubmit={submit}>
  <input
    type="url"
    placeholder="https://…  (Enter чтобы добавить)"
    bind:value
    onkeydown={onKey}
    disabled={busy}
    spellcheck="false"
    autocomplete="off"
    aria-label="URL видео"
  />
  <button type="submit" disabled={busy || !value.trim()} aria-label="Добавить задачу">
    {busy ? '…' : '→'}
  </button>
</form>

<style>
  .url-bar {
    display: flex; gap: 0; flex-shrink: 0;
  }
  input {
    flex: 1;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-right: 0;
    border-radius: 4px 0 0 4px;
    padding: 6px 10px;
    color: var(--fg);
    font-size: 13px;
    min-width: 0;
  }
  input::placeholder { color: var(--muted); }
  input:focus { border-color: var(--accent); }
  button {
    background: var(--surface-3);
    border: 1px solid var(--border);
    border-radius: 0 4px 4px 0;
    color: var(--fg);
    padding: 0 12px;
    cursor: pointer;
    font-size: 14px;
  }
  button:hover { background: var(--accent); border-color: var(--accent); }
  button:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
