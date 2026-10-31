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
    <div className="w-44 shrink-0 border-l border-[#ACA899] dark:border-border bg-[#ECE9D8] dark:bg-card flex flex-col overflow-hidden font-sans">
      <div className="px-2.5 py-1.5 border-b border-[#ACA899] dark:border-border bg-white dark:bg-muted">
        <span className="text-[11px] font-bold text-[#0A246A] dark:text-primary">Agentes en Línea</span>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {agentPresence.length === 0 ? (
          <div className="px-3 py-4 text-center text-[11px] text-[#666666] italic">Sin actividad reciente</div>
        ) : (
          agentPresence.map(({ id, lastSeen, status }) => {
            const m = agentStyle(id);
            return (
              <div key={id} className="flex items-center gap-2 px-2.5 py-1">
                <div className="relative shrink-0">
                  <div className={cn('w-5 h-5 rounded-none flex items-center justify-center text-[10px] font-bold border border-[#ACA899]', m.bg, m.color)}>{m.initial}</div>
                  <div className={cn('absolute -bottom-0.5 -right-0.5 w-1.5 h-1.5 rounded-none border border-white',
                    status === 'online' ? 'bg-[#008000]' : status === 'idle' ? 'bg-[#B8860B]' : 'bg-[#808080]')} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className={cn('text-[11px] font-bold truncate', m.color)}>{id}</div>
                  <div className="text-[10px] text-[#555555] dark:text-muted-foreground truncate">
                    {typingStatuses[id] ? (
                      <span className="text-[#0A246A] dark:text-primary font-medium italic">{typingStatuses[id].status}</span>
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
        <fieldset className="border-t border-[#ACA899] dark:border-border p-2 space-y-1 bg-[#ECE9D8] dark:bg-card rounded-none">
          <legend className="text-[10px] font-bold text-[#01285F] dark:text-muted-foreground px-1">Detalles del Canal</legend>
          {([['Mensajes', selectedChannel.message_count], ['Último turno', `#${selectedChannel.last_turn}`], ['Participantes', agentPresence.length]] as [string, string | number][]).map(([k, v]) => (
            <div key={k} className="flex justify-between text-[11px]">
              <span className="text-[#555555] dark:text-muted-foreground">{k}:</span>
              <span className="text-[#000000] dark:text-foreground font-mono font-bold">{v}</span>
            </div>
          ))}
        </fieldset>
      )}
    </div>
  );
}
