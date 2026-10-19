import type { ITheme } from '@xterm/xterm';

/**
 * Convert HSL CSS variable value (e.g., "210 40% 98%") to hex color.
 */
function hslToHex(hslValue: string): string {
  const trimmed = hslValue.trim();
  if (!trimmed) return '#000000';

  // Parse "H S% L%" format (space-separated, S and L have % suffix)
  const parts = trimmed.split(/\s+/);
  if (parts.length < 3) return '#000000';

  const h = parseFloat(parts[0]) / 360;
  const s = parseFloat(parts[1]) / 100;
  const l = parseFloat(parts[2]) / 100;

  // HSL to RGB conversion
  let r: number, g: number, b: number;

  if (s === 0) {
    r = g = b = l;
  } else {
    const hue2rgb = (p: number, q: number, t: number) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };

    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hue2rgb(p, q, h + 1 / 3);
    g = hue2rgb(p, q, h);
    b = hue2rgb(p, q, h - 1 / 3);
  }

  const toHex = (x: number) => {
    const hex = Math.round(x * 255).toString(16);
    return hex.length === 1 ? '0' + hex : hex;
  };

  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

/**
 * Get the CSS variable value from the computed styles.
 */
function getCssVariable(name: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
}

/**
 * Build an xterm.js theme from the console tokens in index.css. The console
 * surface (--console-background / --console-foreground) is dark in both
 * modes, so the ANSI palette is always the one tuned for a dark ground.
 * --console-success / --console-error are tuned for light surfaces in light
 * mode (diff counts use them), so in light mode green and red fall back to
 * the dark-mode success and error values, which read on the dark console.
 */
export function getTerminalTheme(): ITheme {
  const isDark = document.documentElement.classList.contains('dark');
  const bgHex = hslToHex(getCssVariable('--console-background'));
  const fgHex = hslToHex(getCssVariable('--console-foreground'));
  const greenHex = isDark
    ? hslToHex(getCssVariable('--console-success'))
    : '#5fc08a';
  const redHex = isDark
    ? hslToHex(getCssVariable('--console-error'))
    : '#f0a39b';

  return {
    background: bgHex,
    foreground: fgHex,
    cursor: fgHex,
    cursorAccent: bgHex,
    selectionBackground: '#3a4a63',
    selectionForeground: fgHex,
    black: '#2d2e33',
    red: redHex,
    green: greenHex,
    yellow: '#e7b94c',
    blue: '#8fb0de',
    magenta: '#bba7f0',
    cyan: '#7fc8d6',
    white: '#c9cbd0',
    brightBlack: '#6a6f78',
    brightRed: redHex,
    brightGreen: greenHex,
    brightYellow: '#e7b94c',
    brightBlue: '#9dbce8',
    brightMagenta: '#bba7f0',
    brightCyan: '#9ad6e2',
    brightWhite: fgHex,
  };
}
