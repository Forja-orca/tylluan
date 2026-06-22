/// <reference types="vite/client" />
declare module 'd3-force-3d' {
  export function forceX(x?: number | ((d: any) => number)): any;
  export function forceY(y?: number | ((d: any) => number)): any;
  export function forceCollide(radius?: number | ((d: any) => number)): any;
}
