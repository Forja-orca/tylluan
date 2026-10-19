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
  label: string;
  initial: string;
}

const AGENTS: Record<string, AgentStyle> = {
  jose:        { color: 'text-emerald-700 dark:text-emerald-300', bg: 'bg-emerald-100/70 dark:bg-emerald-950/50', ring: 'ring-emerald-500/40', border: 'border-emerald-300/80 dark:border-emerald-500/30', label: 'Jose',         initial: 'J' },
  'claude-code': { color: 'text-blue-700 dark:text-blue-300',   bg: 'bg-blue-100/70 dark:bg-blue-950/50',       ring: 'ring-blue-500/40',    border: 'border-blue-300/80 dark:border-blue-500/30',       label: 'Claude',       initial: 'C' },
  mimo:        { color: 'text-teal-700 dark:text-teal-300',     bg: 'bg-teal-100/70 dark:bg-teal-950/50',       ring: 'ring-teal-500/40',    border: 'border-teal-300/80 dark:border-teal-500/30',       label: 'Mimo',         initial: 'M' },
  deep:        { color: 'text-cyan-700 dark:text-cyan-300',     bg: 'bg-cyan-100/70 dark:bg-cyan-950/50',       ring: 'ring-cyan-500/40',    border: 'border-cyan-300/80 dark:border-cyan-500/30',       label: 'Deep',         initial: 'D' },
  opencode:    { color: 'text-amber-700 dark:text-amber-300',   bg: 'bg-amber-100/70 dark:bg-amber-950/50',     ring: 'ring-amber-500/40',   border: 'border-amber-300/80 dark:border-amber-500/30',     label: 'OpenCode',     initial: 'O' },
  antigravity: { color: 'text-violet-700 dark:text-violet-300', bg: 'bg-violet-100/70 dark:bg-violet-950/50',   ring: 'ring-violet-500/40',  border: 'border-violet-300/80 dark:border-violet-500/30',   label: 'Antigravity',  initial: 'A' },
  buffy:             { color: 'text-rose-700 dark:text-rose-300',     bg: 'bg-rose-100/70 dark:bg-rose-950/50',       ring: 'ring-rose-500/40',    border: 'border-rose-300/80 dark:border-rose-500/30',       label: 'Buffy',        initial: 'B' },
  qwen:              { color: 'text-orange-700 dark:text-orange-300', bg: 'bg-orange-100/70 dark:bg-orange-950/50',   ring: 'ring-orange-500/40',  border: 'border-orange-300/80 dark:border-orange-500/30',   label: 'Qwen',         initial: 'Q' },
  dsh:               { color: 'text-indigo-700 dark:text-indigo-300', bg: 'bg-indigo-100/70 dark:bg-indigo-950/50', ring: 'ring-indigo-500/40', border: 'border-indigo-300/80 dark:border-indigo-500/30', label: 'DSH',         initial: 'D' },
  'agent-as-system': { color: 'text-slate-700 dark:text-slate-300',   bg: 'bg-slate-200/70 dark:bg-slate-800/50',     ring: 'ring-slate-500/40',   border: 'border-slate-300/80 dark:border-slate-600/30',     label: 'System',      initial: 'S' },
  system:            { color: 'text-slate-700 dark:text-slate-300',   bg: 'bg-slate-200/70 dark:bg-slate-800/50',     ring: 'ring-slate-500/40',   border: 'border-slate-300/80 dark:border-slate-600/30',     label: 'System',      initial: 'S' },
  kernel:            { color: 'text-slate-700 dark:text-slate-300',   bg: 'bg-slate-200/70 dark:bg-slate-800/50',     ring: 'ring-slate-500/30',   border: 'border-slate-300/80 dark:border-slate-600/30',     label: 'Kernel',      initial: 'K' },
};

const FALLBACK: AgentStyle = {
  color: 'text-slate-700 dark:text-slate-300',
  bg: 'bg-slate-200/70 dark:bg-slate-800/50',
  ring: 'ring-slate-600/30',
  border: 'border-slate-300/80 dark:border-slate-600/30',
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
