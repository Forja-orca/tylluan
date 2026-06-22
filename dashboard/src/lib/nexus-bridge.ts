/**
 * 🌉 NEXUS BRIDGE v3 (Sovereign React Edition)
 * TypeScript Implementation for TylluanNexus Dashboard.
 */

// ╔══════════════════════════════════════════════════════════════════════════╗
// ║ @CONTRACT: BRIDGE-API (CONTRACT-05)                                      ║
// ║ Interfaces exportadas son el contrato entre dashboard y kernel.         ║
// ║ No renombrar sin actualizar todos los componentes que las usan.           ║
// ║ Endpoints hardcodeados aqui DEBEN existir en api_v1.rs                   ║
// ║ Ver CONTRACTS.md sección CONTRACT-05                                     ║
// ╚══════════════════════════════════════════════════════════════════════════╝

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
}

// Golden Signals — real metrics only, no placeholders
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

// Interoception — system self-awareness data from hormonal signals
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
export interface AgentProfile {
  agent_id: string;
  first_seen: string;
  total_calls: number;
  competencies: Record<string, number>; // guild -> 0..1
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
    health: string;
  };
  client_configs: {
    claude_code_http: any;
    claude_code_sse: any;
    http_sse: any;
    lm_studio: any;
    custom_sse: any;
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

  /** Returns the kernel base URL (e.g. http://localhost:3030) */
  getBaseUrl(): string {
    return this.baseUrl;
  }

  /**
   * Create a new independent NexusBridge connected to the same kernel URL.
   * BUG-03 fix: replaces (bridge as any) unsafe access in useNexusSSE.
   */
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

    // Don't set Content-Type for FormData — browser sets multipart/form-data + boundary automatically
    const isFormData = options.body instanceof FormData;
    const headers = new Headers(options.headers);
    if (this.token) {
      headers.set('Authorization', `Bearer ${this.token}`);
    }
    if (!isFormData && !headers.has('Content-Type')) {
      headers.set('Content-Type', 'application/json');
    }

    // Vision inference on CPU can take 30-120s — give it 3 minutes
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
    // 1. Handle standard Kernel events (from SSE contract)
    if (raw.type) {
      return {
        type: raw.type,
        data: raw.data || raw,
        source: 'dashboard' as const,
        ts: raw.ts || Date.now()
      };
    }
    
    // 2. Handle MCP-style notifications
    if (raw.method) {
      return {
        type: raw.method.replace('notifications/', '').replace(/\//g, '_'),
        data: raw.params,
        source: 'mcp',
        ts: Date.now()
      };
    }
    
    // 3. Fallback
    return { type: 'unknown', data: raw, source: 'raw', ts: Date.now() };
  }

  async getConfig(): Promise<any> {
    return await this.fetch('/api/v1/config');
  }

  async saveConfig(content: string): Promise<void> {
    await this.fetch('/api/v1/config', {
      method: 'POST',
      body: JSON.stringify({ content })
    });
  }

  async getSilvaGraph(limit = 300, cluster = false): Promise<GraphData> {
    const rawData = await this.fetch(`/api/v1/graph/viz?limit=${limit}&cluster=${cluster}`);
    return {
      nodes: (rawData.nodes || []).map((n: any) => ({ 
        ...n, 
        id: n.id, 
        type: n.node_type || n.type || 'agnostic',
        created_at: n.created_at,
        cluster_id: n.cluster_id
      })),
      edges: (rawData.links || rawData.edges || []).map((l: any) => ({ ...l }))
    };
  }

  // Expose fetch for custom endpoints
  async fetchRaw(path: string, options: RequestInit = {}) {
    return this.fetch(path, options);
  }

  async getHealth() {
    return await this.fetch('/health');
  }

  async health_detailed() {
    return await this.fetch('/api/v1/health/detailed');
  }

  async probe(): Promise<ProbeResult | null> {
    try {
      return await this.fetch('/api/v1/mcp/probe');
    } catch {
      return null;
    }
  }

  async getGraph(): Promise<{ nodes: any[], edges: any[] }> {
    return this.fetch('/memory/graph');
  }

  async getGuilds() {
    return await this.fetch('/api/v1/guilds');
  }

  async getCapabilities() {
    return await this.fetch('/api/v1/capabilities');
  }

  async getStats() {
    try {
      return await this.fetch('/api/v1/guilds/health');
    } catch {
      return { guilds: [], uptime: 0 };
    }
  }

  async getGuildHealth(): Promise<any[]> {
    try {
      return await this.fetch('/api/v1/guilds/health');
    } catch {
      return [];
    }
  }

  async getMemoryStats() {
    return await this.fetch('/api/v1/silva/stats');
  }

  async getApprovals() {
    return await this.fetch('/api/v1/system/approvals');
  }

  async getMailbox() {
    try {
      return await this.fetch('/api/v1/mailbox');
    } catch {
      return { messages: [] };
    }
  }

  async getBlackboard(): Promise<BlackboardData> {
    try {
      return await this.fetch('/api/v1/blackboard');
    } catch {
      return { pending: [], completed_today: 0, active_agents: [], total_tasks: 0 };
    }
  }

  async startGuild(name: string) {
    // BUG-01 fix: use correct path-param endpoint
    return await this.fetch(`/api/v1/guilds/${name}/start`, { method: 'POST' });
  }

  async stopGuild(name: string) {
    return await this.fetch(`/api/v1/guilds/${name}/stop`, { method: 'POST' });
  }

  async testGuild(name: string) {
    return await this.fetch(`/api/v1/guilds/${name}/test`, { method: 'POST' });
  }

  async approveAction(id: string) {
    return await this.fetch(`/api/v1/system/approvals/${id}/approve`, { method: 'POST' });
  }

  async rejectAction(id: string) {
    return await this.fetch(`/api/v1/system/approvals/${id}/reject`, { method: 'POST' });
  }

  // Decision-Oriented Endpoints (Golden Signals)
  async getGoldenSignals(): Promise<GoldenSignals> {
    return await this.fetch('/api/v1/health/golden-signals');
  }

  async getGuildsUtilization(): Promise<GuildsUtilization> {
    return await this.fetch('/api/v1/guilds/utilization');
  }

  async getMemoryRetention(): Promise<MemoryRetention> {
    return await this.fetch('/api/v1/memory/retention');
  }

  async getSloSummary(): Promise<SloSummary> {
    return await this.fetch('/api/v1/slo/summary');
  }

  async getInteroception(): Promise<Interoception> {
    return await this.fetch('/api/v1/interoception');
  }

  async getHormones(): Promise<HormoneAmbient> {
    try {
      return await this.fetch('/api/v1/hormones');
    } catch {
      return { stress: 0, novelty: 0, saturation: 0, energy: 1.0, homeostasis: 1.0, count: 0, signals: [] };
    }
  }

  async getDashboardSummary(): Promise<DashboardSummary> {
    return await this.fetch('/api/v1/dashboard/summary');
  }

  async getAutoResearchSummary(): Promise<AutoResearchSummary> {
    try {
      return await this.fetch('/api/v1/autoresearch/summary');
    } catch {
      return {
        status: "Idle",
        current_mutation: null,
        progress: {
          current_step: 0,
          total_steps: 100,
          last_improvement_at: 0
        },
        metrics: {
          baseline: { recall_1: 0.65, recall_5: 0.90, latency_ms: 202.0 },
          current: { recall_1: 0.65, recall_5: 0.90, latency_ms: 202.0 }
        },
        lineage: []
      };
    }
  }

  async startAutoResearch(): Promise<{ status: string; active: boolean }> {
    return await this.fetch('/api/v1/autoresearch/start', { method: 'POST' });
  }

  async stopAutoResearch(): Promise<{ status: string; active: boolean }> {
    return await this.fetch('/api/v1/autoresearch/stop', { method: 'POST' });
  }

  async evaluateAutoResearch(): Promise<{ status: string; experiment_run: boolean }> {
    return await this.fetch('/api/v1/autoresearch/evaluate', { method: 'POST' });
  }


  async getNodeTraces(nodeId: string): Promise<NodeTrace[]> {
    try {
      return await this.fetch(`/api/v1/silva/traces?node_id=${encodeURIComponent(nodeId)}`);
    } catch {
      return [];
    }
  }

  async getAgentMemories(agentId: string): Promise<AgentMemory[]> {
    try {
      const resp = await this.fetch(`/api/v1/agent-memories/${encodeURIComponent(agentId)}`);
      return resp.memories ?? [];
    } catch {
      return [];
    }
  }

  async getAgentMemorySummary(agentId: string): Promise<AgentMemorySummary> {
    try {
      const resp = await this.fetch(`/api/v1/agent-memories/${encodeURIComponent(agentId)}/summary`);
      if (resp.summary) {
        return { summary: resp.summary.content ?? null, node_id: resp.summary.id, created_at: resp.summary.created_at };
      }
      return { summary: null };
    } catch {
      return { summary: null };
    }
  }

  async deleteAgentMemories(agentId: string): Promise<void> {
    await this.fetch(`/api/v1/agent-memories/${encodeURIComponent(agentId)}`, { method: 'DELETE' });
  }

  async getAgentProfiles(): Promise<AgentProfile[]> {
    try {
      return await this.fetch('/api/v1/agent-profiles');
    } catch {
      return [];
    }
  }

  async getSharedKnowledge(agentA: string, agentB: string): Promise<any> {
    return await this.fetch(`/api/v1/silva/shared/${encodeURIComponent(agentA)}/${encodeURIComponent(agentB)}`);
  }

  async getAgentIdentity(agentId: string): Promise<{identity: any; memories: AgentMemory[]; competencies: Record<string, number>; summary: AgentMemorySummary | null}> {
    try {
      const identityResp = await this.fetch('/api/v1/do', {
        method: 'POST',
        body: JSON.stringify({ tool: 'tylluan_think', query: 'recuérdame', agent_id: agentId })
      });
      const results = await Promise.allSettled([
        this.getAgentMemories(agentId),
        this.getAgentMemorySummary(agentId),
        this.getAgentProfiles()
      ]);
      const memories = results[0].status === 'fulfilled' ? results[0].value : [];
      const summary = results[1].status === 'fulfilled' ? results[1].value : null;
      const profiles = results[2].status === 'fulfilled' ? results[2].value : [];
      const profile = (profiles || []).find((p: AgentProfile) => p.agent_id === agentId);
      return {
        identity: identityResp,
        memories,
        competencies: profile?.competencies || {},
        summary
      };
    } catch {
      return { identity: null, memories: [], competencies: {}, summary: null };
    }
  }

  async maintenance_vacuum() {
    return await this.fetch('/api/v1/maintenance/vacuum', { method: 'POST' });
  }

  async maintenance_checkpoint() {
    return await this.fetch('/api/v1/maintenance/checkpoint', { method: 'POST' });
  }

  async maintenance_decay() {
    return await this.fetch('/api/v1/maintenance/decay', { method: 'POST' });
  }

  async maintenance_purge() {
    return await this.fetch('/api/v1/maintenance/purge', { method: 'POST' });
  }

  async maintenance_reindex() {
    return await this.fetch('/api/v1/memory/reindex', { method: 'POST' });
  }

  async maintenance_status() {
    return await this.fetch('/api/v1/maintenance/status');
  }

  async rotateLogs() {
    return await this.maintenance_checkpoint();
  }

  async getMetricsHistory(): Promise<MetricsHistory> {
    try {
      return await this.fetch('/api/v1/metrics/history');
    } catch {
      return { snapshots: [], interval_secs: 5, capacity: 60 };
    }
  }

  async ingestText(content: string, opts?: {
    nodeType?: string;
    tags?: string;
    context?: string;
    importance?: number;
  }): Promise<{ node_id: string; status: string; triples_extracted: number; content_preview?: string; warnings?: string[] }> {
    const params = new URLSearchParams();
    if (opts?.nodeType) params.set('node_type', opts.nodeType);
    if (opts?.tags) params.set('tags', opts.tags);
    if (opts?.context) params.set('context', opts.context);
    if (opts?.importance != null) params.set('importance', String(opts.importance));

    const form = new FormData();
    form.append('text', content);

    const url = `/api/v1/ingest${params.toString() ? '?' + params.toString() : ''}`;
    return await this.fetchRaw(url, { method: 'POST', body: form });
  }

  async ingestUrl(url: string, tags?: string): Promise<{ status: string; response: string }> {
    return await this.fetchRaw('/api/v1/do', {
      method: 'POST',
      body: JSON.stringify({
        tool: 'tylluan_do',
        guild: 'ingest',
        intent: 'ingest_url',
        url,
        tags: tags || '',
        agent_id: 'dashboard'
      })
    });
  }

  async uploadFile(file: File): Promise<{ status: string; file: string; original_name: string; pipeline: string }> {
    const formData = new FormData();
    formData.append('file', file);
    return await this.fetchRaw('/api/v1/ingest/upload', {
      method: 'POST',
      body: formData
    });
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

  async getSystemStatus(): Promise<{
    silva_healthy: boolean;
    mailbox_healthy: boolean;
    curriculum_entries: number;
    stress_level: number;
    uptime_secs: number;
    embeddings_loaded: boolean;
  } | null> {
    try {
      return await this.fetch('/api/v1/system/status');
    } catch {
      return null;
    }
  }

  async fetchSessionDigests(limit = 3): Promise<Array<{agent_id: string, content: string, created_at: string}>> {
    try {
      const res = await this.fetch('/api/v1/memory/search', {
        method: 'POST',
        body: JSON.stringify({
          query: 'session digest episodios',
          limit,
          mode: 'personal'
        })
      });
      return res || [];
    } catch {
      return [];
    }
  }

  async recall(query: string, limit = 10): Promise<GraphNode[]> {
    try {
      const res = await this.fetch('/api/v1/memory/search', {
        method: 'POST',
        body: JSON.stringify({ query, limit })
      });
      return res.nodes || res || [];
    } catch {
      return [];
    }
  }

  async deleteNode(nodeId: string): Promise<void> {
    await this.fetch(`/api/v1/silva/node/${encodeURIComponent(nodeId)}`, { method: 'DELETE' });
  }

  async getRecentNodes(limit = 10): Promise<GraphNode[]> {
    try {
      const res = await this.fetch(`/api/v1/silva/recent?limit=${limit}`);
      return Array.isArray(res) ? res : (res.nodes || []);
    } catch {
      return [];
    }
  }

  async getCollectiveReputation(): Promise<{ reputation: any[], by_domain: Record<string, any[]> }> {
    try {
      return await this.fetch('/api/v1/collective/reputation');
    } catch {
      return { reputation: [], by_domain: {} };
    }
  }

  async getCollectiveHeatmap(): Promise<{ heatmap: { date: string; count: number }[], window_hours: number }> {
    try {
      return await this.fetch('/api/v1/collective/heatmap');
    } catch {
      return { heatmap: [], window_hours: 0 };
    }
  }

  async getCollectivePulse(): Promise<CollectivePulse> {
    try {
      return await this.fetch('/api/v1/collective/pulse');
    } catch {
      return { active_agents: [], active_count: 0, broadcasts_last_hour: 0, graph: { nodes: 0, edges: 0 }, ts: '' };
    }
  }

  async getSessions(): Promise<McpSession[]> {
    const r = await this.fetch('/api/v1/sessions');
    return r.sessions ?? [];
  }

  async revokeSession(id: string): Promise<void> {
    await this.fetch(`/api/v1/sessions/${id}`, { method: 'DELETE' });
  }

  // ─── M3 Federation & MCP Registry Endpoints ────────────────────────────────────

  // MCP Registry
  async listMcpExternal(): Promise<any[]> {
    return await this.fetch('/api/v1/mcp/external');
  }

  async addMcpExternal(req: { name: string; url?: string; command?: string; args?: string[] }): Promise<any> {
    return await this.fetch('/api/v1/mcp/external', {
      method: 'POST',
      body: JSON.stringify(req)
    });
  }

  async removeMcpExternal(name: string): Promise<any> {
    return await this.fetch(`/api/v1/mcp/external/${encodeURIComponent(name)}`, {
      method: 'DELETE'
    });
  }

  async toggleMcpExternal(name: string, active: boolean): Promise<any> {
    return await this.fetch(`/api/v1/mcp/external/${encodeURIComponent(name)}`, {
      method: 'PUT',
      body: JSON.stringify({ active })
    });
  }

  // Federation
  async listFederationPeers(): Promise<any[]> {
    return await this.fetch('/api/v1/federation/peers');
  }

  async addFederationPeer(req: { name: string; url: string; token: string }): Promise<any> {
    return await this.fetch('/api/v1/federation/peers', {
      method: 'POST',
      body: JSON.stringify(req)
    });
  }

  async removeFederationPeer(name: string): Promise<any> {
    return await this.fetch(`/api/v1/federation/peers/${encodeURIComponent(name)}`, {
      method: 'DELETE'
    });
  }

  async federationSync(peerName: string): Promise<{ synced: number }> {
    return await this.fetch('/api/v1/federation/sync', {
      method: 'POST',
      body: JSON.stringify({ name: peerName })
    });
  }

  // Silva shareable
  async setSilvaShareable(nodeId: string, shareable: boolean): Promise<{ shareable: boolean }> {
    return await this.fetch(`/api/v1/silva/node/${encodeURIComponent(nodeId)}/shareable`, {
      method: 'POST',
      body: JSON.stringify({ shareable })
    });
  }

  // --- Coloquio (M7) ---
  async getColoquioChannels(): Promise<{ channels: any[] }> {
    return await this.fetch('/api/v1/coloquio/channels');
  }

  async getColoquioThread(channelId: string): Promise<{ messages: any[] }> {
    return await this.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}`);
  }

  async postColoquioMessage(channelId: string, req: { author_id: string; role: string; content: string; metadata: string }): Promise<any> {
    return await this.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/post`, {
      method: 'POST',
      body: JSON.stringify(req)
    });
  }

  async createColoquioChannel(channelId: string, name: string): Promise<any> {
    return await this.fetch('/api/v1/coloquio/channels', {
      method: 'POST',
      body: JSON.stringify({ channel_id: channelId, name })
    });
  }

  async deleteColoquioChannel(channelId: string, archive: boolean): Promise<any> {
    return await this.fetch(
      `/api/v1/coloquio/channels/${encodeURIComponent(channelId)}?archive=${archive}`,
      { method: 'DELETE' }
    );
  }

  async getColoquioUnread(reader: string): Promise<{ reader: string; total_unread: number; channels: any[] }> {
    return await this.fetch(`/api/v1/coloquio/unread?reader=${encodeURIComponent(reader)}`);
  }

  async markColoquioRead(channelId: string, readerId: string, turn: number): Promise<any> {
    return await this.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/read`, {
      method: 'POST',
      body: JSON.stringify({ reader_id: readerId, turn })
    });
  }

  async postColoquioTyping(channelId: string, authorId: string, status: string): Promise<any> {
    return await this.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/typing`, {
      method: 'POST',
      body: JSON.stringify({ author_id: authorId, status })
    });
  }

}

// Standalone Helper functions (delegating to a NexusBridge instance)
export async function listMcpExternal(bridge: NexusBridge) {
  return await bridge.listMcpExternal();
}
export async function addMcpExternal(bridge: NexusBridge, req: any) {
  return await bridge.addMcpExternal(req);
}
export async function removeMcpExternal(bridge: NexusBridge, name: string) {
  return await bridge.removeMcpExternal(name);
}
export async function listFederationPeers(bridge: NexusBridge) {
  return await bridge.listFederationPeers();
}
export async function addFederationPeer(bridge: NexusBridge, req: any) {
  return await bridge.addFederationPeer(req);
}
export async function removeFederationPeer(bridge: NexusBridge, name: string) {
  return await bridge.removeFederationPeer(name);
}
export async function federationSync(bridge: NexusBridge, peerName: string) {
  return await bridge.federationSync(peerName);
}
export async function setSilvaShareable(bridge: NexusBridge, nodeId: string, shareable: boolean) {
  return await bridge.setSilvaShareable(nodeId, shareable);
}

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

