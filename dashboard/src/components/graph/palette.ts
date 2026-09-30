/**
 * Shared node/cluster color language for the knowledge cortex visualization.
 * Owl-branding derived: deep indigo/navy base, cyan-silver accents instead of
 * the generic emerald/slate SaaS palette used elsewhere in the dashboard.
 */

export const NODE_TYPE_COLOR: Record<string, string> = {
  // ── Real SilvaDB node_type distribution (audited live 2026-09-30 over
  // /api/v1/graph/viz?limit=500: session_digest 187, code_entity 162,
  // coloquio_memory 43, summary 34, agent_reputation 25, routing_anchor 16,
  // identity 14, ...). The palette maps the REAL types first — machinery
  // gets visually quiet colors so the signal nodes pop, exactly the
  // "colores con significado, no decorativos" brief.
  session_digest:   '#3f4a63', // dim blue-slate — bulk machinery, intentionally quiet
  code_entity:      '#4ade80', // green — source-code artifacts
  coloquio_memory:  '#38bdf8', // sky — team conversation memory
  summary:          '#2dd4bf', // teal — synthesized knowledge
  agent_reputation: '#c084fc', // light purple — scoring/signal about agents
  routing_anchor:   '#475569', // dark slate — internal machinery, near-invisible
  identity:         '#f472b6', // pink — the sovereign self, always distinct
  lesson:           '#fbbf24', // amber — a lesson should stand out, it cost something
  agent_memory:     '#f472b6', // pink — attributed to an agent
  synthesis:        '#2dd4bf', // teal — same family as summary
  entity:           '#22d3ee', // cyan
  memory_document:  '#22d3ee', // cyan — ingested documents
  episodic:         '#818cf8', // indigo
  // ── Legacy/ui-core union (kept for parity with the 3D cortex palette)
  concept:    '#38bdf8', // sky
  episode:    '#818cf8', // indigo
  experience: '#818cf8',
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
