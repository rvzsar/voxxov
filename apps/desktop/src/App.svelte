<script lang="ts">
  import { onMount } from 'svelte';
  import { jobsStore } from './lib/stores/jobs.svelte';
  import { settingsStore } from './lib/stores/settings.svelte';
  import { api } from './lib/api';
  import UrlInput from './lib/components/UrlInput.svelte';
  import JobList from './lib/components/JobList.svelte';
  import LogView from './lib/components/LogView.svelte';
  import LocalFiles from './lib/components/LocalFiles.svelte';
  import Settings from './lib/components/Settings.svelte';
  import Toaster from './lib/components/Toaster.svelte';

  let tab = $state<'tasks' | 'local' | 'settings'>('tasks');
  let backendOk = $state<boolean | null>(null);

  onMount(async () => {
    await Promise.all([settingsStore.load(), jobsStore.init()]);
    const d = await api.diagnose();
    backendOk = !!d.ffmpeg && !!d.ytdlp;
  });
</script>

<div class="app">
  <nav class="activity-bar" aria-label="Основная навигация">
    <div class="ab-section ab-top">
      <span class="logo" aria-label="GigaAM">🎙</span>
    </div>
    <div class="ab-section ab-nav">
      <button class="ab-btn" class:active={tab === 'tasks'} aria-label="Задачи" aria-current={tab === 'tasks' ? 'page' : undefined} onclick={() => (tab = 'tasks')} title="Задачи">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M3 4.5h12M3 9h12M3 13.5h7"/></svg>
      </button>
      <button class="ab-btn" class:active={tab === 'local'} aria-label="Локальные файлы" aria-current={tab === 'local' ? 'page' : undefined} onclick={() => (tab = 'local')} title="Локальные файлы">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M2 5c0-.6.4-1 1-1h4l2 2h6c.6 0 1 .4 1 1v7c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1V5z"/></svg>
      </button>
    </div>
    <div class="ab-section ab-bottom">
      <button class="ab-btn" class:active={tab === 'settings'} aria-label="Настройки" aria-current={tab === 'settings' ? 'page' : undefined} onclick={() => (tab = 'settings')} title="Настройки">
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><circle cx="9" cy="9" r="3"/><path d="M9 2v2M9 14v2M2 9h2M14 9h2M4.2 4.2l1.4 1.4M12.4 12.4l1.4 1.4M4.2 13.8l1.4-1.4M12.4 5.6l1.4-1.4"/></svg>
      </button>
    </div>
  </nav>

  <div class="workspace">
    {#if tab === 'tasks'}
      <div class="split">
        <aside class="panel-left">
          <UrlInput />
          <JobList />
        </aside>
        <section class="panel-right">
          <LogView />
        </section>
      </div>
    {:else if tab === 'local'}
      <section class="panel-full">
        <LocalFiles />
      </section>
    {:else}
      <section class="panel-full">
        <Settings />
      </section>
    {/if}
  </div>

  <footer class="statusbar" role="status">
    <button
      class="sb-item sb-btn"
      type="button"
      onclick={() => { if (backendOk === false) tab = 'settings'; }}
      title={backendOk === false ? 'Нажмите, чтобы открыть настройки' : ''}
    >
      <span class="dot" class:on={backendOk === true} class:warn={backendOk === false}></span>
      {#if backendOk === true}ready{:else if backendOk === false}offline — yt-dlp/ffmpeg не найдены{:else}…{/if}
    </button>
    <span class="sb-sep"></span>
    <span class="sb-item">{jobsStore.jobs.length} jobs</span>
    <span class="sb-right"></span>
    <span class="sb-item dim">v0.1</span>
  </footer>

  <Toaster />
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 42px 1fr;
    grid-template-rows: 1fr 22px;
    height: 100vh;
    overflow: hidden;
  }

  /* Activity bar */
  .activity-bar {
    grid-row: 1 / -1;
    background: var(--bg);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 0;
    user-select: none;
  }
  .ab-section { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .ab-top { padding: 4px 0 8px; }
  .ab-nav { flex: 1; }
  .ab-bottom { margin-top: auto; }
  .logo { font-size: 18px; line-height: 1; }
  .ab-btn {
    width: 32px; height: 32px;
    display: flex; align-items: center; justify-content: center;
    border: 0; background: transparent; color: var(--muted);
    cursor: pointer; border-radius: 4px; position: relative;
  }
  .ab-btn:hover { background: var(--surface-2); color: var(--fg); }
  .ab-btn.active { color: var(--fg); }
  .ab-btn.active::before {
    content: '';
    position: absolute; left: -5px; top: 6px; bottom: 6px;
    width: 2px; background: var(--accent); border-radius: 1px;
  }

  /* Workspace */
  .workspace { min-height: 0; overflow: hidden; }
  .split {
    display: grid;
    grid-template-columns: 340px 1fr;
    height: 100%;
  }
  .panel-left {
    border-right: 1px solid var(--border);
    display: flex; flex-direction: column;
    padding: 10px; gap: 10px;
    overflow: hidden; min-height: 0;
  }
  .panel-right {
    display: flex; flex-direction: column;
    overflow: hidden; min-height: 0;
  }
  .panel-full {
    display: flex; flex-direction: column;
    padding: 14px;
    overflow: auto; height: 100%;
  }

  /* Status bar */
  .statusbar {
    background: var(--surface-1);
    border-top: 1px solid var(--border);
    display: flex; align-items: center;
    padding: 0 10px;
    font-size: 11px; color: var(--muted);
    gap: 8px;
  }
  .sb-item { display: flex; align-items: center; gap: 5px; white-space: nowrap; }
  .sb-btn {
    background: transparent; border: 0; padding: 2px 6px; margin: -2px 0;
    border-radius: 3px; color: inherit; font: inherit; cursor: pointer;
  }
  .sb-btn:hover { background: var(--surface-2); }
  .sb-btn:focus-visible { outline: 1px solid var(--accent); }
  .sb-sep { width: 1px; height: 10px; background: var(--border); }
  .sb-right { flex: 1; }
  .dim { opacity: 0.6; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--muted); }
  .dot.on { background: var(--ok); }
  .dot.warn { background: var(--warn); }

  @media (max-width: 800px) {
    .split { grid-template-columns: 1fr; }
    .panel-left { border-right: 0; border-bottom: 1px solid var(--border); }
  }
</style>
