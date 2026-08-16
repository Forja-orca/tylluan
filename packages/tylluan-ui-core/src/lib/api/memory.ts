import type { 
  GraphData, 
  BlackboardData, 
  MemoryRetention, 
  HormoneAmbient, 
  DashboardSummary, 
  AutoResearchSummary, 
  NodeTrace, 
  AgentMemory, 
  AgentMemorySummary, 
  AgentProfile, 
  ProjectSkill, 
  CollectivePulse,
  GraphNode
} from '../api-client';
import type { ApiFetcher, BackgroundJobInfo } from './types';

type Fetcher = ApiFetcher;

export async function getRepoMap(client: Fetcher): Promise<{
  root: string;
  built_at_unix: number;
  build_duration_ms: number;
  total_files: number;
  total_dirs: number;
  total_lines: number;
  languages: Record<string, { files: number; lines: number; pct: number }>;
  top_level_dirs: Array<{ name: string; file_count: number; dir_count: number }>;
  key_files: Array<{ path: string; kind: string }>;
  identifiers: Record<string, string[]>;
}> {
  return await client.fetch('/api/v1/repo-map');
}

export async function getSilvaGraph(client: Fetcher, limit = 300, cluster = false): Promise<GraphData> {
  const rawData = await client.fetch<{ nodes?: Array<Record<string, unknown>>; links?: Array<Record<string, unknown>>; edges?: Array<Record<string, unknown>> }>(`/api/v1/graph/viz?limit=${limit}&cluster=${cluster}`);
  return {
    nodes: (rawData.nodes || []).map((n) => ({ 
      ...n, 
      id: n.id as string, 
      type: (n.node_type as string) || (n.type as string) || 'agnostic',
      created_at: n.created_at as number,
      cluster_id: n.cluster_id as string | undefined
    })),
    edges: (rawData.links || rawData.edges || []).map((l) => ({ ...l }))
  };
}

export async function getGraph(client: Fetcher): Promise<{ nodes: GraphNode[]; edges: unknown[] }> {
  return client.fetch('/memory/graph');
}

export async function getMemoryStats(client: Fetcher): Promise<Record<string, unknown>> {
  return await client.fetch('/api/v1/silva/stats');
}

export async function getMailbox(client: Fetcher): Promise<{ messages: unknown[] }> {
  try {
    return await client.fetch('/api/v1/mailbox');
  } catch {
    return { messages: [] };
  }
}

export async function getBlackboard(client: Fetcher): Promise<BlackboardData> {
  try {
    return await client.fetch('/api/v1/blackboard');
  } catch {
    return { pending: [], completed_today: 0, active_agents: [], total_tasks: 0 };
  }
}

export async function getMemoryRetention(client: Fetcher): Promise<MemoryRetention> {
  return await client.fetch('/api/v1/memory/retention');
}

export async function getHormones(client: Fetcher): Promise<HormoneAmbient> {
  try {
    return await client.fetch('/api/v1/hormones');
  } catch {
    return { stress: 0, novelty: 0, saturation: 0, energy: 1.0, homeostasis: 1.0, count: 0, signals: [] };
  }
}

export async function getDashboardSummary(client: Fetcher): Promise<DashboardSummary> {
  return await client.fetch('/api/v1/dashboard/summary');
}

export async function getAutoResearchSummary(client: Fetcher): Promise<AutoResearchSummary> {
  try {
    return await client.fetch('/api/v1/autoresearch/summary');
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

export async function startAutoResearch(client: Fetcher): Promise<{ status: string; active: boolean }> {
  return await client.fetch('/api/v1/autoresearch/start', { method: 'POST' });
}

export async function stopAutoResearch(client: Fetcher): Promise<{ status: string; active: boolean }> {
  return await client.fetch('/api/v1/autoresearch/stop', { method: 'POST' });
}

export async function evaluateAutoResearch(client: Fetcher): Promise<{ status: string; experiment_run: boolean }> {
  return await client.fetch('/api/v1/autoresearch/evaluate', { method: 'POST' });
}

export async function getNodeTraces(client: Fetcher, nodeId: string): Promise<NodeTrace[]> {
  try {
    return await client.fetch(`/api/v1/silva/traces?node_id=${encodeURIComponent(nodeId)}`);
  } catch {
    return [];
  }
}

export async function getAgentMemories(client: Fetcher, agentId: string): Promise<AgentMemory[]> {
  try {
    const resp = await client.fetch<{ memories?: AgentMemory[] }>(`/api/v1/agent-memories/${encodeURIComponent(agentId)}`);
    return resp.memories ?? [];
  } catch {
    return [];
  }
}

export async function getAgentMemorySummary(client: Fetcher, agentId: string): Promise<AgentMemorySummary> {
  try {
    const resp = await client.fetch<{ summary?: { content?: string; id?: string; created_at?: string } }>(`/api/v1/agent-memories/${encodeURIComponent(agentId)}/summary`);
    if (resp.summary) {
      return { summary: resp.summary.content ?? null, node_id: resp.summary.id, created_at: resp.summary.created_at };
    }
    return { summary: null };
  } catch {
    return { summary: null };
  }
}

export async function deleteAgentMemories(client: Fetcher, agentId: string): Promise<void> {
  await client.fetch(`/api/v1/agent-memories/${encodeURIComponent(agentId)}`, { method: 'DELETE' });
}

export async function getAgentProfiles(client: Fetcher): Promise<AgentProfile[]> {
  try {
    return await client.fetch('/api/v1/agent-profiles');
  } catch {
    return [];
  }
}

async function callTylluanDoIntent(client: Fetcher, intent: string): Promise<{ text: string; isError: boolean }> {
  const raw = await client.fetchRaw<{ content?: string | string[]; is_error?: boolean }>('/api/v1/do', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ tool: 'tylluan_do', arguments: { intent } })
  });
  const text: string = Array.isArray(raw?.content) ? raw.content.join('') : ((raw?.content as string) ?? '');
  return { text, isError: !!raw?.is_error };
}

export async function getProjectSkills(client: Fetcher): Promise<Pick<ProjectSkill, 'name'>[]> {
  try {
    const res = await client.fetch('/api/v1/skills');
    if (Array.isArray(res)) return res;
  } catch {
    // Fallback to tylluan_do
  }
  const { text } = await callTylluanDoIntent(client, '@skill:list');
  if (text.startsWith('No skills saved')) return [];
  return text
    .split('\n')
    .filter((line) => line.trim().startsWith('- '))
    .map((line) => ({ name: line.trim().replace(/^- /, '') }));
}

export async function listBackgroundJobs(client: Fetcher): Promise<{ jobs: BackgroundJobInfo[]; total: number }> {
  try {
    return await client.fetch('/api/v1/jobs');
  } catch {
    return { jobs: [], total: 0 };
  }
}

export async function getProjectSkill(client: Fetcher, name: string): Promise<ProjectSkill> {
  const { text, isError } = await callTylluanDoIntent(client, `@skill:get:${name}`);
  if (isError) throw new Error(text || `Skill '${name}' not found`);
  const content = text.includes(':\n') ? text.slice(text.indexOf(':\n') + 2) : text;
  return { name, content, created_at: '' };
}

export async function saveProjectSkill(client: Fetcher, name: string, content: string): Promise<void> {
  const { isError, text } = await callTylluanDoIntent(client, `@skill:save:${name}: ${content}`);
  if (isError) throw new Error(text || 'Failed to save skill');
}

export async function deleteProjectSkill(client: Fetcher, name: string): Promise<void> {
  try {
    await client.fetch(`/api/v1/skills/${encodeURIComponent(name)}`, { method: 'DELETE' });
  } catch {
    try {
      await client.fetchRaw('/api/v1/do', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tool: 'tylluan_do',
          arguments: { intent: `@skill:delete:${name}` }
        })
      });
    } catch (e) {
      throw new Error('Project skills not implemented in backend yet.', { cause: e });
    }
  }
}

export async function startBackgroundJob(client: Fetcher, intent: string): Promise<{ jobId: string; guild: string; message: string }> {
  const { text, isError } = await callTylluanDoIntent(client, `@bg:${intent}`);
  if (isError) throw new Error(text || 'Failed to start background job');
  const idMatch = text.match(/started:\s*(\S+)/);
  const guildMatch = text.match(/Guild:\s*(\S+)/);
  if (!idMatch) throw new Error(`Unexpected response starting background job: ${text}`);
  return { jobId: idMatch[1], guild: guildMatch?.[1] ?? 'unknown', message: text };
}

export async function getJobStatus(client: Fetcher, jobId: string): Promise<{ status: 'pending' | 'completed' | 'failed'; text: string }> {
  const { text } = await callTylluanDoIntent(client, `@job:${jobId}`);
  if (text.includes('has not completed yet')) return { status: 'pending', text };
  if (/\(failed\)/.test(text)) return { status: 'failed', text };
  return { status: 'completed', text };
}

export async function getSharedKnowledge(client: Fetcher, agentA: string, agentB: string): Promise<unknown> {
  return await client.fetch(`/api/v1/silva/shared/${encodeURIComponent(agentA)}/${encodeURIComponent(agentB)}`);
}

export async function getAgentIdentity(client: Fetcher, agentId: string): Promise<{identity: unknown; memories: AgentMemory[]; competencies: Record<string, number>; summary: AgentMemorySummary | null}> {
  try {
    const identityResp = await client.fetch('/api/v1/do', {
      method: 'POST',
      body: JSON.stringify({ tool: 'tylluan_think', query: 'recuérdame', agent_id: agentId })
    });
    // Dynamically retrieve associated data via memory helpers
    const results = await Promise.allSettled([
      getAgentMemories(client, agentId),
      getAgentMemorySummary(client, agentId),
      getAgentProfiles(client)
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

export async function maintenance_vacuum(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/maintenance/vacuum', { method: 'POST' });
}

export async function maintenance_checkpoint(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/maintenance/checkpoint', { method: 'POST' });
}

export async function maintenance_decay(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/maintenance/decay', { method: 'POST' });
}

export async function maintenance_purge(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/maintenance/purge', { method: 'POST' });
}

export async function maintenance_reindex(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/memory/reindex', { method: 'POST' });
}

export async function maintenance_status(client: Fetcher): Promise<{ status: string; brain_size_bytes: number; brain_size_human: string; last_export: string; storage_mode: string; node_count: number; edge_count: number }> {
  return await client.fetch('/api/v1/maintenance/status');
}

export async function ingestText(client: Fetcher, content: string, opts?: {
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
  return await client.fetchRaw(url, { method: 'POST', body: form });
}

export async function ingestUrl(client: Fetcher, url: string, tags?: string): Promise<{ status: string; response: string }> {
  return await client.fetchRaw('/api/v1/do', {
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

export async function uploadFile(client: Fetcher, file: File): Promise<{ status: string; file: string; original_name: string; pipeline: string }> {
  const formData = new FormData();
  formData.append('file', file);
  return await client.fetchRaw('/api/v1/ingest/upload', {
    method: 'POST',
    body: formData
  });
}

export async function fetchSessionDigests(client: Fetcher, limit = 3): Promise<Array<{agent_id: string, content: string, created_at: string}>> {
  try {
    const res = await client.fetch<{ nodes?: Array<{ agent_id: string; content: string; created_at: string }> }>('/api/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({
        query: 'session digest episodios',
        limit,
        mode: 'personal'
      })
    });
    return res.nodes || [];
  } catch {
    return [];
  }
}

export async function recall(client: Fetcher, query: string, limit = 10): Promise<GraphNode[]> {
  try {
    const res = await client.fetch<{ nodes?: GraphNode[] }>('/api/v1/memory/search', {
      method: 'POST',
      body: JSON.stringify({ query, limit })
    });
    return res.nodes || [];
  } catch {
    return [];
  }
}

export async function deleteNode(client: Fetcher, nodeId: string): Promise<void> {
  await client.fetch(`/api/v1/silva/node/${encodeURIComponent(nodeId)}`, { method: 'DELETE' });
}

export async function getRecentNodes(client: Fetcher, limit = 10): Promise<GraphNode[]> {
  try {
    const res = await client.fetch<GraphNode[] | { nodes: GraphNode[] }>(`/api/v1/silva/recent?limit=${limit}`);
    return Array.isArray(res) ? res : (res.nodes || []);
  } catch {
    return [];
  }
}

export async function getCollectiveReputation(client: Fetcher): Promise<{ reputation: Array<{ agent_id: string; score: number }>; by_domain: Record<string, Array<{ agent_id: string; score: number }>> }> {
  try {
    return await client.fetch('/api/v1/collective/reputation');
  } catch {
    return { reputation: [], by_domain: {} };
  }
}

export async function getCollectiveHeatmap(client: Fetcher): Promise<{ heatmap: { date: string; count: number }[], window_hours: number }> {
  try {
    return await client.fetch('/api/v1/collective/heatmap');
  } catch {
    return { heatmap: [], window_hours: 0 };
  }
}

export async function getCollectivePulse(client: Fetcher): Promise<CollectivePulse> {
  try {
    return await client.fetch('/api/v1/collective/pulse');
  } catch {
    return { active_agents: [], active_count: 0, broadcasts_last_hour: 0, graph: { nodes: 0, edges: 0 }, ts: '' };
  }
}
