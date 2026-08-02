import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import tailwindcss from 'tailwindcss';

// https://astro.build/config
export default defineConfig({
  site: 'https://photrez.github.io',
  integrations: [
    sitemap(),
  ],
  // Tailwind 3 via PostCSS — @astrojs/tailwind is deprecated and does not
  // support Astro 6 (it dragged in a vulnerable nested astro@5).
  vite: {
    css: {
      postcss: {
        plugins: [tailwindcss()],
      },
    },
  },
});
