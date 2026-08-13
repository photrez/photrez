/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}'],
  theme: {
    extend: {
      fontFamily: {
        heading: ['"Space Grotesk"', 'sans-serif'],
        sans: ['"Inter"', 'sans-serif'],
      },
      colors: {
        brand: {
          amber: '#E15A17',
          'amber-light': '#FFB31A',
          dark: '#121214',
          panel: '#18181B',
        }
      }
    },
  },
  plugins: [],
};
