import React, { createContext, useContext, useEffect, useState, useRef, useCallback } from 'react';
import {
  NexusBridge, NexusEvent,
  Guild, Approval, GraphNode, McpSession,
  GoldenSignals, GuildsUtilization, MemoryRetention, SloSummary,
  Interoception, AgentProfile
} from '../lib/nexus-bridge';

export interface MemoryStats { document_count: number; disk_usage_bytes: number; node_count?: number; edge_count?: number; ivf_ready?: boolean; n_centroids?: number; last_build?: number | null; }

interface NexusContextType {
  online: boolean;
  events: NexusEvent[];
  guilds: Guild[];
  stats: Record<string, unknown> | null;
  memoryStats: MemoryStats | null;
  approvals: Approval[];
  sessions: McpSession[];
  graph: { nodes: GraphNode[]; links: { source: string; target: string }[] };
  loading: boolean;
  error: string | null;
  goldenSignals: GoldenSignals | null;
  guildsUtilization: GuildsUtilization | null;
  memoryRetention: MemoryRetention | null;
  sloSummary: SloSummary | null;
  interoception: Interoception | null;
  healthDetailed: any | null;
  sysStatus: any | null;
  agentProfiles: AgentProfile[];
  reindexState: { running: boolean; done: number; stale: number; total: number } | null;
  bridge: NexusBridge | null;
  setToken: (token: string) => void;
  refreshData: () => Promise<void>;
  refreshGraph: () => Promise<void>;
  clearLogs: () => void;
}

const NexusContext = createContext<NexusContextType | undefined>(undefined);

export const NexusProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [online, setOnline] = useState(false);
  const [events, setEvents] = useState<NexusEvent[]>([]);
  const [guilds, setGuilds] = useState<Guild[]>([]);
  const [stats, setStats] = useState<Record<string, unknown> | null>(null);
  const [memoryStats, setMemoryStats] = useState<MemoryStats | null>(null);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [sessions, setSessions] = useState<McpSession[]>([]);
  const [graph, setGraph] = useState<{ nodes: GraphNode[]; links: { source: string; target: string }[] }>({ nodes: [], links: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Decision-Oriented State
  const [goldenSignals, setGoldenSignals] = useState<GoldenSignals | null>(null);
  const [guildsUtilization, setGuildsUtilization] = useState<GuildsUtilization | null>(null);
  const [memoryRetention, setMemoryRetention] = useState<MemoryRetention | null>(null);
  const [sloSummary, setSloSummary] = useState<SloSummary | null>(null);
  const [interoception, setInteroception] = useState<Interoception | null>(null);
  const [healthDetailed, setHealthDetailed] = useState<any | null>(null);
  const [sysStatus, setSysStatus] = useState<any | null>(null);
  const [agentProfiles, setAgentProfiles] = useState<AgentProfile[]>([]);
  const [reindexState, setReindexState] = useState<{ running: boolean; done: number; stale: number; total: number } | null>(null);
  const bridgeRef = useRef<NexusBridge | null>(null);
  const refreshDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refreshGraph = useCallback(async (cluster = false) => {
    if (!bridgeRef.current) return;
    try {
      const data = await bridgeRef.current.getSilvaGraph(500, cluster);
      setGraph({
        nodes: data.nodes || [],
        links: data.edges || []
      });
    } catch (e) {
      console.error('Failed to refresh graph:', e);
    }
  }, []);

  const refreshData = useCallback(async () => {
    if (!bridgeRef.current) return;
    setLoading(true);
    setError(null);
    try {
      const results = await Promise.allSettled([
        bridgeRef.current.getGuilds(),
        bridgeRef.current.getStats(),
        bridgeRef.current.getApprovals(),
        bridgeRef.current.getGuildsUtilization(),
        bridgeRef.current.getMemoryRetention(),
        bridgeRef.current.getSloSummary(),
        bridgeRef.current.getSessions(),
        bridgeRef.current.getAgentProfiles(),
        bridgeRef.current.getDashboardSummary()
      ]);
      const g = results[0].status === 'fulfilled' ? results[0].value : [];
      const s = results[1].status === 'fulfilled' ? results[1].value : null;
      const a = results[2].status === 'fulfilled' ? results[2].value : [];
      const gu = results[3].status === 'fulfilled' ? results[3].value : null;
      const mr = results[4].status === 'fulfilled' ? results[4].value : null;
      const slo = results[5].status === 'fulfilled' ? results[5].value : null;
      const sess = results[6].status === 'fulfilled' ? results[6].value : [];
      const profiles = results[7].status === 'fulfilled' ? results[7].value : [];
      const summary = results[8].status === 'fulfilled' ? results[8].value : null;

      const m = summary ? summary.silva_stats : null;
      const gs = summary ? summary.golden_signals : null;
      const intero = summary ? summary.interoception : null;
      const sysStat = summary ? summary.system_status : null;

      setGuilds(g.guilds || g || []);
      setStats(s);
      setMemoryStats(m);
      setApprovals(a.pending || a || []);
      setGoldenSignals(gs);
      setGuildsUtilization(gu);
      setMemoryRetention(mr);
      setSloSummary(slo);
      setInteroception(intero);
      setAgentProfiles(Array.isArray(profiles) ? profiles : []);
      setSessions(Array.isArray(sess) ? sess : []);
      setSysStatus(sysStat);
    } catch (e) {
      console.error('Failed to refresh data:', e);
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, []);

  // Visibility-aware polling: pause when tab is hidden, resume when visible
  const pausedRef = useRef(false);

  useEffect(() => {
    const onVisibilityChange = () => {
      pausedRef.current = document.visibilityState === 'hidden';
      if (!pausedRef.current && bridgeRef.current) {
        // Resume: do an immediate refresh so data isn't stale
        refreshData();
      }
    };
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => document.removeEventListener('visibilitychange', onVisibilityChange);
  }, [refreshData]);

  useEffect(() => {
    const handlePollRefresh = () => {
      if (!pausedRef.current) {
        refreshData();
      }
    };
    window.addEventListener('nexus_polling_refresh', handlePollRefresh);
    return () => window.removeEventListener('nexus_polling_refresh', handlePollRefresh);
  }, [refreshData]);

  useEffect(() => {
    const debouncedRefresh = (includeGraph: boolean) => {
      if (refreshDebounceRef.current) clearTimeout(refreshDebounceRef.current);
      refreshDebounceRef.current = setTimeout(() => {
        refreshData();
        if (includeGraph) refreshGraph();
      }, 500);
    };

    const bridge = new NexusBridge(
      (ev) => {
        setEvents(prev => [ev, ...prev].slice(0, 100));
        if (ev.type.includes('guild') || ev.type.includes('memory')) {
          debouncedRefresh(ev.type === 'memory_added');
        }
        if (ev.type === 'maintenance_finished') {
          refreshData();
          refreshGraph(true); // Auto-refresh graph with clustering after maintenance
        }
        if (ev.type === 'reindex_started') {
          setReindexState({ running: true, done: 0, stale: (ev as any).stale || 0, total: (ev as any).total || 0 });
        } else if (ev.type === 'reindex_progress') {
          setReindexState(prev => prev ? { ...prev, done: (ev as any).done || 0 } : null);
        } else if (ev.type === 'reindex_finished') {
          setReindexState(null);
          refreshData();
        }
      },
      (status) => {
        setOnline(status);
        if (status) {
          refreshData();
          refreshGraph();
        }
      },
      import.meta.env.VITE_NEXUS_URL
    );

    bridgeRef.current = bridge;
    bridge.connectEvents();

    const healthInterval = setInterval(async () => {
      if (bridgeRef.current && online && !pausedRef.current) {
        try {
          const h = await bridgeRef.current.health_detailed();
          setHealthDetailed(h);
        } catch {}
      }
    }, 30000);

    return () => {
      bridge.disconnect();
      clearInterval(healthInterval);
    };
  }, [online]);

  const setToken = (token: string) => {
    if (bridgeRef.current) {
      bridgeRef.current.setToken(token);
      refreshData();
    }
  };

  const clearLogs = useCallback(() => {
    setEvents([]);
  }, []);

  return (
    <NexusContext.Provider value={{
      online, events, guilds, stats, memoryStats, approvals, sessions, graph,
      loading, error,
      goldenSignals, guildsUtilization, memoryRetention, sloSummary,
      interoception, healthDetailed, sysStatus, agentProfiles, reindexState,
      bridge: bridgeRef.current, setToken, refreshData, refreshGraph, clearLogs
    }}>
      {children}
    </NexusContext.Provider>
  );
};

export const useNexus = () => {
  const context = useContext(NexusContext);
  if (context === undefined) {
    throw new Error('useNexus must be used within a NexusProvider');
  }
  return context;
};
