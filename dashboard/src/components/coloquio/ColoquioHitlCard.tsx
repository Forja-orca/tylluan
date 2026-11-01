import { Check, ListChecks, Loader2, Network } from 'lucide-react';
import { useState } from 'react';

interface ColoquioHitlCardProps {
  planTurn: number;
  isApproved: boolean;
  tasks: string[];
  onVisualize: () => void;
  onApprove: () => Promise<void>;
}

export function ColoquioHitlCard({
  planTurn,
  isApproved,
  tasks,
  onVisualize,
  onApprove,
}: ColoquioHitlCardProps) {
  const [submitting, setSubmitting] = useState(false);
  const [optimisticApproved, setOptimisticApproved] = useState(false);
  const effectiveApproved = isApproved || optimisticApproved;

  const handleApproveClick = async () => {
    if (submitting || effectiveApproved) return;
    setSubmitting(true);
    try {
      await onApprove();
      setOptimisticApproved(true);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className={
        effectiveApproved
          ? 'mt-3 p-2.5 rounded-md border border-emerald-500/30 border-l-2 border-l-emerald-500 bg-emerald-500/5'
          : 'mt-3 p-2.5 rounded-md border border-amber-500/35 border-l-2 border-l-amber-500 bg-amber-500/5'
      }
      role="region"
      aria-label={`Human-in-the-loop plan #${planTurn}`}
    >
      {tasks.length > 0 && (
        <div className="mb-2.5 pb-2 border-b border-border/70">
          <div className="flex items-center gap-1.5 text-[11px] font-semibold text-foreground mb-1.5">
            <ListChecks className="w-3.5 h-3.5 text-primary" />
            <span>Pasos del plan ({tasks.length})</span>
          </div>
          <ul className="space-y-1 text-[11px] font-mono text-muted-foreground">
            {tasks.slice(0, 6).map((t, idx) => (
              <li key={`${idx}-${t}`} className="flex items-center gap-1.5 truncate">
                <span className="w-1.5 h-1.5 rounded-full bg-primary/70 shrink-0" />
                <span className="truncate">{t}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {!effectiveApproved ? (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="text-[11px] text-amber-700 dark:text-amber-300 font-semibold flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-500 animate-ping" />
            Plan awaiting human approval
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onVisualize}
              className="px-3 py-1 bg-primary hover:bg-primary/90 text-primary-foreground font-semibold text-[11px] rounded border border-primary transition-all flex items-center gap-1 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              <Network className="w-3 h-3" /> Visualize on Whiteboard
            </button>
            <button
              type="button"
              disabled={submitting}
              onClick={handleApproveClick}
              className="px-3 py-1 bg-amber-500 hover:bg-amber-400 disabled:opacity-50 text-slate-950 font-semibold text-[11px] rounded border border-amber-600 transition-all flex items-center gap-1 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {submitting ? (
                <Loader2 className="w-3 h-3 animate-spin" />
              ) : (
                <Check className="w-3 h-3 stroke-[3]" />
              )}
              {submitting ? 'Approving...' : 'Approve & Start'}
            </button>
          </div>
        </div>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-[11px] text-emerald-700 dark:text-emerald-300 font-semibold">
            <Check className="w-3.5 h-3.5 stroke-[3]" />
            Plan approved by human
          </div>
          <button
            type="button"
            onClick={onVisualize}
            className="px-3 py-1 bg-primary/15 hover:bg-primary/25 text-primary font-semibold text-[11px] rounded transition-all flex items-center gap-1 border border-primary/30 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Network className="w-3 h-3" /> Visualize on Whiteboard
          </button>
        </div>
      )}
    </div>
  );
}
