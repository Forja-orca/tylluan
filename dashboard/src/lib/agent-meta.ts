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
  jose:        { color: 'text-emerald-300', bg: 'bg-emerald-950/50', ring: 'ring-emerald-500/40', border: 'border-emerald-500/30', label: 'Jose',         initial: 'J' },
  'claude-code': { color: 'text-blue-300',  bg: 'bg-blue-950/50',    ring: 'ring-blue-500/40',    border: 'border-blue-500/30',    label: 'Claude',       initial: 'C' },
  mimo:        { color: 'text-teal-300',    bg: 'bg-teal-950/50',    ring: 'ring-teal-500/40',    border: 'border-teal-500/30',    label: 'Mimo',         initial: 'M' },
  deep:        { color: 'text-cyan-300',    bg: 'bg-cyan-950/50',    ring: 'ring-cyan-500/40',    border: 'border-cyan-500/30',    label: 'Deep',         initial: 'D' },
  opencode:    { color: 'text-amber-300',   bg: 'bg-amber-950/50',   ring: 'ring-amber-500/40',   border: 'border-amber-500/30',   label: 'OpenCode',     initial: 'O' },
  antigravity: { color: 'text-violet-300',  bg: 'bg-violet-950/50',  ring: 'ring-violet-500/40',  border: 'border-violet-500/30',  label: 'Antigravity',  initial: 'A' },
  qwen:        { color: 'text-orange-300',  bg: 'bg-orange-950/50',  ring: 'ring-orange-500/40',  border: 'border-orange-500/30',  label: 'Qwen',         initial: 'Q' },
  kernel:      { color: 'text-slate-300',   bg: 'bg-slate-800/50',   ring: 'ring-slate-500/30',   border: 'border-slate-600/30',   label: 'Kernel',       initial: 'K' },
};

const FALLBACK: AgentStyle = {
  color: 'text-slate-300',
  bg: 'bg-slate-800/50',
  ring: 'ring-slate-600/30',
  border: 'border-slate-600/30',
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
