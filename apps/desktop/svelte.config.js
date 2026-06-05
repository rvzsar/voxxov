import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').SvelteConfig} */
export default {
  preprocess: vitePreprocess(),
  compilerOptions: {
    // Svelte 5 runes
    runes: true,
  },
  onwarn: (warning, handler) => {
    // Подавляем a11y-варнинги, которые шумят в лаконичном UI
    if (warning.code?.startsWith('a11y-')) return;
    handler?.(warning);
  },
};
