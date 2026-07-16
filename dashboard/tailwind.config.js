/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: [
    './src/**/*.{ts,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        slate: {
          50: "hsl(var(--slate-50) / <alpha-value>)",
          100: "hsl(var(--slate-100) / <alpha-value>)",
          200: "hsl(var(--slate-200) / <alpha-value>)",
          300: "hsl(var(--slate-300) / <alpha-value>)",
          400: "hsl(var(--slate-400) / <alpha-value>)",
          500: "hsl(var(--slate-500) / <alpha-value>)",
          600: "hsl(var(--slate-600) / <alpha-value>)",
          700: "hsl(var(--slate-700) / <alpha-value>)",
          800: "hsl(var(--slate-800) / <alpha-value>)",
          850: "hsl(var(--slate-850) / <alpha-value>)",
          900: "hsl(var(--slate-900) / <alpha-value>)",
          950: "hsl(var(--slate-950) / <alpha-value>)",
        },
        emerald: {
          400: "hsl(var(--emerald-400) / <alpha-value>)",
          500: "hsl(var(--emerald-500) / <alpha-value>)",
          600: "hsl(var(--emerald-600) / <alpha-value>)",
          700: "hsl(var(--emerald-700) / <alpha-value>)",
        },
        amber: {
          400: "hsl(var(--amber-400) / <alpha-value>)",
          500: "hsl(var(--amber-500) / <alpha-value>)",
        },
        red: {
          400: "hsl(var(--red-400) / <alpha-value>)",
          500: "hsl(var(--red-500) / <alpha-value>)",
        },
        rose: {
          400: "hsl(var(--rose-400) / <alpha-value>)",
          500: "hsl(var(--rose-500) / <alpha-value>)",
          600: "hsl(var(--rose-600) / <alpha-value>)",
          700: "hsl(var(--rose-700) / <alpha-value>)",
        },
        indigo: {
          500: "hsl(var(--indigo-500) / <alpha-value>)",
          600: "hsl(var(--indigo-600) / <alpha-value>)",
        },
        violet: {
          500: "hsl(var(--violet-500) / <alpha-value>)",
          600: "hsl(var(--violet-600) / <alpha-value>)",
        },
        blue: {
          400: "hsl(var(--blue-400) / <alpha-value>)",
          500: "hsl(var(--blue-500) / <alpha-value>)",
        },
        primary: {
          DEFAULT: "hsl(var(--primary))",
          foreground: "hsl(var(--primary-foreground))",
        },
        secondary: {
          DEFAULT: "hsl(var(--secondary))",
          foreground: "hsl(var(--secondary-foreground))",
        },
        destructive: {
          DEFAULT: "hsl(var(--destructive))",
          foreground: "hsl(var(--destructive-foreground))",
        },
        muted: {
          DEFAULT: "hsl(var(--muted))",
          foreground: "hsl(var(--muted-foreground))",
        },
        accent: {
          DEFAULT: "hsl(var(--accent))",
          foreground: "hsl(var(--accent-foreground))",
        },
        popover: {
          DEFAULT: "hsl(var(--popover))",
          foreground: "hsl(var(--popover-foreground))",
        },
        card: {
          DEFAULT: "hsl(var(--card))",
          foreground: "hsl(var(--card-foreground))",
        },
      },
      borderRadius: {
        lg: "var(--radius)",
        md: "calc(var(--radius) - 2px)",
        sm: "calc(var(--radius) - 4px)",
      },
    },
  },
  plugins: [],
}
