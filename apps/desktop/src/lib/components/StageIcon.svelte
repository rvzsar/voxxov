<script lang="ts">
  import type { JobStage } from '../types';

  type Props = { stage: JobStage; size?: number };
  let { stage, size = 14 }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if stage === 'queued'}
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.5V8l2.5 2" />
  {:else if stage === 'fetching_metadata'}
    <circle cx="7" cy="7" r="4.5" />
    <path d="M10.5 10.5 14 14" />
  {:else if stage === 'downloading'}
    <path d="M8 2.5V10" />
    <path d="M4.5 7 8 10.5 11.5 7" />
    <path d="M2.5 13h11" />
  {:else if stage === 'extracting_audio'}
    <path d="M6.5 11.5V3.5L12 2v8" />
    <circle cx="4.5" cy="11.5" r="1.5" />
    <circle cx="10.5" cy="10" r="1.5" />
  {:else if stage === 'transcribing'}
    <rect x="6" y="2.5" width="4" height="6" rx="2" />
    <path d="M3.5 8a4.5 4.5 0 0 0 9 0" />
    <path d="M8 12.5V14" />
  {:else if stage === 'done'}
    <path d="M2.5 8.5 6 12l7.5-8.5" />
  {:else if stage === 'failed'}
    <path d="M4 4l8 8M12 4l-8 8" />
  {:else}
    <rect x="4.5" y="4.5" width="7" height="7" rx="1" />
  {/if}
</svg>
