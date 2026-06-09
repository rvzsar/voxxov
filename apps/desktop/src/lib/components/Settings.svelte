<script lang="ts">
  import { settingsStore } from '../stores/settings.svelte';
  import { toast } from '../stores/toast.svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import type { AsrDevice, ProxyKind } from '../types';

  const c = $derived(settingsStore.config);
  const proxy = $derived(c.proxy);
  const dl   = $derived(c.download);
  const asr  = $derived(c.asr);
  const out  = $derived(c.output);
  const log  = $derived(c.logging);

  function updateProxy(p: Record<string, unknown>) { settingsStore.patch('proxy', { ...proxy, ...p }); }
  function updateDl(p: Record<string, unknown>) { settingsStore.patch('download', { ...dl, ...p }); }
  function updateAsr(p: Record<string, unknown>) { settingsStore.patch('asr', { ...asr, ...p }); }
  function updateOut(p: Record<string, unknown>) { settingsStore.patch('output', { ...out, ...p }); }
  function updateLog(p: Record<string, unknown>) { settingsStore.patch('logging', { ...log, ...p }); }

  function toggleFmt(f: string) {
    const has = out.formats.includes(f);
    const next = has ? out.formats.filter((x) => x !== f) : [...out.formats, f];
    updateOut({ formats: next });
  }

  function validatePort(v: number): number | undefined {
    if (!Number.isFinite(v) || v < 1 || v > 65535) return undefined;
    return Math.floor(v);
  }

  // Cross-field + custom-range валидация. HTML5 min/max ловят простые
  // диапазоны на input'е; здесь — то, что input'у недоступно (0 = "off"
  // для maxHeight, cross-field для proxy.host, и т.п.).
  const errors = $derived.by(() => {
    const e: Record<string, string> = {};
    if (proxy.kind !== 'none' && !proxy.host?.trim()) {
      e['proxy.host'] = 'Укажите хост';
    }
    if (!dl.format.trim()) {
      e['dl.format'] = 'Не может быть пустым';
    }
    if (dl.maxHeight !== 0 && (dl.maxHeight < 144 || dl.maxHeight > 4320)) {
      e['dl.maxHeight'] = '0 (откл) или 144..4320';
    }
    if (asr.sampleRate < 8000 || asr.sampleRate > 48000) {
      e['asr.sampleRate'] = '8000..48000 Гц';
    }
    if (out.formats.length === 0) {
      e['out.formats'] = 'Выберите хотя бы один';
    }
    return e;
  });
  const hasErrors = $derived(Object.keys(errors).length > 0);
  const canSave = $derived(settingsStore.dirty && !hasErrors);

  async function save() {
    if (hasErrors) {
      toast.error('Исправьте ошибки в полях');
      return;
    }
    try {
      await settingsStore.save();
      toast.success('Сохранено');
    } catch (e) {
      toast.error((e as Error).message);
    }
  }

  async function reset() {
    const ok = confirm('Сбросить все настройки к значениям по умолчанию?');
    if (!ok) return;
    settingsStore.reset();
    toast.info('Сброшено (не забудьте Сохранить)');
  }

  async function pickDir(setter: (path: string) => void, title: string) {
    try {
      const picked = await openDialog({ directory: true, multiple: false, title });
      if (typeof picked === 'string' && picked.length > 0) setter(picked);
    } catch (e) {
      toast.error(`Не удалось открыть диалог: ${(e as Error).message}`);
    }
  }
  const pickModelDir = () => pickDir((p) => updateAsr({ modelDir: p }), 'Выберите папку с моделью GigaAM');
  const pickOutDir   = () => pickDir((p) => updateOut({ dir: p }),       'Выберите папку для результатов');

  // Ctrl+S / Cmd+S → сохранить (стандартный паттерн desktop-приложений).
  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      save();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="settings">
  <div class="head">
    <span class="title">Настройки</span>
    <div class="actions">
      <button class="btn" type="button" onclick={reset}>Сброс</button>
      <button
        class="btn primary"
        type="button"
        disabled={!canSave}
        title={hasErrors ? 'Исправьте ошибки' : settingsStore.dirty ? 'Ctrl+S' : ''}
        onclick={save}
      >
        {hasErrors ? '⚠ ошибки' : settingsStore.dirty ? 'Сохранить' : '✓ сохранено'}
      </button>
    </div>
  </div>

  <section>
    <h3>Прокси</h3>
    <div class="grid">
      <label>Тип
        <select value={proxy.kind} onchange={(e) => updateProxy({ kind: (e.currentTarget as HTMLSelectElement).value as ProxyKind })}>
          <option value="none">Без прокси</option>
          <option value="http">HTTP</option>
          <option value="https">HTTPS</option>
          <option value="socks5">SOCKS5</option>
        </select>
      </label>
      <label>Хост
        <input
          type="text"
          value={proxy.host ?? ''}
          disabled={proxy.kind === 'none'}
          aria-invalid={!!errors['proxy.host']}
          oninput={(e) => updateProxy({ host: (e.currentTarget as HTMLInputElement).value })}
        />
        {#if errors['proxy.host']}<span class="err-msg">{errors['proxy.host']}</span>{/if}
      </label>
      <label>Порт
        <input type="number" min="1" max="65535" title="1..65535" value={proxy.port ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ port: validatePort(Number((e.currentTarget as HTMLInputElement).value)) })} />
      </label>
      <label>Логин<input type="text" value={proxy.username ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ username: (e.currentTarget as HTMLInputElement).value })} /></label>
      <label>Пароль<input type="password" value={proxy.password ?? ''} disabled={proxy.kind === 'none'} oninput={(e) => updateProxy({ password: (e.currentTarget as HTMLInputElement).value })} /></label>
      <label class="full">No-proxy (через запятую)<input type="text" value={proxy.noProxy ?? ''} placeholder="localhost,127.0.0.1,.internal" oninput={(e) => updateProxy({ noProxy: (e.currentTarget as HTMLInputElement).value })} /></label>
    </div>
  </section>

  <section>
    <h3>Скачивание (yt-dlp)</h3>
    <div class="grid">
      <label class="full">Формат
        <input
          type="text"
          value={dl.format}
          placeholder="bv*+ba/b"
          title="Строка формата yt-dlp (например: bv*+ba/b, bestaudio, 137+140)"
          aria-invalid={!!errors['dl.format']}
          oninput={(e) => updateDl({ format: (e.currentTarget as HTMLInputElement).value })}
        />
        {#if errors['dl.format']}<span class="err-msg">{errors['dl.format']}</span>{/if}
      </label>
      <label>Макс. высота
        <input
          type="number" min="0" max="4320" step="1"
          value={dl.maxHeight}
          title="0 = без ограничения; иначе 144..4320 пикселей"
          aria-invalid={!!errors['dl.maxHeight']}
          oninput={(e) => updateDl({ maxHeight: Number((e.currentTarget as HTMLInputElement).value) || 0 })}
        />
        {#if errors['dl.maxHeight']}<span class="err-msg">{errors['dl.maxHeight']}</span>{/if}
      </label>
      <label>Параллельно<input type="number" min="1" max="16" title="Число параллельных соединений при загрузке" value={dl.concurrentFragments} oninput={(e) => updateDl({ concurrentFragments: Math.max(1, Math.min(16, Number((e.currentTarget as HTMLInputElement).value) || 1)) })} /></label>
      <label>Повторы<input type="number" min="0" max="10" title="Сколько раз повторять неудачную загрузку" value={dl.retries} oninput={(e) => updateDl({ retries: Math.max(0, Math.min(10, Number((e.currentTarget as HTMLInputElement).value) || 0)) })} /></label>
      <label class="full">Cookies (путь к .txt)
        <input type="text" value={dl.cookieFile ?? ''} placeholder="C:\path\to\cookies.txt" title="Файл cookies в формате Netscape для приватных/возрастных видео" oninput={(e) => updateDl({ cookieFile: (e.currentTarget as HTMLInputElement).value || undefined })} />
      </label>
      <label class="full">User-Agent (пусто = default)
        <input type="text" value={dl.userAgent ?? ''} placeholder="Mozilla/5.0 ..." title="Свой User-Agent, если сервер блокирует стандартный" oninput={(e) => updateDl({ userAgent: (e.currentTarget as HTMLInputElement).value || undefined })} />
      </label>
      <label class="check"><input type="checkbox" checked={dl.audioOnly} onchange={(e) => updateDl({ audioOnly: (e.currentTarget as HTMLInputElement).checked })} />Только аудио</label>
      <label class="check"><input type="checkbox" checked={dl.embedSubs} onchange={(e) => updateDl({ embedSubs: (e.currentTarget as HTMLInputElement).checked })} />Вшить субтитры</label>
      <label class="check"><input type="checkbox" checked={dl.overwrite} onchange={(e) => updateDl({ overwrite: (e.currentTarget as HTMLInputElement).checked })} />Перезапись</label>
    </div>
  </section>

  <section>
    <h3>ASR (GigaAM-V3)</h3>
    <div class="grid">
      <label class="full">Папка модели (пусто = <code>&lt;exe_dir&gt;/models</code>, авто-скачивание)
        <div class="row">
          <input
            type="text"
            value={asr.modelDir}
            placeholder="C:\models\gigaam-v3  или  cmd:gigaam-cli --foo"
            title={asr.modelDir || 'Пусто: модель будет скачана автоматически'}
            oninput={(e) => updateAsr({ modelDir: (e.currentTarget as HTMLInputElement).value })}
          />
          <button type="button" class="btn" onclick={pickModelDir}>Обзор…</button>
        </div>
      </label>
      <label>Частота (Гц)
        <input
          type="number" min="8000" max="48000" step="1000"
          value={asr.sampleRate}
          title="8000..48000 Гц (для GigaAM рекомендуется 16000)"
          aria-invalid={!!errors['asr.sampleRate']}
          oninput={(e) => updateAsr({ sampleRate: Math.max(8000, Math.min(48000, Number((e.currentTarget as HTMLInputElement).value) || 16000)) })}
        />
        {#if errors['asr.sampleRate']}<span class="err-msg">{errors['asr.sampleRate']}</span>{/if}
      </label>
      <label>Язык<input type="text" value={asr.language} title="Код языка (ru, en, …)" oninput={(e) => updateAsr({ language: (e.currentTarget as HTMLInputElement).value })} /></label>
      <label>Устройство
        <select value={asr.device} title="cpu = портативно; cuda = NVIDIA GPU; directml = любая GPU; openvino = Intel GPU/NPU" onchange={(e) => updateAsr({ device: (e.currentTarget as HTMLSelectElement).value as AsrDevice })}>
          <option value="cpu">CPU</option>
          <option value="cuda">CUDA</option>
          <option value="directml">DirectML</option>
          <option value="openvino">OpenVINO</option>
        </select>
      </label>
      <label>Beam size
        <input type="number" min="1" max="64" step="1" title="1 = быстрый greedy; 2..64 = beam search (точнее, но медленнее)" value={asr.beamSize} oninput={(e) => updateAsr({ beamSize: Math.max(1, Math.min(64, Number((e.currentTarget as HTMLInputElement).value) || 1)) })} />
      </label>
    </div>
  </section>

  <section>
    <h3>Выход</h3>
    <div class="grid">
      <div class="full" role="group" aria-label="Форматы вывода">
        <span class="lbl">Форматы</span>
        <div class="chips">
          {#each ['txt','srt','json'] as f}
            <button type="button" class="chip" class:on={out.formats.includes(f)} onclick={() => toggleFmt(f)} aria-pressed={out.formats.includes(f)}>{f}</button>
          {/each}
        </div>
        {#if errors['out.formats']}<span class="err-msg">{errors['out.formats']}</span>{/if}
      </div>
      <label class="full">Папка (пусто = default)
        <div class="row">
          <input
            type="text"
            value={out.dir}
            title={out.dir || 'Пусто: <data_root>/data/jobs/<id>/'}
            placeholder="C:\transcripts"
            oninput={(e) => updateOut({ dir: (e.currentTarget as HTMLInputElement).value })}
          />
          <button type="button" class="btn" onclick={pickOutDir}>Обзор…</button>
        </div>
      </label>
    </div>
  </section>

  <section>
    <h3>Логи</h3>
    <div class="grid">
      <label>Уровень
        <select value={log.level} title="error = только ошибки; trace = максимально подробно" onchange={(e) => updateLog({ level: (e.currentTarget as HTMLSelectElement).value })}>
          {#each ['error','warn','info','debug','trace'] as l}<option value={l}>{l}</option>{/each}
        </select>
      </label>
      <label>Макс. MB<input type="number" min="1" max="500" title="Размер лог-файла до ротации" value={log.maxSizeMb} oninput={(e) => updateLog({ maxSizeMb: Math.max(1, Math.min(500, Number((e.currentTarget as HTMLInputElement).value) || 5)) })} /></label>
      <label>Файлов<input type="number" min="1" max="50" title="Сколько старых логов хранить после ротации" value={log.keepFiles} oninput={(e) => updateLog({ keepFiles: Math.max(1, Math.min(50, Number((e.currentTarget as HTMLInputElement).value) || 3)) })} /></label>
    </div>
  </section>
</div>

<style>
  .settings { display: flex; flex-direction: column; gap: 12px; min-height: 0; }
  .head { display: flex; align-items: center; justify-content: space-between; }
  .title { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  h3 { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); margin: 0 0 6px; }
  section { background: var(--surface-1); border: 1px solid var(--border); border-radius: 4px; padding: 10px; }
  .grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 6px 10px; }
  label { display: flex; flex-direction: column; gap: 2px; font-size: 11px; color: var(--muted); }
  label.full { grid-column: 1 / -1; }
  label.check { flex-direction: row; align-items: center; gap: 4px; cursor: pointer; }
  .row { display: flex; gap: 4px; }
  .row input { flex: 1; }
  .actions { display: flex; gap: 4px; }
  .btn {
    background: var(--surface-2); border: 1px solid var(--border); border-radius: 4px;
    padding: 4px 10px; cursor: pointer; font-size: 12px; color: var(--fg);
  }
  .btn:hover { background: var(--surface-3); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:disabled { opacity: 0.4; cursor: not-allowed; }
  .chips { display: flex; gap: 4px; }
  .chip {
    background: var(--surface-2); border: 1px solid var(--border);
    color: var(--muted); border-radius: 3px;
    padding: 3px 8px; cursor: pointer; font-size: 11px;
    text-transform: uppercase; letter-spacing: 0.04em;
  }
  .chip.on { background: var(--accent); color: #fff; border-color: var(--accent); }
  .chip:focus-visible { outline: 1px solid var(--accent); }
  .lbl { font-size: 11px; color: var(--muted); margin-bottom: 2px; display: block; }
  /* Inline-валидация: красная рамка + сообщение под полем. */
  input[aria-invalid="true"] { border-color: var(--err); }
  .err-msg {
    font-size: 10px; color: var(--err); margin-top: 1px;
    /* оставляем label в 2 колонки, чтобы лейаут не дёргался */
  }
</style>
