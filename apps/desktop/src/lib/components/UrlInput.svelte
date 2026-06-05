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
      toast.error('Похоже, это не http(s) ссылка');
      return;
    }
    busy = true;
    try {
      await jobsStore.add(url);
      value = '';
      toast.success('Поставлено в очередь');
    } catch (e) {
      toast.error('Не удалось поставить в очередь: ' + (e as Error).message);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  function pasteExample() {
    value = 'https://www.youtube.com/watch?v=dQw4w9WgXcQ';
  }
</script>

<form class="url-input" onsubmit={submit}>
  <div class="field">
    <span class="prefix">URL</span>
    <input
      type="url"
      placeholder="https://… ссылка на видео или плейлист"
      bind:value
      onkeydown={onKey}
      disabled={busy}
      spellcheck="false"
      autocomplete="off"
    />
    <button type="submit" disabled={busy || !value.trim()}>
      {busy ? 'Отправляю…' : 'Скачать'}
    </button>
  </div>
  <div class="hints">
    <button type="button" class="link" onclick={pasteExample}>вставить пример</button>
    <span class="dim">Enter — отправить, Shift+Enter — перенос</span>
  </div>
</form>

<style>
  .url-input {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .field {
    display: flex;
    align-items: stretch;
    background: var(--surface-2, #1c1f24);
    border: 1px solid var(--border, #2a2e35);
    border-radius: 10px;
    overflow: hidden;
  }
  .field:focus-within {
    border-color: var(--accent, #5b8def);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent, #5b8def) 25%, transparent);
  }
  .prefix {
    display: flex;
    align-items: center;
    padding: 0 10px;
    font-size: 12px;
    color: var(--muted, #8a93a3);
    background: var(--surface-1, #14171c);
    border-right: 1px solid var(--border, #2a2e35);
    user-select: none;
  }
  input {
    flex: 1;
    background: transparent;
    border: 0;
    color: inherit;
    padding: 10px 12px;
    font: inherit;
    outline: none;
    min-width: 0;
  }
  button[type='submit'] {
    background: var(--accent, #5b8def);
    color: white;
    border: 0;
    padding: 0 16px;
    font-weight: 600;
    cursor: pointer;
  }
  button[type='submit']:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .hints {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--muted, #8a93a3);
  }
  .link {
    background: none;
    border: 0;
    color: var(--accent, #5b8def);
    cursor: pointer;
    padding: 0;
    font: inherit;
  }
  .dim { color: var(--muted, #8a93a3); }
</style>
