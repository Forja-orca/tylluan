import { ArrowRight, CheckCircle2, Radio, Sparkles } from 'lucide-react';
import { AgUiHandoff } from '../../lib/ag-ui-parser';
import { agentStyle } from '../../lib/agent-meta';
import { cn } from '../../lib/utils';

interface ColoquioHandoffBadgeProps {
  handoff: AgUiHandoff;
}

export function ColoquioHandoffBadge({ handoff }: ColoquioHandoffBadgeProps) {
  const isDelivery = handoff.kind === 'delivery' || handoff.kind === 'completed';
  const isBroadcast = handoff.kind === 'broadcast';

  return (
    <div
      className={cn(
        'flex flex-wrap items-center justify-between gap-2 px-2.5 py-1.5 mb-2 rounded-md border text-[11px] font-medium',
        isDelivery
          ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-700 dark:text-emerald-300'
          : isBroadcast
          ? 'bg-primary/10 border-primary/30 text-primary'
          : 'bg-cyan-500/10 border-cyan-500/30 text-cyan-700 dark:text-cyan-300'
      )}
      role="status"
      aria-label={`Estado de coordinación: ${handoff.label}`}
    >
      <div className="flex flex-wrap items-center gap-1.5 min-w-0">
        {isDelivery ? (
          <CheckCircle2 className="w-3.5 h-3.5 shrink-0 text-emerald-600 dark:text-emerald-400" />
        ) : isBroadcast ? (
          <Sparkles className="w-3.5 h-3.5 shrink-0 text-primary" />
        ) : (
          <Radio className="w-3.5 h-3.5 shrink-0 text-cyan-600 dark:text-cyan-400" />
        )}

        <span className="font-mono font-bold tracking-tight uppercase">
          {handoff.label}
        </span>

        {handoff.taskId && (
          <span className="px-1.5 py-0.5 rounded bg-background/80 border border-border text-foreground font-mono font-bold text-[10.5px]">
            {handoff.taskId}
          </span>
        )}

        {handoff.toAgents.length > 0 && (
          <div className="flex items-center gap-1 text-muted-foreground">
            <ArrowRight className="w-3 h-3 shrink-0" />
            <div className="flex flex-wrap items-center gap-1">
              {handoff.toAgents.map(target => {
                const st = agentStyle(target);
                return (
                  <span
                    key={target}
                    className={cn(
                      'px-1.5 py-0.2 rounded border text-[10.5px] font-mono font-semibold',
                      st.bg,
                      st.color,
                      st.border
                    )}
                  >
                    @{target}
                  </span>
                );
              })}
            </div>
          </div>
        )}
      </div>

      {handoff.deliveredTag && (
        <span className="px-1.5 py-0.5 rounded bg-emerald-500/15 border border-emerald-500/30 text-emerald-700 dark:text-emerald-300 font-mono text-[10px] font-semibold shrink-0">
          T-Delivered
        </span>
      )}
    </div>
  );
}
