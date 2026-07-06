import { cn } from '../lib/utils';
import { ColoquioChannel } from './coloquio-types';

interface AgentMeta {
  color: string;
  bg: string;
  border: string;
  initial: string;
}

const AGENT_META: Record<string, AgentMeta> = {
  antigravity: { color: 'text-violet-300', bg: 'bg-violet-950/40', border: 'border-violet-500/30', initial: 'A' },
  'claude-code': { color: 'text-blue-300', bg: 'bg-blue-950/40', border: 'border-blue-500/30', initial: 'C' },
  opencode: { color: 'text-amber-300', bg: 'bg-amber-950/40', border: 'border-indigo-500/30', initial: 'O' },
  qwen: { color: 'text-orange-300', bg: 'bg-orange-950/40', border: 'border-orange-500/30', initial: 'Q' },
  jose: { color: 'text-emerald-300', bg: 'bg-emerald-950/40', border: 'border-emerald-500/30', initial: 'J' },
};

const DA = { color: 'text-slate-300', bg: 'bg-slate-800/60', border: 'border-slate-600/30', initial: '?' };

function agentMeta(id: string): AgentMeta {
  const k = Object.keys(AGENT_META).find(key => id.toLowerCase().includes(key));
  return k ? AGENT_META[k] : { ...DA, initial: id[0]?.toUpperCase() ?? '?' };
}

function fmtRel(u: number): string {
  const d = Math.floor(Date.now() / 1000 - u);
  if (d < 60) return 'ahora';
  if (d < 3600) return `hace ${Math.floor(d / 60)}m`;
  if (d < 86400) return `hace ${Math.floor(d / 3600)}h`;
  return new Date(u * 1000).toLocaleDateString('es', { day: 'numeric', month: 'short' });
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
    <div className="w-44 shrink-0 border-l border-slate-700/60 bg-slate-900/50 flex flex-col overflow-hidden">
      <div className="px-3 py-2.5 border-b border-slate-800/80">
        <span className="text-[10px] font-bold text-slate-500 uppercase tracking-wider">Agentes</span>
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {agentPresence.length === 0 ? (
          <div className="px-4 py-6 text-center text-[10px] text-slate-700 italic">Sin actividad</div>
        ) : (
          agentPresence.map(({ id, lastSeen, status }) => {
            const m = agentMeta(id);
            return (
              <div key={id} className="flex items-center gap-2 px-3 py-1.5">
                <div className="relative shrink-0">
                  <div className={cn('w-6 h-6 rounded-full flex items-center justify-center text-[10px] font-bold border', m.bg, m.color, m.border)}>{m.initial}</div>
                  <div className={cn('absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full border border-slate-900',
                    status === 'online' ? 'bg-emerald-500' : status === 'idle' ? 'bg-amber-500' : 'bg-slate-600')} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className={cn('text-[10px] font-semibold truncate', m.color)}>{id}</div>
                  <div className="text-[9px] text-slate-600 truncate">
                    {typingStatuses[id] ? (
                      <span className="text-indigo-400 font-medium italic animate-pulse">{typingStatuses[id].status}</span>
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
        <div className="border-t border-slate-800/80 p-3 space-y-1.5">
          <div className="text-[9px] font-bold text-slate-600 uppercase tracking-wider mb-1">Canal</div>
          {([['Mensajes', selectedChannel.message_count], ['Último turno', `#${selectedChannel.last_turn}`], ['Participantes', agentPresence.length]] as [string, string | number][]).map(([k, v]) => (
            <div key={k} className="flex justify-between text-[10px]">
              <span className="text-slate-600">{k}</span>
              <span className="text-slate-400 font-mono">{v}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
