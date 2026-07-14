/**
 * Shared node/cluster color language for the knowledge cortex visualization.
 * Owl-branding derived: deep indigo/navy base, cyan-silver accents instead of
 * the generic emerald/slate SaaS palette used elsewhere in the dashboard.
 */

export const NODE_TYPE_COLOR: Record<string, string> = {
  concept:    '#38bdf8', // sky
  episode:    '#818cf8', // indigo
  lesson:     '#fbbf24', // amber — a lesson should stand out, it cost something
  experience: '#818cf8',
  identity:   '#f472b6', // pink — the sovereign self, always distinct
  tool_call:  '#a78bfa', // violet
  agent:      '#f472b6',
  image:      '#fb923c', // orange
  document:   '#22d3ee', // cyan
  system:     '#94a3b8', // slate
  agnostic:   '#64748b',
};
export const DEFAULT_NODE_COLOR = '#64748b';

export function nodeTypeColor(type?: string): string {
  return NODE_TYPE_COLOR[type || 'agnostic'] ?? DEFAULT_NODE_COLOR;
}

// Louvain community ring colors — distinct hue family from node-type fills
// so cluster membership and semantic type never get confused.
export const CLUSTER_RING_COLOR = [
  '#22d3ee', '#a78bfa', '#fbbf24', '#fb7185', '#4ade80',
  '#60a5fa', '#f472b6', '#facc15', '#818cf8', '#2dd4bf',
];

export function clusterRingColor(id?: number): string | null {
  if (id === undefined || id === null) return null;
  return CLUSTER_RING_COLOR[id % CLUSTER_RING_COLOR.length];
}

// Deep-space navy, matching the owl logo's night-sky motif — not pure black.
export const CORTEX_BACKGROUND = '#040918';
