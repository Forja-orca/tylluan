import { cn } from '../lib/utils';
import { ColoquioChannel } from './coloquio-types';
import { agentStyle } from '../lib/agent-meta';

function fmtRel(u: number): string {
  const d = Math.floor(Date.now() / 1000 - u);
  if (d < 60) return 'just now';
  if (d < 3600) return `${Math.floor(d / 60)}m ago`;
  if (d < 86400) return `${Math.floor(d / 3600)}h ago`;
  return new Date(u * 1000).toLocaleDateString('en-US', { day: 'numeric', month: 'short' });
}

interface ColoquioAgentsPanelProps {
  agentPresence: { id: string; lastSeen: number; status: 'online' | 'idle' | 'offline' }[];
  typingStatuses: Record<string, { ts: number; status: string }>;
  selectedChannel: ColoquioChannel | undefined;
}

export function ColoquioAgentsPanel({
  agentPresence,
  typingStatuses,
  selectedChannel
}: ColoquioAgentsPanelProps) {
  return (
    <div className="w-44 shrink-0 border-l border-border bg-card flex flex-col overflow-hidden font-sans">
      <div className="px-3 py-2 border-b border-border">
        <span className="text-xs font-bold text-foreground">Agents</span>
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {agentPresence.length === 0 ? (
          <div className="px-4 py-6 text-center text-xs text-muted-foreground italic">No activity</div>
        ) : (
          agentPresence.map(({ id, lastSeen, status }) => {
            const m = agentStyle(id);
            return (
              <div key={id} className="flex items-center gap-2 px-3 py-1.5">
                <div className="relative shrink-0">
                  <div className={cn('w-6 h-6 rounded flex items-center justify-center text-[11px] font-bold border', m.bg, m.color, m.border)}>{m.initial}</div>
                  <div className={cn('absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full border border-background',
                    status === 'online' ? 'bg-emerald-600' : status === 'idle' ? 'bg-amber-600' : 'bg-muted-foreground')} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className={cn('text-xs font-semibold truncate', m.color)}>{id}</div>
                  <div className="text-[11px] text-muted-foreground truncate">
                    {typingStatuses[id] ? (
                      <span className="text-primary font-medium italic animate-pulse">{typingStatuses[id].status}</span>
                    ) : (
                      fmtRel(lastSeen)
                    )}
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
      {selectedChannel && (
        <div className="border-t border-border p-3 space-y-1.5">
          <div className="text-xs font-bold text-foreground mb-1">Channel</div>
          {([['Messages', selectedChannel.message_count], ['Last turn', `#${selectedChannel.last_turn}`], ['Participants', agentPresence.length]] as [string, string | number][]).map(([k, v]) => (
            <div key={k} className="flex justify-between text-xs">
              <span className="text-muted-foreground">{k}</span>
              <span className="text-foreground font-mono font-semibold">{v}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
