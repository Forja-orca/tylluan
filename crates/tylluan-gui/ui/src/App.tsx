import { useState, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  LayoutDashboard, Cpu, Database, MessageSquare, Shield,
  Activity, Settings, FlaskConical, Users, ChevronRight, RefreshCw,
} from 'lucide-react';
import { NexusProvider, useNexus, TylluanStatusHero, ModelsLocalInference, StatusPill, MemoryOverview, GuildsOverview } from '@tylluan/ui-core';
import type { BlackboardData } from '@tylluan/ui-core';
import './App.css';

/* ── Navigation definition ─────────────────────────────────────── */

type ViewId = 'overview' | 'models' | 'memory' | 'coliloquio' | 'guilds' | 'federation' | 'lab' | 'settings';

interface NavItem {
  id: ViewId;
  label: string;
  icon: React.ElementType;
  ready: boolean;
}

const NAV_ITEMS: NavItem[] = [
  { id: 'overview',    label: 'Overview',    icon: LayoutDashboard, ready: true },
  { id: 'models',      label: 'Modelos',     icon: Cpu,             ready: true },
  { id: 'memory',      label: 'Memoria',     icon: Database,        ready: false },
  { id: 'coliloquio',  label: 'Coloquio',    icon: MessageSquare,   ready: false },
  { id: 'guilds',      label: 'Guilds',      icon: Users,           ready: false },
  { id: 'federation',  label: 'Federación',  icon: Shield,          ready: false },
  { id: 'lab',         label: 'Lab',         icon: FlaskConical,    ready: false },
  { id: 'settings',    label: 'Settings',     icon: Settings,        ready: false },
];

/* ── Placeholder for not-yet-built views ──────────────────────── */

function PlaceholderView({ label }: { label: string }) {
  return (
    <div className="flex-1 flex items-center justify-center">
      <div className="text-center space-y-3">
        <div className="w-16 h-16 mx-auto rounded-2xl bg-white/5 border border-white/10 flex items-center justify-center">
          <Activity className="w-8 h-8 text-white/20" />
        </div>
        <h2 className="text-lg font-semibold text-white/60">{label}</h2>
        <p className="text-sm text-white/30 max-w-xs">
          Esta vista se implementará en un próximo ciclo.
        </p>
      </div>
    </div>
  );
}

/* ── Overview Panel (real data from kernel) ────────────────────── */

function OverviewPanel() {
  const {
    bridge, events, sysStatus, online,
    goldenSignals, memoryStats, guilds, sessions,
  } = useNexus();
  const [blackboard, setBlackboard] = useState<BlackboardData | null>(null);

  useEffect(() => {
    if (!bridge) return;
    let cancelled = false;
    (async () => {
      try {
        const bb = await bridge.fetchRaw('/api/v1/blackboard') as BlackboardData;
        if (!cancelled) setBlackboard(bb);
      } catch { /* non-critical */ }
    })();
    return () => { cancelled = true; };
  }, [bridge]);

  return (
    <div className="space-y-5 animate-in fade-in duration-500">
      {/* Hero: what is Tylluan doing now */}
      <TylluanStatusHero
        events={events}
        blackboard={blackboard}
        sysStatus={sysStatus}
      />

      {/* Quick status row */}
      <div className="flex flex-wrap gap-2">
        <StatusPill status={online ? 'active' : 'error'} label="Kernel" />
        <StatusPill
          status={sysStatus?.embeddings_loaded ? 'active' : 'warning'}
          label="BGE-M3"
        />
        <StatusPill
          status={online ? 'active' : 'error'}
          label={`${blackboard?.active_agents?.length || 0} Agentes`}
        />
        <StatusPill
          status="active"
          label={`${memoryStats?.node_count || 0} Silva Nodes`}
        />
      </div>

      {/* Summary cards — using GoldenSignals from context */}
      <div className="grid grid-cols-2 lg:grid-cols-3 gap-3">
        <SummaryCard
          label="Guilds online"
          value={goldenSignals?.status?.guilds_online ?? '—'}
          color="text-sky-400"
        />
        <SummaryCard
          label="Tasa errores"
          value={goldenSignals?.errors?.rate_percent != null ? `${goldenSignals.errors.rate_percent.toFixed(1)}%` : '—'}
          color={goldenSignals?.errors?.critical ? 'text-red-400' : 'text-emerald-400'}
        />
        <SummaryCard
          label="Memoria"
          value={goldenSignals?.saturation?.memory_percent != null ? `${goldenSignals.saturation.memory_percent.toFixed(0)}%` : '—'}
          color="text-amber-400"
        />
        <SummaryCard
          label="Sesiones MCP"
          value={sessions.length}
          color="text-violet-400"
        />
        <SummaryCard
          label="Guilds"
          value={guilds.length}
          color="text-rose-400"
        />
        <SummaryCard
          label="Nodos"
          value={goldenSignals?.saturation?.node_count ?? memoryStats?.node_count ?? '—'}
          color="text-cyan-400"
        />
      </div>
    </div>
  );
}

function SummaryCard({ label, value, color }: { label: string; value: React.ReactNode; color: string }) {
  return (
    <div className="glass-card p-4 space-y-1">
      <p className="text-[11px] font-medium text-white/40 uppercase tracking-wider">{label}</p>
      <p className={`text-xl font-bold ${color}`}>{value}</p>
    </div>
  );
}

/* ── Modelos Panel (loads config from bridge) ──────────────────── */

function ModelosPanel() {
  const { bridge } = useNexus();
  const [config, setConfig] = useState<Record<string, unknown> | null>(null);
  const [models, setModels] = useState<unknown>(null);
  const [loading, setLoading] = useState(true);
  const [selectedDevice, setSelectedDevice] = useState('cpu');
  const [initialDevice, setInitialDevice] = useState('cpu');
  const [restartModal, setRestartModal] = useState(false);

  useEffect(() => {
    if (!bridge) return;
    let cancelled = false;
    (async () => {
      setLoading(true);
      try {
        const cfg = await bridge.getConfig();
        if (cancelled) return;
        setConfig(cfg);
        const dev = ((cfg?.inference?.device) || 'cpu') as string;
        setSelectedDevice(dev);
        setInitialDevice(dev);
        try {
          const m = await bridge.fetchRaw('/api/v1/models');
          if (!cancelled) setModels(m);
        } catch { /* models optional */ }
      } catch (e) {
        console.error('Failed to load config/models', e);
      }
      if (!cancelled) setLoading(false);
    })();
    return () => { cancelled = true; };
  }, [bridge]);

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <RefreshCw className="w-6 h-6 animate-spin text-white/30" />
      </div>
    );
  }

  return (
    <div className="space-y-4 animate-in fade-in duration-500">
      <ModelsLocalInference
        bridge={bridge}
        config={config}
        models={models}
        selectedDevice={selectedDevice}
        initialDevice={initialDevice}
        onDeviceChange={setSelectedDevice}
        onRestartRequired={() => { setInitialDevice(selectedDevice); setRestartModal(true); }}
      />

      {/* Restart modal */}
      {restartModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
          <div className="glass-card p-6 max-w-sm space-y-4">
            <h3 className="text-lg font-bold text-white">Reinicio necesario</h3>
            <p className="text-sm text-white/60">
              El cambio de device requiere reiniciar el kernel para tomar efecto.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setRestartModal(false)}
                className="px-4 py-2 text-sm text-white/50 hover:text-white/80 transition-colors"
              >
                Cancelar
              </button>
              <button
                type="button"
                onClick={() => {
                  bridge?.fetchRaw('/api/v1/maintenance/restart', { method: 'POST' });
                  setRestartModal(false);
                }}
                className="px-4 py-2 text-sm bg-violet-500/20 text-violet-300 border border-violet-500/30 rounded-lg hover:bg-violet-500/30 transition-colors"
              >
                Reiniciar kernel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ── Memory Panel ─────────────────────────────────────────────── */

function MemoryPanel() {
  const { bridge, memoryStats } = useNexus();
  const [notifyMsg, setNotifyMsg] = useState<{ msg: string; type: 'info' | 'error' } | null>(null);

  return (
    <div className="space-y-4 animate-in fade-in duration-500">
      <MemoryOverview
        bridge={bridge}
        memoryStats={memoryStats}
        notify={(msg, type) => { setNotifyMsg({ msg, type: type || 'info' }); setTimeout(() => setNotifyMsg(null), 3000); }}
      />
      {notifyMsg && (
        <div className={`fixed bottom-4 right-4 px-4 py-2 rounded-lg text-sm z-50 ${
          notifyMsg.type === 'error' ? 'bg-red-500/20 text-red-300 border border-red-500/30' : 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
        }`}>
          {notifyMsg.msg}
        </div>
      )}
    </div>
  );
}

/* ── Guilds Panel ─────────────────────────────────────────────── */

function GuildsPanel() {
  const { bridge, guilds, refreshData } = useNexus();
  const [notifyMsg, setNotifyMsg] = useState<{ msg: string; type: 'info' | 'error' } | null>(null);

  return (
    <div className="space-y-4 animate-in fade-in duration-500">
      <GuildsOverview
        bridge={bridge}
        guilds={guilds}
        onRefresh={refreshData}
        notify={(msg, type) => { setNotifyMsg({ msg, type: type || 'info' }); setTimeout(() => setNotifyMsg(null), 3000); }}
      />
      {notifyMsg && (
        <div className={`fixed bottom-4 right-4 px-4 py-2 rounded-lg text-sm z-50 ${
          notifyMsg.type === 'error' ? 'bg-red-500/20 text-red-300 border border-red-500/30' : 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
        }`}>
          {notifyMsg.msg}
        </div>
      )}
    </div>
  );
}

/* ── Main Shell ────────────────────────────────────────────────── */

function Shell() {
  const [activeView, setActiveView] = useState<ViewId>('overview');
  const { online } = useNexus();
  const current = NAV_ITEMS.find(n => n.id === activeView)!;

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      {/* Sidebar */}
      <nav className="w-16 md:w-20 glass-panel !rounded-none border-y-0 border-l-0 flex flex-col items-center py-6 gap-2 z-50 shrink-0">
        {/* Logo */}
        <div className="w-10 h-10 bg-violet-500/20 rounded-xl flex items-center justify-center border border-violet-500/30 mb-4">
          <Shield className="text-violet-400" size={22} />
        </div>

        {/* Nav icons */}
        <div className="flex-1 flex flex-col gap-1">
          {NAV_ITEMS.map(item => {
            const Icon = item.icon;
            const isActive = activeView === item.id;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => item.ready && setActiveView(item.id)}
                title={item.label}
                className={`group relative p-3 rounded-xl transition-all duration-200 ${
                  isActive
                    ? 'bg-violet-500/15 text-violet-400 border border-violet-500/25'
                    : item.ready
                      ? 'text-white/35 hover:bg-white/5 hover:text-white/70'
                      : 'text-white/15 cursor-default'
                }`}
              >
                <Icon size={20} />
                {/* Tooltip */}
                <span className="absolute left-full ml-3 px-2.5 py-1 bg-slate-900 text-white text-[10px] rounded-lg opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none uppercase tracking-wider whitespace-nowrap z-50 border border-white/10">
                  {item.label}{!item.ready ? ' — Próximamente' : ''}
                </span>
                {/* Active indicator */}
                {isActive && (
                  <div className="absolute -left-1 top-1/2 -translate-y-1/2 w-1 h-5 bg-violet-400 rounded-full shadow-[0_0_8px_rgba(139,92,246,0.6)]" />
                )}
              </button>
            );
          })}
        </div>

        {/* Kernel status dot */}
        <div className="flex items-center gap-2 px-2">
          <div className={`w-2 h-2 rounded-full ${online ? 'bg-emerald-500 shadow-[0_0_6px_rgba(34,197,94,0.6)]' : 'bg-red-500'}`} />
        </div>
      </nav>

      {/* Main area */}
      <main className="flex-1 flex flex-col h-full min-w-0">
        {/* Header */}
        <header className="h-14 px-6 flex items-center justify-between border-b border-white/5 bg-black/20 shrink-0 z-40">
          <div className="flex items-center gap-2.5">
            <span className="text-violet-400/50 font-mono text-xs">TYLLUAN</span>
            <ChevronRight size={12} className="text-white/15" />
            <span className="font-semibold text-sm text-white/80">{current.label}</span>
          </div>
          <div className="flex items-center gap-4">
            <span className="text-[10px] font-mono text-white/25">:4000</span>
            <StatusPill status={online ? 'active' : 'error'} label={online ? 'Online' : 'Offline'} />
          </div>
        </header>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          <AnimatePresence mode="wait">
            <motion.div
              key={activeView}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={{ duration: 0.2 }}
              className="h-full"
            >
              {activeView === 'overview' && <OverviewPanel />}
              {activeView === 'models' && <ModelosPanel />}
              {activeView === 'memory' && <MemoryPanel />}
              {activeView === 'guilds' && <GuildsPanel />}
              {!['overview', 'models', 'memory', 'guilds'].includes(activeView) && (
                <PlaceholderView label={current.label} />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      </main>
    </div>
  );
}

/* ── App root ──────────────────────────────────────────────────── */

export default function App() {
  return (
    <NexusProvider>
      <Shell />
    </NexusProvider>
  );
}
