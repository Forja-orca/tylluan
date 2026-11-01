import { cn } from '../lib/utils';
import { ColoquioChannel } from './coloquio-types';
import { agentStyle } from '../lib/agent-meta';

function fmtRel(u: number): string {
  const d = Math.floor(Date.now() / 1000 - u);
  if (d < 60) return 'hace un instante';
  if (d < 3600) return `hace ${Math.floor(d / 60)} min`;
  if (d < 86400) return `hace ${Math.floor(d / 3600)} h`;
  return new Date(u * 1000).toLocaleDateString('es-ES', { day: 'numeric', month: 'short' });
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
    <div className="w-44 shrink-0 border-l border-border bg-muted/20 flex flex-col overflow-hidden font-sans">
      <div className="px-3 py-2 border-b border-border bg-card/60">
        <span className="text-[11px] font-bold text-primary">Agentes en Línea</span>
      </div>
      <div className="flex-1 overflow-y-auto py-1.5">
        {agentPresence.length === 0 ? (
          <div className="px-3 py-4 text-center text-[11px] text-muted-foreground italic">Sin actividad reciente</div>
        ) : (
          agentPresence.map(({ id, lastSeen, status }) => {
            const m = agentStyle(id);
            return (
              <div key={id} className="flex items-center gap-2 px-3 py-1.5 hover:bg-card/50 transition-colors">
                <div className="relative shrink-0">
                  <div className={cn('w-6 h-6 rounded-md flex items-center justify-center text-[10px] font-bold border shadow-sm', m.bg, m.color, m.border)}>{m.initial}</div>
                  <div className={cn('absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full border border-card',
                    status === 'online' ? 'bg-cyan-500 dark:bg-cyan-400' : status === 'idle' ? 'bg-amber-500' : 'bg-slate-400')} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className={cn('text-[11.5px] font-bold truncate', m.color)}>{id}</div>
                  <div className="text-[10px] text-muted-foreground truncate">
                    {typingStatuses[id] ? (
                      <span className="text-cyan-700 dark:text-cyan-300 font-medium italic">{typingStatuses[id].status}</span>
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
        <fieldset className="border-t border-border p-2.5 space-y-1 bg-card/60">
          <legend className="text-[10px] font-bold text-muted-foreground px-1">Detalles del Canal</legend>
          {([['Mensajes', selectedChannel.message_count], ['Último turno', `#${selectedChannel.last_turn}`], ['Participantes', agentPresence.length]] as [string, string | number][]).map(([k, v]) => (
            <div key={k} className="flex justify-between text-[11px]">
              <span className="text-muted-foreground">{k}:</span>
              <span className="text-foreground font-mono font-bold">{v}</span>
            </div>
          ))}
        </fieldset>
      )}
    </div>
  );
}
