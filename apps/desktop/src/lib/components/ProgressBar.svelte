<script lang="ts">
  type Props = { pct: number; label?: string };
  let { pct }: Props = $props();
  const w = $derived(`${(Math.max(0, Math.min(1, pct)) * 100).toFixed(1)}%`);
</script>

<div class="bar">
  <div class="fill" class:done={pct >= 1} style="width: {w}"></div>
</div>

<style>
  .bar {
    height: 2px; background: var(--surface-3);
    border-radius: 1px; overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 120ms ease;
  }
  .fill.done { background: var(--ok); }
</style>
