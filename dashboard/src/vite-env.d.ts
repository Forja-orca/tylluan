/// <reference types="vite/client" />
declare module 'd3-force-3d' {
  export function forceX(x?: number | ((d: unknown) => number)): unknown;
  export function forceY(y?: number | ((d: unknown) => number)): unknown;
  export function forceCollide(radius?: number | ((d: unknown) => number)): unknown;
}

