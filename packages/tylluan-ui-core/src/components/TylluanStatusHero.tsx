import type { NexusEvent, BlackboardData } from '../lib/api-client';

interface Props {
  events: NexusEvent[];
  blackboard: BlackboardData | null;
  sysStatus: any | null;
}

/**
 * Hero block: "Tylluan está [estado]" — single source of truth for both
 * dashboard/ and ui/. Derives state from real kernel data, never hardcoded.
 */
export function TylluanStatusHero({ events, blackboard, sysStatus }: Props) {
  const activeAgentCount = blackboard?.active_agents?.length || 0;
  const recentEvents = events.filter(e => (Date.now() / 1000 - e.ts) < 300);
  const recentToolCalls = recentEvents.filter(e => e.type === 'tool_call');
  const recentMemoryOps = recentEvents.filter(e => e.type.startsWith('memory'));
  const isNightMode = sysStatus?.night_consolidation_active || false;

  let status: { text: string; subtext: string; color: string; pulse: boolean };
  if (isNightMode) {
    status = {
      text: 'Consolidando memoria nocturna',
      subtext: 'Tylluan está reorganizando y consolidando conocimiento acumulado.',
      color: 'bg-violet-500/10 border-violet-500/30 text-violet-300',
      pulse: true,
    };
  } else if (activeAgentCount > 0 && recentToolCalls.length > 0) {
    status = {
      text: `Trabajando con ${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''}`,
      subtext: `${recentToolCalls.length} llamadas a herramientas en los últimos 5 minutos.`,
      color: 'bg-emerald-500/10 border-emerald-500/30 text-emerald-300',
      pulse: true,
    };
  } else if (recentMemoryOps.length > 0) {
    status = {
      text: 'Procesando memoria',
      subtext: `${recentMemoryOps.length} operaciones de memoria recientes.`,
      color: 'bg-blue-500/10 border-blue-500/30 text-blue-300',
      pulse: true,
    };
  } else if (activeAgentCount > 0) {
    status = {
      text: `${activeAgentCount} agente${activeAgentCount > 1 ? 's' : ''} conectado${activeAgentCount > 1 ? 's' : ''}`,
      subtext: 'Esperando instrucciones.',
      color: 'bg-amber-500/10 border-amber-500/30 text-amber-300',
      pulse: false,
    };
  } else {
    status = {
      text: 'Inactivo',
      subtext: 'Sin agentes conectados. Tylluan está en standby.',
      color: 'bg-slate-500/10 border-slate-500/30 text-slate-400',
      pulse: false,
    };
  }

  return (
    <div className={`rounded-xl border p-4 ${status.color} transition-all duration-500`}>
      <div className="flex items-center gap-3">
        <div className={`w-3 h-3 rounded-full ${status.pulse ? 'bg-emerald-400 animate-pulse' : 'bg-slate-500'}`} />
        <div>
          <h3 className="text-sm font-semibold">Tylluan está {status.text.toLowerCase()}</h3>
          <p className="text-[11px] opacity-70 mt-0.5">{status.subtext}</p>
        </div>
      </div>
    </div>
  );
}
