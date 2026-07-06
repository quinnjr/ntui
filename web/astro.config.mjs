// @ts-check
import { defineConfig } from 'astro/config';

import tailwindcss from '@tailwindcss/vite';

import sitemap from '@astrojs/sitemap';

// https://astro.build/config
export default defineConfig({
  // Served from GitHub Pages as a project site: https://quinnjr.github.io/ntui/
  site: 'https://quinnjr.github.io',
  base: '/ntui',

  vite: {
    plugins: [tailwindcss()]
  },

  integrations: [sitemap()]
});