<script lang="ts">
  import { onMount } from 'svelte';
  import { jobsStore } from './lib/stores/jobs.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { api } from './lib/api';
  import UrlInput from './lib/components/UrlInput.svelte';
  import JobList from './lib/components/JobList.svelte';
  import LogView from './lib/components/LogView.svelte';
  import Settings from './lib/components/Settings.svelte';
  import Toaster from './lib/components/Toaster.svelte';

  let tab = $state<'tasks' | 'settings'>('tasks');
  let backendOk = $state<boolean | null>(null);

  onMount(async () => {
    await Promise.all([settingsStore.load(), jobsStore.init()]);
    if (api.isTauri) {
      const d = await api.diagnose();
      backendOk = !!d.ffmpeg && !!d.ytdlp;
    } else {
      backendOk = false; // dev-режим в браузере
    }
  });
</script>

<div class="app">
  <header class="topbar">
    <div class="brand">
      <span class="logo">🎙️</span>
      <span class="name">GigaAM Desktop</span>
      <span class="ver">v0.1 · Sprint 1</span>
    </div>
    <nav class="tabs">
      <button class:active={tab === 'tasks'} onclick={() => (tab = 'tasks')}>Задачи</button>
      <button class:active={tab === 'settings'} onclick={() => (tab = 'settings')}>Настройки</button>
    </nav>
    <div class="status" title={api.isTauri ? 'Tauri backend' : 'Браузер (mock-режим)'}>
      <span class="dot" class:on={backendOk === true} class:warn={backendOk === false}></span>
      <span class="lbl">
        {#if !api.isTauri}dev-mock{:else if backendOk === true}готов{:else if backendOk === false}бэкенд недоступен{:else}проверяю…{/if}
      </span>
    </div>
  </header>

  <main>
    {#if tab === 'tasks'}
      <aside class="left">
        <UrlInput />
        <JobList />
      </aside>
      <section class="right">
        <LogView />
      </section>
    {:else}
      <section class="full">
        <Settings />
      </section>
    {/if}
  </main>

  <Toaster />
</div>

<style>
  :global(html, body, #app) { height: 100%; margin: 0; }
  :global(body) {
    background: var(--bg, #0e1014);
    color: var(--fg, #e6e8ee);
    font: 14px/1.45 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  }

  .app {
    display: grid;
    grid-template-rows: 48px 1fr;
    height: 100vh;
  }

  .topbar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 14px;
    background: var(--surface-1, #14171c);
    border-bottom: 1px solid var(--border, #2a2e35);
    user-select: none;
  }
  .brand { display: flex; align-items: center; gap: 8px; }
  .logo { font-size: 18px; }
  .name { font-weight: 700; }
  .ver { color: var(--muted, #8a93a3); font-size: 12px; }
  .tabs { display: flex; gap: 4px; }
  .tabs button {
    background: transparent; color: var(--muted, #8a93a3); border: 0;
    padding: 6px 12px; border-radius: 6px; cursor: pointer; font: inherit;
  }
  .tabs button:hover { background: var(--surface-2, #1c1f24); color: inherit; }
  .tabs button.active { background: var(--surface-2, #1c1f24); color: inherit; }
  .status { display: flex; align-items: center; gap: 6px; justify-self: end; font-size: 12px; color: var(--muted, #8a93a3); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--muted, #8a93a3); }
  .dot.on { background: var(--ok, #38c172); }
  .dot.warn { background: var(--warn, #f5a524); }

  main {
    display: grid;
    grid-template-columns: 360px 1fr;
    min-height: 0;
  }
  main:has(.full) { grid-template-columns: 1fr; }
  .left, .right, .full {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    overflow: hidden;
  }
  .left { border-right: 1px solid var(--border, #2a2e35); }
  .right { background: var(--bg, #0e1014); }
  .full { overflow: auto; }

  @media (max-width: 800px) {
    main { grid-template-columns: 1fr; }
    .left { border-right: 0; border-bottom: 1px solid var(--border, #2a2e35); }
  }
</style>
