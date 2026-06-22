import { useState, useEffect } from 'react';
import { Activity, Cpu, Database, Users, AlertTriangle, CheckCircle, Clock, Zap, Radio } from 'lucide-react';
import { cn } from '../lib/utils';

// ── types ─────────────────────────────────────────────────────────────────────

interface AgentActivity {
  id: string;
  lastTurn: number;
  lastChannel: string;
  lastTs: number;   // unix seconds
  totalTurns: number;
}

interface GuildStatus {
  name: string;
  online: boolean;
}

interface KernelHealth {
  uptime: number;
  cpu: number;
  memPct: number;
  guildsOnline: number;
  guildsTotal: number;
  status: string;
  version: string;
}

interface FleetData {
  agents: AgentActivity[];
  kernel: KernelHealth;
  guilds: GuildStatus[];
}

// ── helpers ───────────────────────────────────────────────────────────────────

const KNOWN_AGENTS = ['user', 'agent-1', 'agent-2', 'agent-3', 'kernel'];

const AGENT_STYLE: Record<string, { color: string; bg: string; ring: string; label: string }> = {
  'user':        { color: 'text-emerald-300', bg: 'bg-emerald-950/50', ring: 'ring-emerald-500/40', label: 'User' },
  'human':       { color: 'text-emerald-300', bg: 'bg-emerald-950/50', ring: 'ring-emerald-500/40', label: 'User' },
  'agent-1':     { color: 'text-blue-300',    bg: 'bg-blue-950/50',    ring: 'ring-blue-500/40',    label: 'Agent-1' },
  'agent-2':     { color: 'text-amber-300',   bg: 'bg-amber-950/50',   ring: 'ring-amber-500/40',   label: 'Agent-2' },
  'agent-3':     { color: 'text-violet-300',  bg: 'bg-violet-950/50',  ring: 'ring-violet-500/40',  label: 'Agent-3' },
  'kernel':      { color: 'text-slate-300',   bg: 'bg-slate-800/50',   ring: 'ring-slate-500/30',   label: 'Kernel' },
};

function styleFor(id: string) {
  const cleanId = id.toLowerCase();
  const key = Object.keys(AGENT_STYLE).find(k => cleanId.includes(k));
  if (key) return AGENT_STYLE[key];
  
  let hash = 0;
  for (let i = 0; i < cleanId.length; i++) {
    hash = cleanId.charCodeAt(i) + ((hash << 5) - hash);
  }
  const colors = [
    { color: 'text-orange-300', bg: 'bg-orange-950/50', ring: 'ring-orange-500/40' },
    { color: 'text-pink-300', bg: 'bg-pink-950/50', ring: 'ring-pink-500/40' },
    { color: 'text-indigo-300', bg: 'bg-indigo-950/50', ring: 'ring-indigo-500/40' },
    { color: 'text-teal-300', bg: 'bg-teal-950/50', ring: 'ring-teal-500/40' },
    { color: 'text-lime-300', bg: 'bg-lime-950/50', ring: 'ring-lime-500/40' },
    { color: 'text-rose-300', bg: 'bg-rose-950/50', ring: 'ring-rose-500/40' },
  ];
  const choice = colors[Math.abs(hash) % colors.length];
  return { ...choice, label: id };
}

function initial(id: string) {
  const s = styleFor(id);
  return s.label[0].toUpperCase();
}

function fmtUptime(s: number) {
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d`;
}

function fmtAgo(ts: number) {
  const d = Math.floor(Date.now() / 1000 - ts);
  if (d < 5) return 'ahora';
  if (d < 60) return `${d}s`;
  if (d < 3600) return `${Math.floor(d / 60)}m`;
  if (d < 86400) return `${Math.floor(d / 3600)}h`;
  return `${Math.floor(d / 86400)}d`;
}

function agentStatus(ts: number): 'online' | 'idle' | 'offline' {
  const d = Math.floor(Date.now() / 1000 - ts);
  if (d < 300) return 'online';
  if (d < 3600) return 'idle';
  return 'offline';
}

const STATUS_DOT: Record<string, string> = {
  online:  'bg-emerald-400',
  idle:    'bg-amber-400',
  offline: 'bg-slate-600',
};

// ── data fetching ─────────────────────────────────────────────────────────────

async function fetchFleetData(): Promise<FleetData> {
  const [summaryRes, channelsRes] = await Promise.all([
    fetch('/api/v1/dashboard/summary'),
    fetch('/api/v1/coloquio/channels'),
  ]);
  const summary = await summaryRes.json();
  const channelsData = await channelsRes.json();

  const kernel: KernelHealth = {
    uptime:       summary.system_status?.uptime_secs ?? 0,
    cpu:          Math.round(summary.system_status?.system?.cpu_usage ?? 0),
    memPct:       Math.round(summary.system_status?.system?.memory_percent ?? 0),
    guildsOnline: summary.system_status?.guilds_online ?? 0,
    guildsTotal:  summary.system_status?.guilds_total ?? 0,
    status:       summary.system_status?.status ?? 'unknown',
    version:      summary.system_status?.version ?? '',
  };

  // Fetch last messages from each channel to derive agent activity
  const channels: { channel_id: string; last_turn: number }[] = channelsData.channels ?? [];
  const threadResults = await Promise.all(
    channels.map(async ch => {
      try {
        const r = await fetch(`/api/v1/coloquio/channels/${ch.channel_id}?limit=20`);
        const d = await r.json();
        return { channel_id: ch.channel_id, messages: d.messages ?? [] };
      } catch { return { channel_id: ch.channel_id, messages: [] }; }
    })
  );

  // Aggregate per-agent: latest turn across all channels
  const agentMap: Record<string, AgentActivity> = {};
  for (const { channel_id, messages } of threadResults) {
    for (const msg of messages) {
      const id: string = msg.author_id;
      if (!id) continue;
      const existing = agentMap[id];
      const ts = typeof msg.created_at === 'number' ? msg.created_at : Math.floor(Date.now() / 1000);
      if (!existing || ts > existing.lastTs) {
        agentMap[id] = {
          id,
          lastTurn: msg.turn,
          lastChannel: channel_id,
          lastTs: ts,
          totalTurns: (existing?.totalTurns ?? 0) + 1,
        };
      } else {
        agentMap[id] = { ...existing, totalTurns: existing.totalTurns + 1 };
      }
    }
  }

  // Ensure known agents appear even if silent
  for (const id of KNOWN_AGENTS) {
    if (!agentMap[id] && id !== 'kernel') {
      agentMap[id] = { id, lastTurn: 0, lastChannel: '—', lastTs: 0, totalTurns: 0 };
    }
  }

  const agents = Object.values(agentMap).sort((a, b) => b.lastTs - a.lastTs);

  // Guild list from golden signals (online count only — no per-guild detail in this endpoint)
  const guilds: GuildStatus[] = [];

  return { agents, kernel, guilds };
}

// ── subcomponents ─────────────────────────────────────────────────────────────

function KernelCard({ k }: { k: KernelHealth }) {
  const ok = k.status === 'ok' || k.status === 'healthy';
  return (
    <div className="rounded-xl border border-slate-700/60 bg-slate-900/60 p-4 space-y-3">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Zap className="w-4 h-4 text-indigo-400" />
          <span className="text-xs font-bold text-slate-200">Kernel</span>
          <span className="text-[10px] text-slate-500 font-mono">{k.version}</span>
        </div>
        <span className={cn('text-[10px] font-bold px-2 py-0.5 rounded-full',
          ok ? 'bg-emerald-950/60 text-emerald-400' : 'bg-amber-950/60 text-amber-400')}>
          {k.status}
        </span>
      </div>
      <div className="grid grid-cols-2 gap-2">
        {([
          ['Uptime',   fmtUptime(k.uptime), Clock],
          ['CPU',      `${k.cpu}%`,          Cpu],
          ['Memoria',  `${k.memPct}%`,        Database],
          ['Guilds',   `${k.guildsOnline}/${k.guildsTotal}`, Activity],
        ] as [string, string, React.ElementType][]).map(([label, value, Icon]) => (
          <div key={label} className="flex items-center gap-2 bg-slate-800/50 rounded-lg px-3 py-2">
            <Icon className="w-3.5 h-3.5 text-slate-500 shrink-0" />
            <div>
              <div className="text-[9px] text-slate-600 uppercase tracking-wide">{label}</div>
              <div className="text-xs font-mono font-semibold text-slate-200">{value}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function AgentCard({ a }: { a: AgentActivity }) {
  const s = styleFor(a.id);
  const status = a.lastTs > 0 ? agentStatus(a.lastTs) : 'offline';
  const label = s.label;

  return (
    <div className={cn('rounded-xl border p-3 flex items-start gap-3 transition-colors', s.bg,
      status === 'online' ? 'border-emerald-500/20' : status === 'idle' ? 'border-amber-500/15' : 'border-slate-700/40')}>
      <div className="relative shrink-0 mt-0.5">
        <div className={cn('w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold ring-1', s.bg, s.color, s.ring)}>
          {initial(a.id)}
        </div>
        <div className={cn('absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full border-2 border-slate-900', STATUS_DOT[status])} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-baseline justify-between gap-1">
          <span className={cn('text-xs font-semibold truncate', s.color)}>{label}</span>
          <span className="text-[10px] text-slate-600 shrink-0">
            {a.lastTs > 0 ? fmtAgo(a.lastTs) : 'sin actividad'}
          </span>
        </div>
        <div className="text-[10px] text-slate-500 mt-0.5 truncate">
          {a.lastTs > 0 ? (
            <>T{a.lastTurn} en <span className="text-slate-400">#{a.lastChannel}</span></>
          ) : (
            <span className="italic">no ha participado</span>
          )}
        </div>
        <div className="mt-1 flex items-center gap-1.5">
          <div className="text-[9px] text-slate-600">{a.totalTurns} turnos</div>
          {status === 'online' && <span className="text-[9px] font-bold text-emerald-500 animate-pulse">● activo</span>}
          {status === 'idle'   && <span className="text-[9px] text-amber-500">● inactivo</span>}
          {status === 'offline' && a.lastTs > 0 && <span className="text-[9px] text-slate-600">● desconectado</span>}
        </div>
      </div>
    </div>
  );
}

function GuildBar({ online, total }: { online: number; total: number }) {
  const pct = total > 0 ? Math.round((online / total) * 100) : 0;
  const color = pct >= 80 ? 'bg-emerald-500' : pct >= 50 ? 'bg-amber-500' : 'bg-rose-500';
  return (
    <div className="rounded-xl border border-slate-700/60 bg-slate-900/60 p-4 space-y-2">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Radio className="w-4 h-4 text-indigo-400" />
          <span className="text-xs font-bold text-slate-200">Guilds</span>
        </div>
        <span className="text-xs font-mono text-slate-300">{online}<span className="text-slate-600">/{total}</span></span>
      </div>
      <div className="h-1.5 bg-slate-800 rounded-full overflow-hidden">
        <div className={cn('h-full rounded-full transition-all', color)} style={{ width: `${pct}%` }} />
      </div>
      <div className="grid grid-cols-3 gap-1.5 pt-1">
        {pct >= 80
          ? <div className="col-span-3 flex items-center gap-1.5 text-[10px] text-emerald-400"><CheckCircle className="w-3 h-3" /> Flota operativa</div>
          : <div className="col-span-3 flex items-center gap-1.5 text-[10px] text-amber-400"><AlertTriangle className="w-3 h-3" /> {total - online} guilds offline</div>
        }
      </div>
    </div>
  );
}

// ── main component ────────────────────────────────────────────────────────────

export function FleetTab() {
  const [data, setData] = useState<FleetData | null>(null);
  const [loading, setLoading] = useState(true);
  const [lastRefresh, setLastRefresh] = useState(0);

  const refresh = async () => {
    try {
      const d = await fetchFleetData();
      setData(d);
      setLastRefresh(Date.now());
    } catch { /* ignore */ } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 10000);
    return () => clearInterval(id);
  }, []);

  if (loading) return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-slate-600 text-sm animate-pulse">Cargando estado de la flota...</div>
    </div>
  );

  if (!data) return null;

  const onlineCount = data.agents.filter(a => agentStatus(a.lastTs) === 'online').length;

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Users className="w-4 h-4 text-indigo-400" />
          <h2 className="text-sm font-bold text-slate-200">Estado de la Flota</h2>
          <span className="text-[10px] bg-indigo-950/60 text-indigo-400 px-2 py-0.5 rounded-full font-mono">
            {onlineCount} activos
          </span>
        </div>
        <button onClick={refresh} className="text-[10px] text-slate-600 hover:text-slate-400 cursor-pointer transition-colors">
          actualizar · {lastRefresh > 0 ? fmtAgo(Math.floor(lastRefresh / 1000)) : '—'}
        </button>
      </div>

      {/* Kernel health */}
      <KernelCard k={data.kernel} />

      {/* Guild bar */}
      <GuildBar online={data.kernel.guildsOnline} total={data.kernel.guildsTotal} />

      {/* Agent grid */}
      <div>
        <div className="text-[10px] font-bold text-slate-600 uppercase tracking-widest mb-2 px-0.5">Agentes</div>
        <div className="grid grid-cols-1 gap-2">
          {data.agents.map(a => <AgentCard key={a.id} a={a} />)}
        </div>
      </div>
    </div>
  );
}
