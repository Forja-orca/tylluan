import { create } from 'zustand';
import type {
  NexusEvent, Guild, Approval, GraphNode, McpSession,
  GoldenSignals, GuildsUtilization, MemoryRetention, SloSummary,
  AgentProfile
} from '../lib/api-client';
import type { MemoryStats } from '../hooks/useNexus';

export interface Interoception {
  hormones: {
    stress: number;
    novelty: number;
    saturation: number;
    energy: number;
    homeostasis: number;
  };
  node_traces: unknown[];
}

interface NexusState {
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
  healthDetailed: Record<string, unknown> | null;
  sysStatus: Record<string, unknown> | null;
  agentProfiles: AgentProfile[];

  setOnline: (online: boolean) => void;
  addEvent: (event: NexusEvent) => void;
  clearEvents: () => void;
  setGuilds: (guilds: Guild[]) => void;
  setStats: (stats: Record<string, unknown> | null) => void;
  setMemoryStats: (stats: MemoryStats | null) => void;
  setApprovals: (approvals: Approval[]) => void;
  setSessions: (sessions: McpSession[]) => void;
  setGraph: (graph: { nodes: GraphNode[]; links: { source: string; target: string }[] }) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  setGoldenSignals: (signals: GoldenSignals | null) => void;
  setGuildsUtilization: (util: GuildsUtilization | null) => void;
  setMemoryRetention: (retention: MemoryRetention | null) => void;
  setSloSummary: (slo: SloSummary | null) => void;
  setInteroception: (intero: Interoception | null) => void;
  setHealthDetailed: (health: Record<string, unknown> | null) => void;
  setSysStatus: (status: Record<string, unknown> | null) => void;
  setAgentProfiles: (profiles: AgentProfile[]) => void;
}

export const useNexusStore = create<NexusState>((set) => ({
  online: false,
  events: [],
  guilds: [],
  stats: null,
  memoryStats: null,
  approvals: [],
  sessions: [],
  graph: { nodes: [], links: [] },
  loading: true,
  error: null,
  goldenSignals: null,
  guildsUtilization: null,
  memoryRetention: null,
  sloSummary: null,
  interoception: null,
  healthDetailed: null,
  sysStatus: null,
  agentProfiles: [],

  setOnline: (online) => set({ online }),
  addEvent: (event) =>
    set((state) => ({
      events: [event, ...state.events].slice(0, 100),
    })),
  clearEvents: () => set({ events: [] }),
  setGuilds: (guilds) => set({ guilds }),
  setStats: (stats) => set({ stats }),
  setMemoryStats: (memoryStats) => set({ memoryStats }),
  setApprovals: (approvals) => set({ approvals }),
  setSessions: (sessions) => set({ sessions }),
  setGraph: (graph) => set({ graph }),
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  setGoldenSignals: (goldenSignals) => set({ goldenSignals }),
  setGuildsUtilization: (guildsUtilization) => set({ guildsUtilization }),
  setMemoryRetention: (memoryRetention) => set({ memoryRetention }),
  setSloSummary: (sloSummary) => set({ sloSummary }),
  setInteroception: (interoception) => set({ interoception }),
  setHealthDetailed: (healthDetailed) => set({ healthDetailed }),
  setSysStatus: (sysStatus) => set({ sysStatus }),
  setAgentProfiles: (agentProfiles) => set({ agentProfiles }),
}));
