export default {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        slate: {
          950: '#030712',
        },
        // Semantic status colors — use these (bg-success-500, text-danger-400, ...)
        // instead of raw green/red/amber/cyan/purple so a future palette change
        // is a one-line edit here rather than a grep-and-replace across tabs.
        success: {
          400: '#4ade80', 500: '#22c55e', 600: '#16a34a', 900: '#14532d',
        },
        warning: {
          400: '#fb923c', 500: '#f97316', 600: '#ea580c',
        },
        danger: {
          400: '#f87171', 500: '#ef4444', 600: '#dc2626', 900: '#7f1d1d', 950: '#450a0a',
        },
        info: {
          400: '#22d3ee', 500: '#06b6d4', 600: '#0891b2',
        },
        accent: {
          400: '#c084fc', 500: '#a855f7', 600: '#9333ea',
        },
      },
    },
  },
  plugins: [],
};
