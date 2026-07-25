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

export function ProvenanceBadge({ provenance = 'unverified', className = '', showIcon = true }: ProvenanceBadgeProps) {
  let label = 'Desconocido';
  let colorClass = 'bg-slate-800/80 text-slate-400 border-slate-700/60';
  let Icon = HelpCircle;

  switch (provenance) {
    case 'user_direct':
      label = 'Usuario Directo';
      colorClass = 'bg-sky-500/10 text-sky-400 border-sky-500/30';
      Icon = User;
      break;
    case 'agent_generated':
      label = 'Agente IA';
      colorClass = 'bg-purple-500/10 text-purple-400 border-purple-500/30';
      Icon = Cpu;
      break;
    case 'federation_peer':
      label = 'Federación P2P';
      colorClass = 'bg-amber-500/10 text-amber-400 border-amber-500/30';
      Icon = Network;
      break;
    case 'guild_output':
      label = 'Guild Output';
      colorClass = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
      Icon = Terminal;
      break;
    case 'unverified':
      label = 'No Verificado';
      colorClass = 'bg-rose-500/10 text-rose-400 border-rose-500/30';
      Icon = HelpCircle;
      break;
    default:
      if (provenance.startsWith('consolidated')) {
        label = 'Consolidado';
        colorClass = 'bg-teal-500/10 text-teal-400 border-teal-500/30';
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
