import {
  AlertTriangle,
  Brain,
  CheckCircle2,
  ChevronRight,
  Database,
  GitBranch,
  Loader2,
  Network,
  Scale,
  Terminal,
  Wrench,
} from 'lucide-react';
import { useState } from 'react';
import { AgUiToolCall } from '../../lib/ag-ui-parser';
import { cn } from '../../lib/utils';

interface ColoquioToolCardProps {
  toolCall: AgUiToolCall;
}

function getToolIcon(tool: string, guild?: string) {
  const key = (guild || tool || '').toLowerCase();
  if (key.includes('remember') || key.includes('recall') || key.includes('silva')) {
    return <Database className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400 shrink-0" />;
  }
  if (key.includes('graph')) {
    return <Network className="w-3.5 h-3.5 text-violet-600 dark:text-violet-400 shrink-0" />;
  }
  if (key.includes('think')) {
    return <Brain className="w-3.5 h-3.5 text-cyan-600 dark:text-cyan-400 shrink-0" />;
  }
  if (key.includes('decide')) {
    return <Scale className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />;
  }
  if (key.includes('git')) {
    return <GitBranch className="w-3.5 h-3.5 text-primary shrink-0" />;
  }
  if (key.includes('bash') || key.includes('shell')) {
    return <Terminal className="w-3.5 h-3.5 text-cyan-600 dark:text-cyan-400 shrink-0" />;
  }
  return <Wrench className="w-3.5 h-3.5 text-primary shrink-0" />;
}

export function ColoquioToolCard({ toolCall }: ColoquioToolCardProps) {
  const [expanded, setExpanded] = useState(false);

  const statusBadge =
    toolCall.status === 'running' ? (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/30 text-cyan-700 dark:text-cyan-300 font-mono text-[10px] font-semibold shrink-0">
        <Loader2 className="w-3 h-3 animate-spin" />
        running
      </span>
    ) : toolCall.status === 'failed' ? (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-rose-500/10 border border-rose-500/30 text-rose-700 dark:text-rose-300 font-mono text-[10px] font-semibold shrink-0">
        <AlertTriangle className="w-3 h-3" />
        failed
      </span>
    ) : (
      <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-700 dark:text-emerald-300 font-mono text-[10px] font-semibold shrink-0">
        <CheckCircle2 className="w-3 h-3" />
        passed
      </span>
    );

  const summaryText =
    toolCall.changed ||
    toolCall.performed ||
    toolCall.intent ||
    'Ejecución de herramienta completada';

  return (
    <div className="my-2 rounded-md border border-border bg-muted/35 overflow-hidden text-[11.5px]">
      <button
        type="button"
        onClick={() => setExpanded(prev => !prev)}
        aria-expanded={expanded}
        className="w-full flex items-center justify-between gap-2 px-2.5 py-1.5 text-left hover:bg-muted/70 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <div className="flex items-center gap-1.5 min-w-0 flex-1">
          <ChevronRight
            className={cn(
              'w-3.5 h-3.5 text-muted-foreground transition-transform shrink-0',
              expanded ? 'rotate-90' : ''
            )}
          />
          {getToolIcon(toolCall.tool, toolCall.guild)}
          <span className="font-mono font-bold text-foreground shrink-0">
            {toolCall.guild ? `${toolCall.guild} · ${toolCall.tool}` : toolCall.tool}
          </span>
          <span className="text-muted-foreground truncate font-mono text-[11px]">
            — {summaryText}
          </span>
        </div>
        {statusBadge}
      </button>

      {expanded && (
        <div className="px-3 py-2 border-t border-border bg-background/70 space-y-1.5 font-mono text-[11px]">
          {toolCall.intent && (
            <div>
              <span className="text-muted-foreground font-semibold">Intent: </span>
              <span className="text-foreground break-words">{toolCall.intent}</span>
            </div>
          )}
          {toolCall.performed && (
            <div>
              <span className="text-muted-foreground font-semibold">Performed: </span>
              <span className="text-foreground break-words">{toolCall.performed}</span>
            </div>
          )}
          {toolCall.changed && (
            <div>
              <span className="text-muted-foreground font-semibold">Result: </span>
              <span className="text-foreground break-words">{toolCall.changed}</span>
            </div>
          )}
          {toolCall.rawJson && (
            <pre className="mt-1.5 p-2 rounded bg-muted/60 border border-border text-[10.5px] text-muted-foreground overflow-x-auto">
              {toolCall.rawJson}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
