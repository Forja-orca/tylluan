import { useState, useEffect, useCallback, useMemo } from 'react'
import {
  LayoutDashboard,
  MessageSquare,
  ShieldCheck,
  Database,
  Cpu,
  Settings,
  Beaker,
  Wifi, WifiOff,
  AlertTriangle,
  Zap,
  Radio,
  Bell,
  Sun,
  Moon,
  Monitor,
  Key,
  Loader2
} from 'lucide-react'
import { useNexus } from './hooks/useNexus'
import { useNexusSSE } from './hooks/useNexusSSE'
import { usePolling } from './hooks/usePolling'
import { useThemeManager } from './hooks/useThemeManager'
import { useHitlGrants } from './hooks/useHitlGrants'
import { useColoquioMentions } from './hooks/useColoquioMentions'
import { useSSEBridgeEvents } from './hooks/useSSEBridgeEvents'
import { useAppStore } from './stores/useAppStore'
import { cn } from './lib/utils'
import { ErrorBoundary } from './components/ErrorBoundary'
import { AuthModal } from './components/ui/AuthModal'

import React, { lazy, Suspense } from 'react'
import { OverviewConsolidated } from './components/OverviewConsolidated'
import { TylluanLogo } from './components/TylluanLogo'
import DeviceStatusBadge from './components/DeviceStatusBadge'

// Lazy-loaded Views (6 Sovereign Views + Optional Diagnostic)
const ColoquioTab = lazy(() => import('./components/ColoquioTab').then(m => ({ default: m.ColoquioTab })))
const ApprovalsAndSecurityTab = lazy(() => import('./components/ApprovalsAndSecurityTab').then(m => ({ default: m.ApprovalsAndSecurityTab })))
const MemoryConsolidated = lazy(() => import('./components/MemoryConsolidated'))
const ModelsTab = lazy(() => import('./components/ModelsTab').then(m => ({ default: m.ModelsTab })))
const SettingsTab = lazy(() => import('./components/SettingsTab').then(m => ({ default: m.SettingsTab })))
const DiagnosticTab = lazy(() => import('./components/DiagnosticTab').then(m => ({ default: m.DiagnosticTab })))

function App() {
  const { activeTab, setActiveTab: setTab, toasts, addToast, kernelUptime, incrementUptime, coloquioUnread, setColoquioUnread, activeMentions, showMentionsDropdown, setShowMentionsDropdown } = useAppStore()
  const setActiveTab = useCallback((id: string) => {
    setTab(id)
  }, [setTab])

  const { theme, setTheme } = useThemeManager()

  const {
    online, events, guilds, stats, memoryStats, approvals,
    error,
    goldenSignals, guildsUtilization, memoryRetention, sloSummary,
    healthDetailed, sysStatus, interoception,
    bridge, refreshData, clearLogs
  } = useNexus();

  const [retrying, setRetrying] = useState(false);
  const [devMode, setDevMode] = useState<boolean>(false);

  // Check if dev_mode is active from sysStatus or config
  useEffect(() => {
    if (sysStatus?.dev_mode !== undefined) {
      setDevMode(Boolean(sysStatus.dev_mode));
    }
  }, [sysStatus]);

  const handleRetry = useCallback(async () => {
    setRetrying(true);
    try {
      await refreshData();
    } finally {
      setRetrying(false);
    }
  }, [refreshData]);
  
  const handleTabChange = useCallback((id: string) => {
    setActiveTab(id);
  }, [setActiveTab]);

  useEffect(() => {
    const onSwitchTab = (event: Event) => {
      const id = (event as CustomEvent<string>).detail;
      if (id) {
        // Map legacy tab names to the 6 official tabs if triggered by old code
        if (id === 'team') handleTabChange('coloquio');
        else if (id === 'audit') handleTabChange('approvals');
        else if (id === 'guilds') handleTabChange('models');
        else if (id === 'lab') handleTabChange('diagnostic');
        else handleTabChange(id);
      }
    };
    window.addEventListener('nexus_switch_tab', onSwitchTab);
    return () => window.removeEventListener('nexus_switch_tab', onSwitchTab);
  }, [handleTabChange]);

  const [authModalOpen, setAuthModalOpen] = useState(false);

  useEffect(() => {
    const onUnauthorized = () => {
      setAuthModalOpen(true);
    };
    window.addEventListener('nexus_unauthorized', onUnauthorized);
    return () => window.removeEventListener('nexus_unauthorized', onUnauthorized);
  }, []);

  const notify = useCallback((msg: string, type: 'info' | 'error' = 'info', guild?: string) => {
    addToast(msg, type, guild);
  }, [addToast]);

  const handleSaveToken = useCallback((token: string) => {
    if (bridge) {
      bridge.setToken(token);
    } else {
      localStorage.setItem('tylluan_token', token);
    }
    setAuthModalOpen(false);
    notify('Token de autenticación guardado correctamente', 'info');
    refreshData();
  }, [bridge, notify, refreshData]);

  const { pendingGrant, setPendingGrant, handleApproveGrant } = useHitlGrants({ bridge, notify })
  useColoquioMentions({ notify })
  useSSEBridgeEvents({ notify })

  // SSE layer — resilient, with error_result -> toast routing
  const { connectionStatus, reconnectAttempts } = useNexusSSE(bridge, {
    onError: useCallback((msg: string, guild?: string) => {
      notify(msg, 'error', guild);
    }, [notify]),
    maxEvents: 200,
  });

  const [lastRefresh, setLastRefresh] = useState(new Date());

  const formatUptime = (secs: number) => {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${h}h ${m}m ${s}s`;
  };

  useEffect(() => {
    if (online) {
      setLastRefresh(new Date());
    }
  }, [online, stats, memoryStats]);

  // Polling via centralized coordinator
  usePolling('app-uptime', incrementUptime, { interval: 'fast', enabled: online });
  usePolling('app-coloquio-unread', async () => {
    if (!bridge) return;
    try {
      const data = await bridge.getColoquioUnread('jose');
      setColoquioUnread(data.total_unread ?? 0);
    } catch {}
  }, { interval: 'quick', enabled: !!bridge && online });

  const [activeAgents, setActiveAgents] = useState<string[]>([]);

  usePolling('app-active-agents', async () => {
    if (!bridge) return;
    try {
      const data = await bridge.getColoquioThread("mision-activa") as { messages?: Array<Record<string, unknown>> };
      const msgs = (data.messages ?? []) as Array<{ created_at: number; author_id: string }>;
      const nowSecs = Math.floor(Date.now() / 1000);
      const active = Array.from(new Set(
        msgs
          .filter((m) => (nowSecs - m.created_at) < 1800)
          .map((m) => m.author_id)
      ));
      setActiveAgents(active);
    } catch {}
  }, { interval: 'medium', enabled: !!bridge && online });

  const getAgentDotColor = (agentId: string): string => {
    const cleanId = agentId.toLowerCase();
    if (cleanId.includes('claude')) return 'bg-orange-500';
    if (cleanId.includes('qwen')) return 'bg-blue-400';
    if (cleanId.includes('antigravity')) return 'bg-purple-500';
    if (cleanId.includes('buffy')) return 'bg-rose-500';
    if (cleanId.includes('opencode') || cleanId.includes('deepseek')) return 'bg-cyan-500';
    if (cleanId.includes('jose') || cleanId.includes('human')) return 'bg-amber-500';
    return 'bg-emerald-500';
  };

  // The 6 Sovereign Official Views (AG-12)
  const tabs = useMemo(() => {
    const baseTabs: Array<{
      id: string;
      name: string;
      icon: React.ComponentType<{ className?: string }>;
      badge?: number | null;
    }> = [
      { id: 'overview', name: 'Estado y Salud', icon: LayoutDashboard },
      { id: 'coloquio', name: 'Coloquio', icon: MessageSquare, badge: coloquioUnread > 0 ? coloquioUnread : null },
      { id: 'approvals', name: 'Aprobaciones', icon: ShieldCheck, badge: approvals.length > 0 ? approvals.length : null },
      { id: 'memory', name: 'Memoria', icon: Database },
      { id: 'models', name: 'Modelos', icon: Cpu },
      { id: 'settings', name: 'Ajustes', icon: Settings },
    ];

    if (devMode) {
      baseTabs.push({ id: 'diagnostic', name: 'Diagnóstico', icon: Beaker });
    }

    return baseTabs;
  }, [coloquioUnread, approvals.length, devMode]);

  return (
    <div className="min-h-screen bg-background text-foreground font-sans selection:bg-primary/20">
      {/* Top Navbar */}
      <header className="h-16 border-b border-border bg-card/90 backdrop-blur-md sticky top-0 z-50 px-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <button
            type="button"
            aria-label="Ir a Estado y Salud"
            className="flex items-center gap-2 group cursor-pointer rounded-xl focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            onClick={() => handleTabChange('overview')}
          >
            <TylluanLogo size="md" showText={true} animated={false} />
          </button>
        </div>

        <div className="flex min-w-0 items-center gap-2 md:gap-4 lg:gap-6">
          <div className="hidden lg:flex items-center gap-4 text-[11px] font-mono text-muted-foreground">
             <div className="flex flex-col items-end">
               <span className="uppercase opacity-70 tracking-widest text-[11px]">Activo</span>
               <span className="text-foreground">{online ? formatUptime(kernelUptime) : '—'}</span>
             </div>
             <div className="h-6 w-px bg-border" />
             <div className="flex flex-col items-end">
               <span className="uppercase opacity-70 tracking-widest text-[11px]">Última Actualización</span>
               <span className="text-foreground">{lastRefresh.toLocaleTimeString()}</span>
             </div>
          </div>
          
          {activeAgents.length > 0 && (
            <div className="hidden sm:flex items-center gap-2 px-3 py-1.5 rounded-full bg-card border border-border text-[11px] font-mono font-bold uppercase tracking-wider text-muted-foreground">
              <span className="text-foreground">{activeAgents.length} {activeAgents.length === 1 ? 'agente' : 'agentes'}</span>
              <div className="flex gap-1">
                {activeAgents.map(agent => (
                  <span
                    key={agent}
                    className={`w-1.5 h-1.5 rounded-full ${getAgentDotColor(agent)}`}
                    title={`@${agent} activo`}
                  />
                ))}
              </div>
            </div>
          )}

          <div className="hidden md:block">
            <DeviceStatusBadge bridge={bridge} />
          </div>

          <div
            role="status"
            aria-live="polite"
            className={cn(
              'hidden md:flex items-center gap-2 px-3 py-1.5 rounded-full border transition-colors',
              connectionStatus === 'connected'
                ? 'bg-card border-border text-foreground'
                : connectionStatus === 'reconnecting'
                ? 'bg-amber-500/10 border-amber-500/30 text-amber-600 dark:text-amber-400'
                : 'bg-destructive/10 border-destructive/30 text-destructive'
            )}
          >
            {connectionStatus === 'connected'
              ? <Wifi className="w-3 h-3 text-emerald-400" aria-hidden="true" />
              : connectionStatus === 'reconnecting'
              ? <Radio className="w-3 h-3 text-amber-400" aria-hidden="true" />
              : <WifiOff className="w-3 h-3 text-red-400" aria-hidden="true" />}
            <span className="text-[11px] font-bold uppercase tracking-wider">
              {connectionStatus === 'connected'
                ? '● Conectado'
                : connectionStatus === 'reconnecting'
                ? `▲ Reconectando (${reconnectAttempts})`
                : '■ Sin Conexión'}
            </span>
          </div>
          
          {/* Theme Selector Toggle */}
          <div role="group" aria-label="Selector de Tema" className="flex items-center gap-1 bg-muted/60 border border-border rounded-full p-0.5">
            <button
              type="button"
              aria-label="Tema Claro"
              aria-pressed={theme === 'light'}
              onClick={() => setTheme('light')}
              className={cn(
                "p-1.5 rounded-full transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                theme === 'light' ? "bg-card text-primary shadow-sm" : "text-muted-foreground hover:text-foreground"
              )}
              title="Tema Claro"
            >
              <Sun className="w-3.5 h-3.5" aria-hidden="true" />
            </button>
            <button
              type="button"
              aria-label="Tema Oscuro"
              aria-pressed={theme === 'dark'}
              onClick={() => setTheme('dark')}
              className={cn(
                "p-1.5 rounded-full transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                theme === 'dark' ? "bg-card text-primary shadow-sm" : "text-muted-foreground hover:text-foreground"
              )}
              title="Tema Oscuro"
            >
              <Moon className="w-3.5 h-3.5" aria-hidden="true" />
            </button>
            <button
              type="button"
              aria-label="Tema del Sistema"
              aria-pressed={theme === 'system'}
              onClick={() => setTheme('system')}
              className={cn(
                "p-1.5 rounded-full transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                theme === 'system' ? "bg-card text-primary shadow-sm" : "text-muted-foreground hover:text-foreground"
              )}
              title="Tema del Sistema"
            >
              <Monitor className="w-3.5 h-3.5" aria-hidden="true" />
            </button>
          </div>

          {/* Notifications Bell */}
          <div className="relative">
            <button
              type="button"
              aria-label="Menciones Recientes"
              aria-expanded={showMentionsDropdown}
              onClick={() => setShowMentionsDropdown(!showMentionsDropdown)}
              className={cn(
                "p-2 rounded-full border transition-all relative cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none",
                activeMentions.length > 0 
                  ? "bg-primary/10 border-primary/30 text-primary" 
                  : "bg-card border-border text-muted-foreground hover:text-foreground"
              )}
              title="Menciones Recientes"
            >
              <Bell className="w-4 h-4" aria-hidden="true" />
              {activeMentions.length > 0 && (
                <span className="absolute -top-1 -right-1 w-2 h-2 bg-primary rounded-full ring-2 ring-card" />
              )}
            </button>

            {showMentionsDropdown && (
              <div className="absolute right-0 mt-2 w-80 bg-card border border-border rounded-xl shadow-2xl z-50 overflow-hidden">
                <div className="px-4 py-3 bg-muted/40 border-b border-border flex items-center justify-between">
                  <span className="text-xs font-bold text-foreground uppercase">Menciones (@yo)</span>
                  {activeMentions.length > 0 && (
                    <button 
                      type="button"
                      aria-label="Limpiar Menciones"
                      onClick={() => {
                        useAppStore.getState().clearMentions();
                        setShowMentionsDropdown(false);
                      }}
                      className="text-[11px] text-primary hover:underline font-bold uppercase cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded"
                    >
                      Limpiar
                    </button>
                  )}
                </div>
                <div className="max-h-64 overflow-y-auto divide-y divide-border/60">
                  {activeMentions.length === 0 ? (
                    <div className="px-4 py-6 text-center text-xs text-muted-foreground">
                      Sin menciones recientes
                    </div>
                  ) : (
                    activeMentions.map((m) => (
                      <div key={m.id} className="p-3 hover:bg-muted/30 transition-colors">
                        <div className="flex items-center justify-between mb-1">
                          <span className="text-xs font-bold text-primary">@{m.sender}</span>
                          <span className="text-[11px] font-mono text-muted-foreground">#{m.channel}</span>
                        </div>
                        <p className="text-xs text-foreground line-clamp-2">{m.message}</p>
                        <span className="text-[11px] font-mono text-muted-foreground block mt-1">
                          {m.ts.toLocaleTimeString()}
                        </span>
                      </div>
                    ))
                  )}
                </div>
              </div>
            )}
          </div>

          {/* Bearer Token Config Button */}
          <button
            type="button"
            aria-label="Configurar Bearer Token"
            onClick={() => setAuthModalOpen(true)}
            className="p-2 rounded-full border bg-card border-border text-muted-foreground hover:text-primary hover:border-primary/40 transition-all cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
            title="Configurar Bearer Token"
          >
            <Key className="w-4 h-4" aria-hidden="true" />
          </button>

          {online && healthDetailed ? (
            <div 
              role="status"
              aria-live="polite"
              className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-card border border-border cursor-help group relative text-foreground"
              title={`Embeddings: ${healthDetailed.components?.embeddings?.ok ? '✓' : '✗'} | Reranker: ${healthDetailed.components?.reranker?.ok ? '✓' : '✗'} | Guilds: ${healthDetailed.components?.guilds?.active}/${healthDetailed.components?.guilds?.total} | Silva: ${healthDetailed.components?.silva?.nodes}n/${healthDetailed.components?.silva?.edges}e`}
            >
              <div className={cn(
                "w-2 h-2 rounded-full",
                healthDetailed.score >= 80 ? "bg-emerald-500" :
                healthDetailed.score >= 50 ? "bg-amber-500" : "bg-red-500"
              )} />
              <span className="text-[11px] font-bold uppercase tracking-wider">
                {healthDetailed.score >= 80 ? '● ' : healthDetailed.score >= 50 ? '▲ ' : '■ '}
                {healthDetailed.status}
              </span>
            </div>
          ) : !online ? (
            <div 
              role="status"
              className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-destructive/10 border border-destructive/30 text-destructive"
            >
              <div className="w-2 h-2 rounded-full bg-destructive" />
              <span className="text-[11px] font-bold uppercase tracking-wider">
                ■ DESCONECTADO
              </span>
            </div>
          ) : null}
        </div>
      </header>

      <div className="flex h-[calc(100vh-64px)] overflow-hidden">
        {/* Sidebar Navigation (232px Fixed according to DISENO_UI_2026-10-28.md) */}
        <aside aria-label="Navegación Principal" className="w-16 md:w-[232px] border-r border-border bg-card flex flex-col shrink-0 overflow-y-auto transition-all duration-300">
          <div className="p-2 md:p-3 space-y-1">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              const active = activeTab === tab.id || (activeTab === 'team' && tab.id === 'coloquio') || (activeTab === 'audit' && tab.id === 'approvals');
              return (
                <button
                  key={tab.id}
                  type="button"
                  aria-label={tab.name}
                  aria-current={active ? 'page' : undefined}
                  onClick={() => handleTabChange(tab.id)}
                  className={cn(
                    "w-full flex items-center justify-between p-2 md:px-3 md:py-2.5 rounded-xl text-sm font-medium transition-all group focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none cursor-pointer",
                    active 
                      ? "bg-primary/10 text-primary font-semibold" 
                      : "text-muted-foreground hover:text-foreground hover:bg-muted/50"
                  )}
                >
                  <div className="flex items-center gap-3">
                    <Icon className={cn("w-4 h-4 transition-colors shrink-0", active ? "text-primary" : "text-muted-foreground group-hover:text-foreground")} aria-hidden="true" />
                    <span className="hidden md:inline">{tab.name}</span>
                  </div>
                  {tab.badge && (
                    <span className="hidden md:inline-block px-1.5 py-0.5 rounded-full bg-primary/15 text-primary border-primary/30 text-[11px] font-bold min-w-[1.2rem] text-center border">
                      {tab.badge}
                    </span>
                  )}
                </button>
              );
            })}
          </div>

          {/* Footer: Version and commit without ports */}
          <div className="mt-auto p-2 md:p-3 border-t border-slate-800 bg-slate-900/20 flex flex-col items-center md:items-stretch">
             <div className="flex items-center gap-2 mb-2">
               <div className="w-6 h-6 rounded bg-slate-800 flex items-center justify-center text-[10px] font-mono font-bold text-slate-400 border border-slate-700 shrink-0">TY</div>
               <div className="overflow-hidden hidden md:block">
                 <p className="text-[11px] font-bold text-slate-300 uppercase truncate">Tylluan Sovereign</p>
                 <p className="text-[10px] text-slate-400 font-mono truncate">{sysStatus?.version ? `v${sysStatus.version}` : 'v0.17.2'}</p>
               </div>
             </div>
             <div className="h-1 bg-slate-800 rounded-full overflow-hidden w-full hidden md:block">
                <div className={cn("h-full w-full", online ? "bg-emerald-500" : "bg-red-500")} />
             </div>
          </div>
        </aside>

        {/* Main Content Area */}
        <main className="flex-1 min-h-0 flex flex-col overflow-hidden bg-slate-950">
          {error && (
            <div className="m-3 md:m-6 p-4 rounded-2xl bg-red-500/10 border border-red-500/20 flex items-start justify-between gap-3">
              <div className="flex items-start gap-3">
                <AlertTriangle className="w-5 h-5 text-red-500 shrink-0 mt-0.5" />
                <div>
                  <h4 className="text-sm font-bold text-red-400">Error de Comunicación</h4>
                  <p className="text-xs text-red-500/80 mt-1">{error}</p>
                </div>
              </div>
              <button
                type="button"
                disabled={retrying}
                onClick={handleRetry}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-red-500/20 hover:bg-red-500/30 disabled:opacity-50 text-red-300 text-xs font-semibold shrink-0 cursor-pointer disabled:cursor-not-allowed transition-all focus-visible:ring-2 focus-visible:ring-red-400 focus-visible:outline-none"
              >
                {retrying && <Loader2 className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />}
                <span>{retrying ? 'Reintentando...' : 'Reintentar'}</span>
              </button>
            </div>
          )}

          {/* Consolidated View Panels */}
          <Suspense fallback={<div className="flex-1 flex items-center justify-center font-mono text-xs text-slate-400">Cargando vista...</div>}>
            <div className={cn("flex-1 min-h-0 flex flex-col", (activeTab === 'overview' || activeTab === 'approvals' || activeTab === 'audit' || activeTab === 'models' || activeTab === 'settings' || activeTab === 'diagnostic') ? "overflow-y-auto p-3 sm:p-6" : "p-3 sm:p-6")}>
              {/* 1. Estado y Salud */}
              {activeTab === 'overview' && (
                <ErrorBoundary>
                  <OverviewConsolidated
                    online={online}
                    bridge={bridge}
                    goldenSignals={goldenSignals}
                    guildsUtilization={guildsUtilization}
                    memoryRetention={memoryRetention}
                    sloSummary={sloSummary}
                    guilds={guilds}
                    approvals={approvals}
                    memoryStats={memoryStats}
                    healthDetailed={healthDetailed}
                    sysStatus={sysStatus}
                    events={events}
                    interoception={interoception}
                    notify={notify}
                    onClearLogs={clearLogs}
                    refreshData={refreshData}
                  />
                </ErrorBoundary>
              )}

              {/* 2. Coloquio (First-class citizen) */}
              {(activeTab === 'coloquio' || activeTab === 'team') && (
                <ErrorBoundary>
                  <ColoquioTab bridge={bridge} />
                </ErrorBoundary>
              )}

              {/* 3. Aprobaciones y Seguridad */}
              {(activeTab === 'approvals' || activeTab === 'audit') && (
                <ErrorBoundary>
                  <ApprovalsAndSecurityTab bridge={bridge} />
                </ErrorBoundary>
              )}

              {/* 4. Memoria SilvaDB */}
              {activeTab === 'memory' && (
                <ErrorBoundary>
                  <MemoryConsolidated
                    bridge={bridge}
                    notify={notify}
                    memoryStats={memoryStats}
                  />
                </ErrorBoundary>
              )}

              {/* 5. Modelos y Servidor */}
              {activeTab === 'models' && (
                <ErrorBoundary>
                  <ModelsTab bridge={bridge} />
                </ErrorBoundary>
              )}

              {/* 6. Ajustes y Conexión */}
              {activeTab === 'settings' && (
                <ErrorBoundary>
                  <SettingsTab
                    bridge={bridge}
                    notify={notify}
                    onOpenAuthModal={() => setAuthModalOpen(true)}
                  />
                </ErrorBoundary>
              )}

              {/* Diagnóstico (Dev Mode) */}
              {activeTab === 'diagnostic' && (
                <ErrorBoundary>
                  <DiagnosticTab
                    bridge={bridge}
                    notify={notify}
                    events={events}
                    onClearLogs={clearLogs}
                  />
                </ErrorBoundary>
              )}
            </div>
          </Suspense>
        </main>
      </div>

      {/* Immersive HITL Capability Grant Modal */}
      {pendingGrant && (
        <div className="fixed inset-0 bg-slate-950/80 backdrop-blur-md z-[200] flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl max-w-md w-full overflow-hidden shadow-[0_0_50px_rgba(245,158,11,0.15)] animate-in zoom-in-95">
            <div className="px-6 py-4 border-b border-slate-800 bg-amber-500/5 flex items-center gap-3">
              <ShieldCheck className="w-6 h-6 text-amber-500 animate-pulse" />
              <div>
                <h3 className="text-sm font-bold text-slate-100 uppercase tracking-wider font-mono">Autorización Requerida</h3>
                <p className="text-[11px] text-amber-500 font-mono font-bold uppercase tracking-tight">HITL Sandbox Gateway Clearance</p>
              </div>
            </div>

            <div className="p-6 space-y-4 font-mono text-[11px]">
              <div className="space-y-1.5 p-3 rounded-lg bg-slate-950 border border-slate-800">
                <div className="flex justify-between">
                  <span className="text-slate-500">Guild Solicitado:</span>
                  <span className="text-slate-200 font-bold">{pendingGrant.guild}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500">Herramienta:</span>
                  <span className="text-slate-200 font-bold text-amber-400">{pendingGrant.tool}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500">Solicitado por:</span>
                  <span className="text-slate-300">{pendingGrant.agentId}</span>
                </div>
              </div>

              <div className="space-y-1">
                <span className="text-slate-500 uppercase font-black text-[11px]">Motivo de Bloqueo:</span>
                <p className="p-3 bg-red-950/15 border border-red-500/20 text-red-400 rounded leading-normal">
                  {pendingGrant.blockedReason}
                </p>
              </div>

              <div className="space-y-2 pt-2">
                <span className="text-slate-500 uppercase font-black text-[11px] block">Alcance de Autorización:</span>
                <div className="grid grid-cols-1 gap-2">
                  <button
                    onClick={() => handleApproveGrant('once')}
                    className="w-full py-2 bg-amber-500/15 hover:bg-amber-500/25 text-amber-400 border border-amber-500/25 rounded-lg text-xs font-bold font-mono transition-colors text-left px-4 flex items-center justify-between cursor-pointer"
                  >
                    <span>Autorizar solo esta vez</span>
                    <span className="text-[11px] opacity-60">scope: once</span>
                  </button>
                  <button
                    onClick={() => handleApproveGrant('session')}
                    className="w-full py-2 bg-blue-500/15 hover:bg-blue-500/25 text-blue-400 border border-blue-500/25 rounded-lg text-xs font-bold font-mono transition-colors text-left px-4 flex items-center justify-between cursor-pointer"
                  >
                    <span>Autorizar durante esta sesión</span>
                    <span className="text-[11px] opacity-60">scope: session</span>
                  </button>
                  <button
                    onClick={() => handleApproveGrant('always')}
                    className="w-full py-2 bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-400 border border-emerald-500/25 rounded-lg text-xs font-bold font-mono transition-colors text-left px-4 flex items-center justify-between cursor-pointer"
                  >
                    <span>Autorizar siempre este guild</span>
                    <span className="text-[11px] opacity-60">scope: always</span>
                  </button>
                </div>
              </div>
            </div>
            <div className="px-6 py-4 bg-slate-950/50 border-t border-slate-800 flex justify-end">
              <button
                onClick={() => setPendingGrant(null)}
                className="px-4 py-1.5 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded-lg text-xs font-bold transition-colors font-mono cursor-pointer"
              >
                Denegar Ejecución
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Sovereign Auth Modal for Bearer Token input on 401 */}
      <AuthModal
        isOpen={authModalOpen}
        onClose={() => setAuthModalOpen(false)}
        onSave={handleSaveToken}
        initialToken={bridge?.getToken()}
      />

      {/* Notifications Layer */}
      <div className="fixed bottom-6 right-6 z-[100] flex flex-col gap-3">
        {toasts.map((t) => (
          <div key={t.id} className={cn(
            "p-4 rounded-2xl border shadow-2xl flex items-center gap-3 animate-in slide-in-from-right min-w-[300px]",
            t.type === 'error' ? "bg-red-950 border-red-500/30 text-red-200" : "bg-slate-900 border-slate-800 text-slate-100"
          )}>
            {t.type === 'error' ? <AlertTriangle className="w-5 h-5 text-red-500" /> : <Zap className="w-5 h-5 text-emerald-400" />}
            <div>
              {t.guild && <span className="block text-[11px] font-bold uppercase tracking-widest text-slate-500 mb-0.5">{t.guild}</span>}
              <p className="text-sm">{t.msg}</p>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}

export default App
