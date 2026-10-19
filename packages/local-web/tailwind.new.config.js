/** @type {import('tailwindcss').Config} */

const sizes = {
  '2xs': 0.5,
  xs: 0.75,
  sm: 0.875,
  base: 1,
  lg: 1.125,
  xl: 1.25,
}

const lineHeightMultiplier = 1.5;
const iconMultiplier = 1.25;
const chatMaxWidth = '48rem';

function getSize(sizeLabel, multiplier = 1) {

  return sizes[sizeLabel] * multiplier + "rem";
}

module.exports = {
  darkMode: ["class"],
  important: false,
  content: [
    './pages/**/*.{ts,tsx}',
    './components/**/*.{ts,tsx}',
    './app/**/*.{ts,tsx}',
    './src/**/*.{ts,tsx}',
    '../web-core/src/**/*.{ts,tsx}',
    '../remote-web/src/**/*.{ts,tsx}',
    '../ui/src/**/*.{ts,tsx}',
    "node_modules/@rjsf/shadcn/src/**/*.{js,ts,jsx,tsx,mdx}"
  ],
  safelist: [
    'xl:hidden',
    'xl:relative',
    'xl:inset-auto',
    'xl:z-auto',
    'xl:h-full',
    'xl:w-[800px]',
    'xl:flex',
    'xl:flex-1',
    'xl:min-w-0',
    'xl:overflow-y-auto',
    'xl:opacity-100',
    'xl:pointer-events-auto',
  ],
  prefix: "",
  theme: {
    container: {
      center: true,
      padding: "2rem",
      screens: {
        "2xl": "1400px",
      },
    },
    extend: {
      height: {
        'cta': '29px',
      },
      minHeight: {
        'cta': '29px',
      },
      width: {
        chat: chatMaxWidth,
      },
      containers: {
        chat: chatMaxWidth,
      },
      size: {
        'icon-2xs': getSize('2xs', iconMultiplier),
        'icon-xs': getSize('xs', iconMultiplier),
        'icon-sm': getSize('sm', iconMultiplier),
        'icon-base': getSize('base', iconMultiplier),
        'icon-lg': getSize('lg', iconMultiplier),
        'icon-xl': getSize('xl', iconMultiplier),
        'dot': '0.3rem', // 6px - for animated indicator dots
      },
      backgroundImage: {
        'diagonal-lines': `
          repeating-linear-gradient(-45deg, hsl(var(--text-low) / 0.4) 0 2px, transparent 1px 12px),
          linear-gradient(hsl(var(--bg-primary)), hsl(var(--bg-primary)))
        `,
      },
      ringColor: {
        DEFAULT: 'hsl(var(--brand))',
      },
      fontSize: {
        xs: [getSize('xs'), { lineHeight: getSize('xs', lineHeightMultiplier) }],      // 12px (meta, chips)
        sm: [getSize('sm'), { lineHeight: getSize('sm', lineHeightMultiplier) }],   // 14px (body, card titles)
        base: [getSize('base'), { lineHeight: getSize('base', lineHeightMultiplier) }],  // 16px
        lg: [getSize('lg'), { lineHeight: getSize('lg', lineHeightMultiplier) }],    // 18px (panel titles)
        xl: [getSize('xl'), { lineHeight: getSize('xl', lineHeightMultiplier) }],         // 20px
        cta: [getSize('base'), { lineHeight: getSize('base') }],         // 16px
      },
      spacing: {
        'half': getSize('base', 0.25),
        'base': getSize('base', 0.5),
        'plusfifty': getSize('base', 0.75),
        'double': getSize('base', 1),
      },
      colors: {
        // Text colors: text-high, text-normal, text-low
        high: "hsl(var(--text-high))",
        normal: "hsl(var(--text-normal))",
        low: "hsl(var(--text-low))",
        // Background colors: bg-primary, bg-secondary, bg-panel
        primary: "hsl(var(--bg-primary))",
        secondary: "hsl(var(--bg-secondary))",
        panel: "hsl(var(--bg-panel))",
        // Surfaces: bg-surface-raised (archived/muted cards), border-divider
        // (hairline inside a card)
        'surface-raised': "hsl(var(--surface-raised))",
        divider: "hsl(var(--divider))",
        // Accent colors. The brand is blue #3F6AA6 in both modes;
        // brand-secondary is brand-colored text (steps lighter in dark).
        brand: "hsl(var(--brand))",
        'brand-hover': "hsl(var(--brand-hover))",
        'brand-secondary': "hsl(var(--brand-secondary))",
        'brand-soft': "hsl(var(--brand-soft))",
        'brand-soft-foreground': "hsl(var(--brand-soft-foreground))",
        error: "hsl(var(--error))",
        'error-soft': "hsl(var(--error-soft))",
        success: "hsl(var(--success))",
        'success-soft': "hsl(var(--success-soft))",
        merged: "hsl(var(--merged))",
        // "Needs you" amber: dot (attention), text (attention-foreground),
        // surface (attention-soft), border (attention-border)
        attention: "hsl(var(--attention))",
        'attention-foreground': "hsl(var(--attention-foreground))",
        'attention-soft': "hsl(var(--attention-soft))",
        'attention-border': "hsl(var(--attention-border))",
        // Text on accent
        'on-brand': "hsl(var(--text-on-brand))",
        // Dialog scrim, always used with an opacity: bg-scrim/45
        scrim: "hsl(var(--scrim) / <alpha-value>)",
        // shadcn-style colors (used by @apply in CSS base layer)
        background: "hsl(var(--bg-primary))",
        foreground: "hsl(var(--text-normal))",
        border: "hsl(var(--border))",
      },
      borderColor: {
        DEFAULT: "hsl(var(--border))",
        border: "hsl(var(--border))",
      },
      // `rounded` follows --radius (0.5rem): buttons, inputs, menus.
      // Cards use rounded-lg (10px), dialogs rounded-xl (12px), chips
      // rounded-full.
      borderRadius: {
        DEFAULT: 'var(--radius)',
        sm: '0.375rem',
        md: '0.5rem',
        lg: '0.625rem',
        xl: '0.75rem',
      },
      boxShadow: {
        // Menus, popovers, tooltips and dialogs. Cards carry no shadow.
        overlay: '0 12px 32px -8px rgb(31 35 40 / 0.16), 0 2px 6px rgb(31 35 40 / 0.06)',
        // A side panel floating over the board
        panel: '-8px 0 24px rgb(31 35 40 / 0.05)',
      },
      borderWidth: {
        base: getSize('base'),
        half: getSize('base', 0.5),
      },
      fontFamily: {
        sans: ['Figtree', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'ui-monospace', 'monospace'],
      },
      keyframes: {
        "accordion-down": {
          from: { height: "0" },
          to: { height: "var(--radix-accordion-content-height)" },
        },
        "accordion-up": {
          from: { height: "var(--radix-accordion-content-height)" },
          to: { height: "0" },
        },
        pill: {
          '0%': { opacity: '0' },
          '10%': { opacity: '1' },
          '80%': { opacity: '1' },
          '100%': { opacity: '0' },
        },
        'running-dot': {
          '0%, 100%': { opacity: '0.3' },
          '50%': { opacity: '1' },
        },
        'border-flash': {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
        shake: {
          '0%, 100%': { transform: 'translateX(0)' },
          '10%, 30%, 50%, 70%, 90%': { transform: 'translateX(-2px)' },
          '20%, 40%, 60%, 80%': { transform: 'translateX(2px)' },
        },
      },
      animation: {
        "accordion-down": "accordion-down 0.2s ease-out",
        "accordion-up": "accordion-up 0.2s ease-out",
        pill: 'pill 2s ease-in-out forwards',
        'running-dot-1': 'running-dot 1.4s ease-in-out infinite',
        'running-dot-2': 'running-dot 1.4s ease-in-out 0.2s infinite',
        'running-dot-3': 'running-dot 1.4s ease-in-out 0.4s infinite',
        'border-flash': 'border-flash 2s linear infinite',
        shake: 'shake 0.3s ease-in-out',
      },
    },
  },
  plugins: [require("tailwindcss-animate"), require("@tailwindcss/container-queries"), require("tailwind-scrollbar")({ nocompatible: true })],
}
