import React from 'react';
import type { LifecycleState } from '../../lib/api-client';

interface LifecycleBadgeProps {
  state?: LifecycleState | string;
  className?: string;
}

export function LifecycleBadge({ state, className = '' }: LifecycleBadgeProps) {
  const norm = (state || 'active').toLowerCase() as LifecycleState;

  let badgeStyle = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
  let label = 'ACTIVE';

  if (norm === 'quiet') {
    badgeStyle = 'bg-cyan-500/10 text-cyan-400 border-cyan-500/30';
    label = 'QUIET';
  } else if (norm === 'consolidated') {
    badgeStyle = 'bg-indigo-500/10 text-indigo-400 border-indigo-500/30';
    label = 'CONSOLIDATED';
  } else if (norm === 'archived') {
    badgeStyle = 'bg-slate-800/60 text-slate-400 border-slate-700/60';
    label = 'ARCHIVED';
  }

  return (
    <span className={`inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-mono font-bold uppercase border tracking-wider ${badgeStyle} ${className}`}>
      {label}
    </span>
  );
}

export default LifecycleBadge;
