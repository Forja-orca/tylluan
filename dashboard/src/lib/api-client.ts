/**
 * 🌉 NEXUS CLIENT / API CLIENT v3 (Sovereign React Edition)
 * Main class base for TylluanNexus Dashboard.
 */

import * as federation from './api/federation';
import * as coloquio from './api/coloquio';
import * as a2a from './api/a2a';
import * as scopes from './api/scopes';
import * as system from './api/system';
import * as memory from './api/memory';
import * as security from './api/security';

// ============ CONTRACTS (TypeScript Interfaces) ============
export interface Session {
  id: string;
  agent_id: string;
  client_name: string;
  uptime_secs: number;
  last_activity_secs: number;
  tool_count: number;
  last_intent?: string;
  last_guild?: string;
}

export interface McpSession {
  id: string;
  client_name: string;
  agent_id: string | null;
  tool_count: number;
  last_intent: string | null;
  last_guild: string | null;
  last_active_unix: number;
  created_unix: number;
}

export type NexusEvent = 
  | { type: 'notifications_tools_list_changed'; data: { tools: any[] }; source: 'mcp'; ts: number; }
  | { type: 'notifications_resources_list_changed'; data: { resources: any[] }; source: 'mcp'; ts: number; }
  | { type: 'memory_added' | 'memory_updated'; data: { id: string; node_type: string }; source: 'mcp'; ts: number; }
  | { type: 'guild_spawned' | 'guild_killed'; data: { name: string }; source: 'mcp'; ts: number; }
  | { type: 'tool_call'; data: { status: 'started' | 'finished'; tool: string; agent_id: string; intent?: string; ok?: boolean }; source: 'mcp'; ts: number; }
  | { type: 'heartbeat'; data: { uptime_secs: number }; source: 'mcp' | 'dashboard'; ts: number; }
  | { type: 'graph_autolinked'; data: { count: number }; source: 'dashboard'; ts: number; }
  | { type: 'guild_health_updated'; data: Record<string, number>; source: 'dashboard'; ts: number; }
  | { type: string; data: any; source: 'mcp' | 'dashboard' | 'raw'; ts: number; };

export interface GraphData {
  nodes: any[];
  edges: any[];
}

export interface Guild {
  name: string;
  running: boolean;
  always_on: boolean;
  tools_count: number;
  description?: string;
  idle_seconds?: number;
  launcher_type?: 'python' | 'stdio' | 'http';
  last_latency_ms?: number;
  total_calls?: number;
  restarts_5m?: number;
}

export interface Approval {
  id: string;
  tool?: string;
  guild?: string;
  params?: Record<string, unknown>;
  status?: string;
  created_at?: string;
}

export interface GraphNode {
  id: string;
  type?: string;
  node_type?: string;
  label?: string;
  content?: string;
  weight?: number;
  created_at?: string;
  updated_at?: string;
  last_agent?: string;
  provenance?: string;
  owner_scope?: string;
}

// Golden Signals
export interface GoldenSignals {
  traffic: { active_guilds: number; total_guilds: number; active_tools: number };
  errors: { rate_percent: number; total_errors: number; critical: boolean };
  saturation: { memory_percent: number; storage_percent: number; node_count: number; edge_count: number };
  uptime_seconds: number;
  slo_target: number;
  status: { guilds_online: number; guilds_total: number; nodes: number; edges: number };
}

// Guilds Utilization
export interface GuildsUtilization {
  total: number;
  active: number;
  idle: number;
  offline: number;
  utilization_percent: number;
  active_guilds: { name: string; tools: number; idle_secs: number }[];
  idle_guilds: { name: string; always_on: boolean }[];
}

// Memory Retention
export interface MemoryRetention {
  silva: {
    total_nodes: number;
    total_edges: number;
    fresh_24h: number;
    stale_7d: number;
    cold_30d: number;
    retention_rate_percent: number;
  };
  hybrid_memory: {
    documents: number;
    disk_bytes: number;
  };
}

// SLO Summary
export interface SloSummary {
  slo_target: number;
  current_availability: number;
  error_budget_consumed_percent: number;
  error_budget_remaining_percent: number;
  status: 'healthy' | 'degraded' | 'violated';
  metrics: {
    total_services: number;
    healthy_services: number;
    total_nodes: number;
  };
}

export interface BlackboardTask {
  id: string;
  content: string;
  created_by: string;
  assigned_to: string;
  priority: number;
  age_mins: number;
}

export interface BlackboardData {
  pending: BlackboardTask[];
  completed_today: number;
  active_agents: string[];
  total_tasks: number;
}

export interface CollectivePulse {
  active_agents: string[];
  active_count: number;
  broadcasts_last_hour: number;
  graph: { nodes: number; edges: number };
  ts: string;
}

// Interoception
export interface HormoneAmbient {
  stress: number;       // 0–1
  novelty: number;      // 0–1
  saturation: number;   // 0–1
  energy: number;       // 0–1
  homeostasis: number;  // 0–1
  count: number;
  signals: any[];
}

export interface NodeTrace {
  agent_id: string;
  timestamp: number;
  trace_type: 'remember' | 'tylluan_do' | 'read' | string;
}

export interface Interoception {
  homeostasis: number;
  stress_level: number;
  knowledge_hunger: number;
  graph_density: number;
  active_pheromones: number;
  agent_rhythms: Record<string, { 
    tool_calls: number; 
    last_active_secs_ago: number; 
    client: string;
  }>;
  recommendations: string[];
  capabilities?: {
    embeddings_loaded: boolean;
    reranker_loaded: boolean;
    embedding_model: string;
    reranker_model: string;
  };
  tunnel?: {
    enabled: boolean;
    wsl_bridge_active: boolean;
    wsl_url: string | null;
  };
}

export interface AgentMemory {
  id: string;
  content: string;
  weight: number;
  created_at: string;
  importance?: number;
}
export interface AgentMemorySummary {
  summary: string | null;
  node_id?: string;
  created_at?: string;
}

export interface ProjectSkill {
  name: string;
  content: string;
  created_at: string;
}

export interface BackgroundJob {
  id: string;
  guild: string;
  intent: string;
  status: 'pending' | 'completed' | 'failed';
  started_at: string;
  elapsed_secs: number;
  result_text?: string;
}

export interface AgentProfile {
  agent_id: string;
  first_seen: string;
  total_calls: number;
  competencies: Record<string, number>;
}

export interface ProbeResult {
  detected_dialect: string;
  detected_from: string;
  user_agent: string;
  kernel_version: string;
  port: number;
  endpoints: {
    http_streamable: string;
    sse_classic: string;
  };
}

export interface DiagnosticReport {
  timestamp: string;
  status: 'healthy' | 'degraded' | 'critical';
  guilds: Array<{
    name: string;
    running: boolean;
    tools_count: number;
    issues: string[];
  }>;
  storage: {
    memory_db_ok: boolean;
    silva_db_ok: boolean;
    docs_count: number;
    nodes_count: number;
    memory_bytes: number;
    silva_bytes: number;
    recent_nodes: string[];
  };
  system: {
    total_memory_mb: number;
    used_memory_mb: number;
    memory_percent: number;
    cpu_usage_percent: number;
    process_count: number;
    thread_count: number;
    status: string;
    warnings: string[];
  };
  config_valid: boolean;
  suggestions: string[];
}

export interface SetupHint {
  profile: string;
  endpoints: {
    http_streamable: string;
    sse_classic: string;
    health: string;
  };
  client_configs: {
    claude_code_http: any;
    claude_code_sse: any;
    antigravity: any;
    lm_studio: any;
    qwen_desktop: any;
    continue_dev: any[];
    cursor: any;
  };
}

export interface MetricsSnapshot {
  ts: number;
  cpu: number;
  mem: number;
  avg_latency_ms: number | null;
}

export interface MetricsHistory {
  snapshots: MetricsSnapshot[];
  interval_secs: number;
  capacity: number;
}

export interface DashboardSummary {
  golden_signals: GoldenSignals;
  interoception: Interoception;
  hormones: HormoneAmbient;
  silva_stats: any;
  system_status: {
    status: string;
    version: string;
    uptime_secs: number;
    guilds_online: number;
    guilds_total: number;
  };
}

export interface AutoResearchSummary {
  status: string;
  current_mutation: {
    id: string;
    target: string;
    original_val: number;
    mutated_val: number;
  } | null;
  progress: {
    current_step: number;
    total_steps: number;
    last_improvement_at: number;
  };
  metrics: {
    baseline: { recall_1: number; recall_5: number; latency_ms: number };
    current: { recall_1: number; recall_5: number; latency_ms: number };
  };
  lineage: Array<{
    step: number;
    target: string;
    val: number;
    recall_1: number;
    status: string;
  }>;
  current_params?: {
    candidate_pool_mult: number;
    rerank_window: number;
    semantic_weight: number;
    dedup_cosine: number;
  };
}

export class NexusBridge {
  private baseUrl: string;
  private token: string;
  private eventSource: EventSource | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 10;
  private maxReconnectDelay = 30000;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  private onEventCallback: (ev: NexusEvent) => void;
  private onStatusCallback: (online: boolean) => void;

  private static fetchCache = new Map<string, { promise: Promise<any>, ts: number }>();

  constructor(
    onEvent: (ev: NexusEvent) => void,
    onStatus: (online: boolean) => void,
    baseUrl?: string
  ) {
    this.baseUrl = baseUrl || window.location.origin;
    this.token = localStorage.getItem('tylluan_token') || '';
    this.onEventCallback = onEvent;
    this.onStatusCallback = onStatus;
  }

  setToken(token: string) {
    this.token = token.trim();
    if (this.token) {
      localStorage.setItem('tylluan_token', this.token);
    } else {
      localStorage.removeItem('tylluan_token');
    }
  }

  getToken() {
    return this.token;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  clone(
    onEvent: (ev: NexusEvent) => void,
    onStatus: (online: boolean) => void
  ): NexusBridge {
    const b = new NexusBridge(onEvent, onStatus, this.baseUrl);
    b.setToken(this.token);
    return b;
  }

  async fetch(path: string, options: RequestInit = {}) {
    const url = `${this.baseUrl}${path}`;
    const method = (options.method || 'GET').toUpperCase();
    const useCache = method === 'GET';
    const cacheKey = `${url}:${this.token}`;

    if (useCache) {
      const cached = NexusBridge.fetchCache.get(cacheKey);
      if (cached && Date.now() - cached.ts < 5000) {
        return cached.promise;
      }
    }

    const isFormData = options.body instanceof FormData;
    const headers = new Headers(options.headers);
    if (this.token) {
      headers.set('Authorization', `Bearer ${this.token}`);
    }
    if (!isFormData && !headers.has('Content-Type')) {
      headers.set('Content-Type', 'application/json');
    }

    const isVision = path.includes('/vision/');
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), isVision ? 180000 : 15000);
    
    const promise = (async () => {
      try {
        const resp = await window.fetch(url, {
          ...options,
          headers,
          signal: controller.signal,
        });
        
        if (resp.status === 401) {
          window.dispatchEvent(new CustomEvent('nexus_unauthorized'));
          throw new Error("Unauthorized");
        }
        
        if (!resp.ok) {
          const errBody = await resp.json().catch(() => ({}));
          throw new Error(errBody.message || `HTTP Error: ${resp.status}`);
        }
        
        return await resp.json();
      } finally {
        clearTimeout(timeout);
      }
    })();

    if (useCache) {
      NexusBridge.fetchCache.set(cacheKey, { promise, ts: Date.now() });
    }

    return promise;
  }

  async fetchRaw(path: string, options: RequestInit = {}) {
    return this.fetch(path, options);
  }

  connectEvents() {
    if (this.eventSource) this.eventSource.close();

    const sseUrl = this.token
      ? `${this.baseUrl}/api/v1/events?token=${encodeURIComponent(this.token)}`
      : `${this.baseUrl}/api/v1/events`;
    this.eventSource = new EventSource(sseUrl);

    this.eventSource.onopen = () => {
      this.reconnectAttempts = 0;
      this.onStatusCallback(true);
    };

    this.eventSource.onerror = () => {
      this.onStatusCallback(false);
      this.reconnectAttempts++;

      if (this.reconnectAttempts >= this.maxReconnectAttempts) {
        console.error("🚨 [SSE] Max reconnect attempts reached, giving up");
        this.onStatusCallback(false);
        return;
      }

      const delay = Math.min(1000 * Math.pow(1.5, this.reconnectAttempts), this.maxReconnectDelay);

      if (this.eventSource) this.eventSource.close();
      this.reconnectTimeout = setTimeout(() => this.connectEvents(), delay);
    };

    this.eventSource.addEventListener('nexus', (e) => {
      try {
        const raw = JSON.parse(e.data);
        const normalized = this.normalizeEvent(raw);
        this.onEventCallback(normalized);
      } catch (err) {
        console.error("🚨 [SSE] Malformed Event:", err);
      }
    });
  }

  private normalizeEvent(raw: any): NexusEvent {
    if (raw.type) {
      return {
        type: raw.type,
        data: raw.data || raw,
        source: 'dashboard' as const,
        ts: raw.ts || Date.now()
      };
    }
    if (raw.method) {
      return {
        type: raw.method.replace('notifications/', '').replace(/\//g, '_'),
        data: raw.params,
        source: 'mcp',
        ts: Date.now()
      };
    }
    return { type: 'unknown', data: raw, source: 'raw', ts: Date.now() };
  }

  disconnect() {
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.reconnectAttempts = 0;
  }

  // ============ DOMAIN DELEGATIONS ============

  // --- A2A ---
  async getAgentCard() { return a2a.getAgentCard(this); }
  async getA2aTaskStatus(taskId: string) { return a2a.getA2aTaskStatus(this, taskId); }

  // --- Federation ---
  async listMcpExternal() { return federation.listMcpExternal(this); }
  async addMcpExternal(req: any) { return federation.addMcpExternal(this, req); }
  async removeMcpExternal(name: string) { return federation.removeMcpExternal(this, name); }
  async toggleMcpExternal(name: string, active: boolean) { return federation.toggleMcpExternal(this, name, active); }
  async listFederationPeers() { return federation.listFederationPeers(this); }
  async addFederationPeer(req: any) { return federation.addFederationPeer(this, req); }
  async removeFederationPeer(name: string) { return federation.removeFederationPeer(this, name); }
  async federationSync(peerName: string) { return federation.federationSync(this, peerName); }
  async setSilvaShareable(nodeId: string, shareable: boolean) { return federation.setSilvaShareable(this, nodeId, shareable); }

  // --- Coloquio ---
  async getColoquioChannels() { return coloquio.getColoquioChannels(this); }
  async getColoquioThread(channelId: string) { return coloquio.getColoquioThread(this, channelId); }
  async postColoquioMessage(channelId: string, req: any) { return coloquio.postColoquioMessage(this, channelId, req); }
  async createColoquioChannel(channelId: string, name: string) { return coloquio.createColoquioChannel(this, channelId, name); }
  async deleteColoquioChannel(channelId: string, archive: boolean) { return coloquio.deleteColoquioChannel(this, channelId, archive); }
  async getColoquioUnread(reader: string) { return coloquio.getColoquioUnread(this, reader); }
  async markColoquioRead(channelId: string, readerId: string, turn: number) { return coloquio.markColoquioRead(this, channelId, readerId, turn); }
  async postColoquioTyping(channelId: string, authorId: string, status: string) { return coloquio.postColoquioTyping(this, channelId, authorId, status); }

  // --- Scopes ---
  async getNodesByScopePrefix(prefix: string, limit = 100) { return scopes.getNodesByScopePrefix(this, prefix, limit); }
  async getSecurityScopes() { return scopes.getSecurityScopes(this); }
  async saveSecurityScopes(roles: { role: string; scopes: string[] }[]) { return scopes.saveSecurityScopes(this, roles); }

  // --- System ---
  async getHealth() { return system.getHealth(this); }
  async health_detailed() { return system.health_detailed(this); }
  async getDoctorReport() { return system.getDoctorReport(this); }
  async repairDoctor(target: 'guild' | 'storage' | 'benchmark', name?: string) { return system.repairDoctor(this, target, name); }
  async probe() { return system.probe(this); }
  async getStats() { return system.getStats(this); }
  async getGuildHealth() { return system.getGuildHealth(this); }
  async getApprovals() { return system.getApprovals(this); }
  async approveAction(id: string) { return system.approveAction(this, id); }
  async rejectAction(id: string) { return system.rejectAction(this, id); }
  async getGuilds() { return system.getGuilds(this); }
  async getCapabilities() { return system.getCapabilities(this); }
  async startGuild(name: string) { return system.startGuild(this, name); }
  async stopGuild(name: string) { return system.stopGuild(this, name); }
  async testGuild(name: string) { return system.testGuild(this, name); }
  async getGoldenSignals() { return system.getGoldenSignals(this); }
  async getGuildsUtilization() { return system.getGuildsUtilization(this); }
  async getSloSummary() { return system.getSloSummary(this); }
  async getInteroception() { return system.getInteroception(this); }
  async getSystemStatus() { return system.getSystemStatus(this); }
  async getSessions() { return system.getSessions(this); }
  async revokeSession(id: string) { return system.revokeSession(this, id); }
  async resumeSession(sessionId: string) { return system.resumeSession(this, sessionId); }
  async getAuditTrail(agentId?: string, limit?: number) { return system.getAuditTrail(this, agentId, limit); }
  async getCoherenceGateStats() { return security.getCoherenceGateStats(this); }
  async getRecallFeedbackStats() { return security.getRecallFeedbackStats(this); }
  async getConfig() { return system.getConfig(this); }
  async saveConfig(content: string) { return system.saveConfig(this, content); }
  async setSandboxProfile(profile: 'strict' | 'balanced' | 'permissive') { return system.setSandboxProfile(this, profile); }
  async setGuildSandboxOverride(guild: string, profile: 'strict' | 'balanced' | 'permissive') { return system.setGuildSandboxOverride(this, guild, profile); }
  async clearGuildSandboxOverride(guild: string) { return system.clearGuildSandboxOverride(this, guild); }
  async clearSessionSandboxOverride(agentId: string) { return system.clearSessionSandboxOverride(this, agentId); }
  async rotateLogs() { return system.rotateLogs(this); }
  async getMetricsHistory() { return system.getMetricsHistory(this); }
  async maintenance_onnx_clean() { return system.maintenance_onnx_clean(this); }
  async maintenance_logs_compact() { return system.maintenance_logs_compact(this); }

  // --- Memory ---
  async getRepoMap() { return memory.getRepoMap(this); }
  async getSilvaGraph(limit = 300, cluster = false) { return memory.getSilvaGraph(this, limit, cluster); }
  async getGraph() { return memory.getGraph(this); }
  async getMemoryStats() { return memory.getMemoryStats(this); }
  async getMailbox() { return memory.getMailbox(this); }
  async getBlackboard() { return memory.getBlackboard(this); }
  async getMemoryRetention() { return memory.getMemoryRetention(this); }
  async getHormones() { return memory.getHormones(this); }
  async getDashboardSummary() { return memory.getDashboardSummary(this); }
  async getAutoResearchSummary() { return memory.getAutoResearchSummary(this); }
  async startAutoResearch() { return memory.startAutoResearch(this); }
  async stopAutoResearch() { return memory.stopAutoResearch(this); }
  async evaluateAutoResearch() { return memory.evaluateAutoResearch(this); }
  async getNodeTraces(nodeId: string) { return memory.getNodeTraces(this, nodeId); }
  async getAgentMemories(agentId: string) { return memory.getAgentMemories(this, agentId); }
  async getAgentMemorySummary(agentId: string) { return memory.getAgentMemorySummary(this, agentId); }
  async deleteAgentMemories(agentId: string) { return memory.deleteAgentMemories(this, agentId); }
  async getAgentProfiles() { return memory.getAgentProfiles(this); }
  async getProjectSkills() { return memory.getProjectSkills(this); }
  async getProjectSkill(name: string) { return memory.getProjectSkill(this, name); }
  async saveProjectSkill(name: string, content: string) { return memory.saveProjectSkill(this, name, content); }
  async deleteProjectSkill(name: string) { return memory.deleteProjectSkill(this, name); }
  async startBackgroundJob(intent: string) { return memory.startBackgroundJob(this, intent); }
  async listBackgroundJobs() { return memory.listBackgroundJobs(this); }
  async getJobStatus(jobId: string) { return memory.getJobStatus(this, jobId); }
  async getSharedKnowledge(agentA: string, agentB: string) { return memory.getSharedKnowledge(this, agentA, agentB); }
  async getAgentIdentity(agentId: string) { return memory.getAgentIdentity(this, agentId); }
  async maintenance_vacuum() { return memory.maintenance_vacuum(this); }
  async maintenance_checkpoint() { return memory.maintenance_checkpoint(this); }
  async maintenance_decay() { return memory.maintenance_decay(this); }
  async maintenance_purge() { return memory.maintenance_purge(this); }
  async maintenance_reindex() { return memory.maintenance_reindex(this); }
  async maintenance_status() { return memory.maintenance_status(this); }
  async ingestText(content: string, opts?: any) { return memory.ingestText(this, content, opts); }
  async ingestUrl(url: string, tags?: string) { return memory.ingestUrl(this, url, tags); }
  async uploadFile(file: File) { return memory.uploadFile(this, file); }
  async fetchSessionDigests(limit = 3) { return memory.fetchSessionDigests(this, limit); }
  async recall(query: string, limit = 10) { return memory.recall(this, query, limit); }
  async deleteNode(nodeId: string) { return memory.deleteNode(this, nodeId); }
  async getRecentNodes(limit = 10) { return memory.getRecentNodes(this, limit); }
  async getCollectiveReputation() { return memory.getCollectiveReputation(this); }
  async getCollectiveHeatmap() { return memory.getCollectiveHeatmap(this); }
  async getCollectivePulse() { return memory.getCollectivePulse(this); }
}

// Standalone Helper functions (delegating to a NexusBridge instance)
export async function listMcpExternal(bridge: NexusBridge) { return bridge.listMcpExternal(); }
export async function addMcpExternal(bridge: NexusBridge, req: any) { return bridge.addMcpExternal(req); }
export async function removeMcpExternal(bridge: NexusBridge, name: string) { return bridge.removeMcpExternal(name); }
export async function listFederationPeers(bridge: NexusBridge) { return bridge.listFederationPeers(); }
export async function addFederationPeer(bridge: NexusBridge, req: any) { return bridge.addFederationPeer(req); }
export async function removeFederationPeer(bridge: NexusBridge, name: string) { return bridge.removeFederationPeer(name); }
export async function federationSync(bridge: NexusBridge, peerName: string) { return bridge.federationSync(peerName); }
export async function setSilvaShareable(bridge: NexusBridge, nodeId: string, shareable: boolean) { return bridge.setSilvaShareable(nodeId, shareable); }

export async function startGuild(name: string): Promise<{ status: string }> {
  const BASE = window.location.origin;
  const token = localStorage.getItem('tylluan_token') || '';
  const headers: HeadersInit = { 'Content-Type': 'application/json' };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  const res = await fetch(`${BASE}/api/v1/guilds/${name}/start`, { 
    method: 'POST',
    headers
  });
  return res.json();
}
