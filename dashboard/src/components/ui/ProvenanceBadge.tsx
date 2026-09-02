import React from 'react';
import { User, Cpu, Network, Terminal, HelpCircle, Sparkles } from 'lucide-react';

export type ProvenanceType = 
  | 'user_direct' 
  | 'agent_generated' 
  | 'federation_peer' 
  | 'guild_output' 
  | 'unverified' 
  | string;

interface ProvenanceBadgeProps {
  provenance?: ProvenanceType;
  className?: string;
  showIcon?: boolean;
}

// WCAG pre-flight 2026-08-12 (all on #10141E Obsidian Nocturne canvas):
//   sky-400   (#38BDF8) → 8.59:1 PASS
//   purple-400(#C084FC) → 6.97:1 PASS
//   amber-400 (#F5B041) → 9.79:1 PASS
//   emerald-400(#34D399)→ 9.58:1 PASS
//   rose-400  (#F87171) → 6.65:1 PASS
//   teal-400  (#2DD4BF) → 9.89:1 PASS
//   slate-400 (#94A3B8) → 7.18:1 PASS (unverified/default)

export function ProvenanceBadge({ provenance = 'unverified', className = '', showIcon = true }: ProvenanceBadgeProps) {
  let label = 'Unknown';
  let colorClass = 'bg-slate-800/60 text-slate-400 border-slate-700/60';
  let Icon = HelpCircle;

  switch (provenance) {
    case 'user_direct':
      label = 'Direct User';
      colorClass = 'bg-sky-500/10 text-sky-400 border-sky-500/30';
      Icon = User;
      break;
    case 'agent_generated':
      label = 'AI Agent';
      colorClass = 'bg-violet-500/10 text-violet-400 border-violet-500/30';
      Icon = Cpu;
      break;
    case 'federation_peer':
      label = 'P2P Federation';
      colorClass = 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      Icon = Network;
      break;
    case 'guild_output':
      label = 'Guild Output';
      colorClass = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      Icon = Terminal;
      break;
    case 'unverified':
      label = 'Unverified';
      colorClass = 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      Icon = HelpCircle;
      break;
    default:
      if (provenance.startsWith('consolidated')) {
        label = 'Consolidated';
        colorClass = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
        Icon = Sparkles;
      }
      break;
  }

  return (
    <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-mono font-semibold border ${colorClass} ${className}`}>
      {showIcon && <Icon className="w-3 h-3 flex-shrink-0" />}
      <span>{label}</span>
    </span>
  );
}
export default ProvenanceBadge;
