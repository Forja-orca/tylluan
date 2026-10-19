/**
 * canvas-color.ts
 *
 * Resolves color strings (hex or CSS token variables) to valid Canvas 2D color strings.
 * Canvas 2D contexts do not evaluate CSS variables directly via var(...),
 * so this utility resolves them against the DOM computed style.
 */

export function resolveCanvasColor(
  color: string,
  targetEl?: HTMLElement | null
): { stroke: string; fill: (alpha: number) => string } {
  if (color.startsWith('#')) {
    const hex = color.length === 4
      ? `#${color[1]}${color[1]}${color[2]}${color[2]}${color[3]}${color[3]}`
      : color;
    const r = parseInt(hex.slice(1, 3), 16) || 0;
    const g = parseInt(hex.slice(3, 5), 16) || 0;
    const b = parseInt(hex.slice(5, 7), 16) || 0;
    return {
      stroke: color,
      fill: (alpha: number) => `rgba(${r}, ${g}, ${b}, ${alpha})`,
    };
  }

  const varMatch = color.match(/var\((--[^)]+)\)/);
  if (varMatch && typeof window !== 'undefined') {
    const varName = varMatch[1];
    const el = targetEl || document.documentElement;
    const rawVal = window.getComputedStyle(el).getPropertyValue(varName).trim();
    if (rawVal) {
      return {
        stroke: `hsl(${rawVal})`,
        fill: (alpha: number) => `hsl(${rawVal} / ${alpha})`,
      };
    }
  }

  return {
    stroke: color,
    fill: () => color,
  };
}
