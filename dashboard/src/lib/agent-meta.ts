/**
 * Unified agent identity metadata — single source of truth for FleetTab,
 * ColoquioAgentsPanel, and ColoquioMessagesPanel.
 *
 * If a new agent is added, this is the ONLY file to update.
 */

export interface AgentStyle {
  color: string;
  bg: string;
  ring: string;
  border: string;
  accentBorder: string;
  label: string;
  initial: string;
}

const AGENTS: Record<string, AgentStyle> = {
  jose: {
    color: 'text-emerald-700 dark:text-emerald-300',
    bg: 'bg-emerald-500/10 dark:bg-emerald-950/60',
    ring: 'ring-emerald-500/40',
    border: 'border-emerald-500/35 dark:border-emerald-500/40',
    accentBorder: 'border-l-emerald-600 dark:border-l-emerald-400',
    label: 'Jose',
    initial: 'J',
  },
  'claude-code': {
    color: 'text-primary',
    bg: 'bg-primary/10 dark:bg-primary/15',
    ring: 'ring-primary/40',
    border: 'border-primary/35 dark:border-primary/40',
    accentBorder: 'border-l-primary',
    label: 'Claude',
    initial: 'C',
  },
  mimo: {
    color: 'text-teal-700 dark:text-teal-300',
    bg: 'bg-teal-500/10 dark:bg-teal-950/60',
    ring: 'ring-teal-500/40',
    border: 'border-teal-500/35 dark:border-teal-500/40',
    accentBorder: 'border-l-teal-600 dark:border-l-teal-400',
    label: 'Mimo',
    initial: 'M',
  },
  deep: {
    color: 'text-cyan-700 dark:text-cyan-300',
    bg: 'bg-cyan-500/10 dark:bg-cyan-950/60',
    ring: 'ring-cyan-500/40',
    border: 'border-cyan-500/35 dark:border-cyan-500/40',
    accentBorder: 'border-l-cyan-600 dark:border-l-cyan-400',
    label: 'Deep',
    initial: 'D',
  },
  opencode: {
    color: 'text-amber-700 dark:text-amber-300',
    bg: 'bg-amber-500/10 dark:bg-amber-950/60',
    ring: 'ring-amber-500/40',
    border: 'border-amber-500/35 dark:border-amber-500/40',
    accentBorder: 'border-l-amber-600 dark:border-l-amber-400',
    label: 'OpenCode',
    initial: 'O',
  },
  antigravity: {
    color: 'text-violet-700 dark:text-violet-300',
    bg: 'bg-violet-500/10 dark:bg-violet-950/60',
    ring: 'ring-violet-500/40',
    border: 'border-violet-500/35 dark:border-violet-500/40',
    accentBorder: 'border-l-violet-600 dark:border-l-violet-400',
    label: 'Antigravity',
    initial: 'A',
  },
  buffy: {
    color: 'text-rose-700 dark:text-rose-300',
    bg: 'bg-rose-500/10 dark:bg-rose-950/60',
    ring: 'ring-rose-500/40',
    border: 'border-rose-500/35 dark:border-rose-500/40',
    accentBorder: 'border-l-rose-600 dark:border-l-rose-400',
    label: 'Buffy',
    initial: 'B',
  },
  qwen: {
    color: 'text-blue-700 dark:text-blue-300',
    bg: 'bg-blue-500/10 dark:bg-blue-950/60',
    ring: 'ring-blue-500/40',
    border: 'border-blue-500/35 dark:border-blue-500/40',
    accentBorder: 'border-l-blue-600 dark:border-l-blue-400',
    label: 'Qwen',
    initial: 'Q',
  },
  dsh: {
    color: 'text-indigo-700 dark:text-indigo-300',
    bg: 'bg-indigo-500/10 dark:bg-indigo-950/60',
    ring: 'ring-indigo-500/40',
    border: 'border-indigo-500/35 dark:border-indigo-500/40',
    accentBorder: 'border-l-indigo-600 dark:border-l-indigo-400',
    label: 'DSH',
    initial: 'D',
  },
  'agent-as-system': {
    color: 'text-slate-600 dark:text-slate-300',
    bg: 'bg-slate-500/10 dark:bg-slate-800/60',
    ring: 'ring-slate-500/40',
    border: 'border-slate-500/35 dark:border-slate-600/40',
    accentBorder: 'border-l-slate-500 dark:border-l-slate-400',
    label: 'System',
    initial: 'S',
  },
  system: {
    color: 'text-slate-600 dark:text-slate-300',
    bg: 'bg-slate-500/10 dark:bg-slate-800/60',
    ring: 'ring-slate-500/40',
    border: 'border-slate-500/35 dark:border-slate-600/40',
    accentBorder: 'border-l-slate-500 dark:border-l-slate-400',
    label: 'System',
    initial: 'S',
  },
  kernel: {
    color: 'text-slate-600 dark:text-slate-300',
    bg: 'bg-slate-500/10 dark:bg-slate-800/60',
    ring: 'ring-slate-500/30',
    border: 'border-slate-500/35 dark:border-slate-600/40',
    accentBorder: 'border-l-slate-500 dark:border-l-slate-400',
    label: 'Kernel',
    initial: 'K',
  },
};

const FALLBACK: AgentStyle = {
  color: 'text-slate-600 dark:text-slate-300',
  bg: 'bg-slate-500/10 dark:bg-slate-800/60',
  ring: 'ring-slate-600/30',
  border: 'border-slate-500/35 dark:border-slate-600/40',
  accentBorder: 'border-l-slate-500 dark:border-l-slate-400',
  label: 'Unknown',
  initial: '?',
};

/** Ordered list of known agent IDs (used by FleetTab for deterministic ordering). */
export const KNOWN_AGENTS = Object.keys(AGENTS);

/**
 * Match an agent ID (e.g. "agent-mimo-3fa2") to its style by substring.
 * Returns FALLBACK with the agent's initial if no match.
 */
export function agentStyle(id: string): AgentStyle {
  const key = Object.keys(AGENTS).find(k => id.toLowerCase().includes(k));
  return key ? { ...AGENTS[key] } : { ...FALLBACK, initial: id[0]?.toUpperCase() ?? '?' };
}
