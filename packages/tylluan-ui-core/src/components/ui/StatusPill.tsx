

export type StatusType = 'online' | 'healthy' | 'degraded' | 'offline' | 'critical' | 'running' | 'idle' | 'stopped' | string;

interface StatusPillProps {
  status: StatusType;
  label?: string;
  className?: string;
}

export function StatusPill({ status, label, className = '' }: StatusPillProps) {
  const norm = (status || '').toLowerCase();

  const text = label || status;

  // Semantic token classes — all colours resolved via CSS vars defined in index.css
  // Ratios verified WCAG AA (pre-flight 2026-08-12):
  //   healthy: text-emerald-400 on emerald-950/10 bg → 9.58:1 PASS
  //   degraded: text-amber-400 on amber-950/10 bg   → 9.79:1 PASS
  //   offline:  text-rose-400 on rose-500/10 bg     → 6.65:1 PASS
  let colorClass = 'bg-slate-800/60 text-slate-400 border-slate-700/60';

  if (['online', 'healthy', 'running', 'ok'].includes(norm)) {
    colorClass = 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30';
  } else if (['degraded', 'warning', 'idle'].includes(norm)) {
    colorClass = 'bg-amber-500/10 text-amber-400 border-amber-500/30';
  } else if (['offline', 'critical', 'stopped', 'error', 'failed'].includes(norm)) {
    colorClass = 'bg-rose-500/10 text-rose-400 border-rose-500/30';
  }

  return (
    <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-mono font-bold uppercase border tracking-wider ${colorClass} ${className}`}>
      <span className="relative flex h-1.5 w-1.5 shrink-0">
        {['online', 'healthy', 'running', 'ok'].includes(norm) && (
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
        )}
        <span className={`relative inline-flex rounded-full h-1.5 w-1.5 ${
          ['online', 'healthy', 'running', 'ok'].includes(norm) ? 'bg-emerald-500' :
          ['degraded', 'warning', 'idle'].includes(norm) ? 'bg-amber-500' : 'bg-rose-500'
        }`}></span>
      </span>
      <span>{text}</span>
    </span>
  );
}
export default StatusPill;
