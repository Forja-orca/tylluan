/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        background: '#0a0a0c',
        foreground: '#e2e2e2',
        glass: 'rgba(255, 255, 255, 0.05)',
        'glass-border': 'rgba(255, 255, 255, 0.1)',
        primary: {
          DEFAULT: '#00f2fe',
          glow: 'rgba(0, 242, 254, 0.5)',
        },
        secondary: {
          DEFAULT: '#a01fe9',
          glow: 'rgba(160, 31, 233, 0.5)',
        }
      },
      backdropBlur: {
        xs: '2px',
      },
      animation: {
        'pulse-slow': 'pulse 4s cubic-bezier(0.4, 0, 0.6, 1) infinite',
      }
    },
  },
  plugins: [],
}
