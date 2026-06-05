<script lang="ts">
  import { settingsStore } from '../stores/settings.svelte';
  import { toast } from '../stores/toast.svelte';
  import type { AsrConfig, DownloadConfig, LoggingConfig, OutputConfig, ProxyConfig } from '../types';

  // Локальные биндинги для удобства
  const c = $derived(settingsStore.config);
  const proxy = $derived(c.proxy);
  const dl   = $derived(c.download);
  const asr  = $derived(c.asr);
  const out  = $derived(c.output);
  const log  = $derived(c.logging);

  function updateProxy(p: Partial<ProxyConfig>) { settingsStore.patch('proxy', { ...proxy, ...p }); }
  function updateDl(p: Partial<DownloadConfig>) { settingsStore.patch('download', { ...dl, ...p }); }
  function updateAsr(p: Partial<AsrConfig>) { settingsStore.patch('asr', { ...asr, ...p }); }
  function updateOut(p: Partial<OutputConfig>) { settingsStore.patch('output', { ...out, ...p }); }
  function updateLog(p: Partial<LoggingConfig>) { settingsStore.patch('logging', { ...log, ...p }); }

  function toggleFmt(f: 'txt' | 'srt' | 'json' | 'vtt') {
    const has = out.formats.includes(f);
    const next = has ? out.formats.filter((x) => x !== f) : [...out.formats, f];
    updateOut({ formats: next });
  }

  async function save() {
    try {
      await settingsStore.save();
      toast.success('Настройки сохранены');
    } catch (e) {
      toast.error('Ошибка сохранения: ' + (e as Error).message);
    }
  }
  async function reset() {
    await settingsStore.reset();
    toast.info('Сброшено к значениям по умолчанию');
  }
</script>

<div class="settings">
  <div class="head">
    <h2>Настройки</h2>
    <div class="actions">
      <button class="ghost" onclick={reset}>Сбросить</button>
      <button class="primary" disabled={!settingsStore.dirty} onclick={save}>
        {settingsStore.dirty ? 'Сохранить' : 'Сохранено'}
      </button>
    </div>
  </div>

  <section>
    <h3>Прокси</h3>
    <div class="grid">
      <label>Тип
        <select value={proxy.kind} onchange={(e) => updateProxy({ kind: (e.currentTarget as HTMLSelectElement).value as ProxyConfig['kind'] })}>
          <option value="none">Без прокси</option>
          <option value="http">HTTP</option>
          <option value="https">HTTPS</option>
          <option value="socks5">SOCKS5</option>
        </select>
      </label>
      <label>Хост
        <input type="text" value={proxy.host ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ host: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label>Порт
        <input type="number" min="1" max="65535" value={proxy.port ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ port: Number((e.currentTarget as HTMLInputElement).value) || undefined })} />
      </label>
      <label>Логин
        <input type="text" value={proxy.username ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ username: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label>Пароль
        <input type="password" value={proxy.password ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ password: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label class="full">Не использовать для (через запятую)
        <input type="text" value={proxy.noProxy ?? ''} oninput={(e) => updateProxy({ noProxy: (e.currentTarget as HTMLInputElement).value })} />
      </label>
    </div>
  </section>

  <section>
    <h3>Скачивание</h3>
    <div class="grid">
      <label class="full">Формат yt-dlp
        <input type="text" value={dl.format} oninput={(e) => updateDl({ format: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label>Макс. высота
        <input type="number" min="144" step="1" value={dl.maxHeight ?? ''} oninput={(e) => updateDl({ maxHeight: Number((e.currentTarget as HTMLInputElement).value) || null })} />
      </label>
      <label>Лимит размера
        <input type="text" value={dl.maxFilesize ?? ''} placeholder="например 500M" oninput={(e) => updateDl({ maxFilesize: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label class="full">Доп. аргументы (через пробел)
        <input type="text" value={dl.extraArgs.join(' ')} oninput={(e) => updateDl({ extraArgs: (e.currentTarget as HTMLInputElement).value.split(/\s+/).filter(Boolean) })} />
      </label>
      <label class="full">Cookies-файл (Netscape)
        <input type="text" value={dl.cookiesFile ?? ''} oninput={(e) => updateDl({ cookiesFile: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label class="check">
        <input type="checkbox" checked={dl.audioOnly} onchange={(e) => updateDl({ audioOnly: (e.currentTarget as HTMLInputElement).checked })} />
        Только аудио
      </label>
    </div>
  </section>

  <section>
    <h3>ASR (GigaAM-V3)</h3>
    <div class="grid">
      <label class="full">Папка с ONNX-моделями
        <input type="text" value={asr.modelDir} placeholder="/path/to/gigaam-v3-onnx" oninput={(e) => updateAsr({ modelDir: (e.currentTarget as HTMLInputElement).value })} />
      </label>
      <label>Вариант
        <select value={asr.modelVariant} onchange={(e) => updateAsr({ modelVariant: (e.currentTarget as HTMLSelectElement).value as AsrConfig['modelVariant'] })}>
          <option value="v3_rnnt">V3 RNN-T (рекомендуется)</option>
          <option value="v3_ctc">V3 CTC</option>
          <option value="v3_e2e">V3 E2E</option>
        </select>
      </label>
      <label>Устройство
        <select value={asr.device} onchange={(e) => updateAsr({ device: (e.currentTarget as HTMLSelectElement).value as AsrConfig['device'] })}>
          <option value="openvino_cpu">OpenVINO · CPU</option>
          <option value="cpu">ONNX · CPU</option>
          <option value="openvino_gpu">OpenVINO · GPU</option>
          <option value="openvino_npu">OpenVINO · NPU (Intel)</option>
        </select>
      </label>
      <label>Потоки
        <input type="number" min="1" max="32" value={asr.threads} oninput={(e) => updateAsr({ threads: Math.max(1, Number((e.currentTarget as HTMLInputElement).value) || 1) })} />
      </label>
      <label>Длина чанка (с)
        <input type="number" min="5" step="1" value={asr.chunkLengthSec} oninput={(e) => updateAsr({ chunkLengthSec: Number((e.currentTarget as HTMLInputElement).value) || 20 })} />
      </label>
      <label>Оверлап (с)
        <input type="number" min="0" step="0.5" value={asr.chunkOverlapSec} oninput={(e) => updateAsr({ chunkOverlapSec: Number((e.currentTarget as HTMLInputElement).value) || 0 })} />
      </label>
      <label class="check">
        <input type="checkbox" checked={asr.useVad} onchange={(e) => updateAsr({ useVad: (e.currentTarget as HTMLInputElement).checked })} />
        VAD (Silero) — пропускать тишину
      </label>
    </div>
  </section>

  <section>
    <h3>Выход</h3>
    <div class="grid">
      <div class="full chips">
        {#each ['txt','srt','json','vtt'] as f}
          <button type="button" class="chip" class:on={out.formats.includes(f as any)} onclick={() => toggleFmt(f as any)}>{f}</button>
        {/each}
      </div>
      <label>Макс. длина строки
        <input type="number" min="40" max="200" value={out.maxLineLength} oninput={(e) => updateOut({ maxLineLength: Number((e.currentTarget as HTMLInputElement).value) || 90 })} />
      </label>
      <label class="full">Папка результатов (пусто = по умолчанию)
        <input type="text" value={out.outputDir ?? ''} oninput={(e) => updateOut({ outputDir: (e.currentTarget as HTMLInputElement).value || undefined })} />
      </label>
    </div>
  </section>

  <section>
    <h3>Логи</h3>
    <div class="grid">
      <label>Уровень
        <select value={log.level} onchange={(e) => updateLog({ level: (e.currentTarget as HTMLSelectElement).value as LoggingConfig['level'] })}>
          {#each ['error','warn','info','debug','trace'] as l}
            <option value={l}>{l}</option>
          {/each}
        </select>
      </label>
      <label>Макс. размер (MB)
        <input type="number" min="1" max="500" value={log.maxFileSizeMb} oninput={(e) => updateLog({ maxFileSizeMb: Number((e.currentTarget as HTMLInputElement).value) || 10 })} />
      </label>
      <label class="check">
        <input type="checkbox" checked={log.file} onchange={(e) => updateLog({ file: (e.currentTarget as HTMLInputElement).checked })} />
        Писать в файл
      </label>
    </div>
  </section>
</div>

<style>
  .settings { display: flex; flex-direction: column; gap: 16px; min-height: 0; overflow-y: auto; }
  .head { display: flex; align-items: center; justify-content: space-between; }
  h2 { font-size: 13px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted, #8a93a3); margin: 0; }
  h3 { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted, #8a93a3); margin: 0 0 8px; }
  section {
    background: var(--surface-1, #14171c);
    border: 1px solid var(--border, #2a2e35);
    border-radius: 10px;
    padding: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px 12px;
  }
  label {
    display: flex; flex-direction: column; gap: 4px;
    font-size: 12px; color: var(--muted, #8a93a3);
  }
  label.full { grid-column: 1 / -1; }
  label.check { flex-direction: row; align-items: center; gap: 6px; }
  input, select {
    background: var(--surface-2, #1c1f24);
    color: inherit;
    border: 1px solid var(--border, #2a2e35);
    border-radius: 6px;
    padding: 6px 8px;
    font: inherit;
  }
  input:disabled, select:disabled { opacity: 0.5; }
  .actions { display: flex; gap: 6px; }
  button.primary {
    background: var(--accent, #5b8def); color: white; border: 0; border-radius: 6px; padding: 6px 12px; cursor: pointer; font: inherit;
  }
  button.primary:disabled { opacity: 0.5; cursor: not-allowed; }
  button.ghost {
    background: transparent; color: var(--muted, #8a93a3); border: 1px solid var(--border, #2a2e35); border-radius: 6px; padding: 6px 12px; cursor: pointer; font: inherit;
  }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chip {
    background: var(--surface-2, #1c1f24);
    border: 1px solid var(--border, #2a2e35);
    color: var(--muted, #8a93a3);
    border-radius: 999px;
    padding: 4px 10px;
    cursor: pointer;
    font: inherit;
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.06em;
  }
  .chip.on {
    background: var(--accent, #5b8def);
    color: white;
    border-color: var(--accent, #5b8def);
  }
</style>
