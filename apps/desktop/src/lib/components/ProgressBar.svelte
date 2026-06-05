<script lang="ts">
  type Props = { pct: number; label?: string; height?: number };
  let { pct, label, height = 6 }: Props = $props();

  const clamped = $derived(Math.max(0, Math.min(1, pct)));
  const width = $derived(`${(clamped * 100).toFixed(1)}%`);
  const variant = $derived(
    clamped >= 1 ? 'done' : clamped > 0 ? 'progress' : 'idle',
  );
</script>

<div class="wrap" style="--h: {height}px">
  <div class="bar {variant}"><div class="fill" style="width: {width}"></div></div>
  {#if label}<div class="lbl">{label}</div>{/if}
</div>

<style>
  .wrap { display: flex; flex-direction: column; gap: 4px; }
  .bar {
    height: var(--h);
    background: var(--surface-2, #1c1f24);
    border-radius: 999px;
    overflow: hidden;
    border: 1px solid var(--border, #2a2e35);
  }
  .fill {
    height: 100%;
    background: linear-gradient(90deg, var(--accent, #5b8def), color-mix(in oklab, var(--accent, #5b8def) 60%, #b58cff));
    transition: width 120ms ease;
  }
  .done .fill { background: var(--ok, #38c172); }
  .lbl { font-size: 12px; color: var(--muted, #8a93a3); }
</style>
