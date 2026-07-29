import type { 
  DiagnosticReport, 
  ProbeResult, 
  GoldenSignals, 
  GuildsUtilization, 
  SloSummary, 
  Interoception, 
  McpSession, 
  Approval, 
  MetricsHistory,
  Guild
} from '../api-client';
import type { ApiFetcher, AuditEntry } from './types';

export interface TylluanConfig {
  inference?: Record<string, unknown>;
  memory?: Record<string, unknown>;
  embedding?: Record<string, unknown>;
  embeddings?: Record<string, unknown>;
  security?: Record<string, unknown>;
  guilds?: Record<string, unknown>;
  federation?: Record<string, unknown>;
  silva?: Record<string, unknown>;
  network?: Record<string, unknown>;
  p2p?: Record<string, unknown>;
  dev_mode?: boolean;
  host?: string;
  port?: number;
  [key: string]: unknown;
}

type Fetcher = ApiFetcher;

export async function getHealth(client: Fetcher): Promise<{ status: string; uptime_secs: number }> {
  return await client.fetch('/health');
}

export async function health_detailed(client: Fetcher): Promise<{ status: string; components: Record<string, { ok: boolean; detail?: string }> }> {
  return await client.fetch('/api/v1/health/detailed');
}

export async function getDoctorReport(client: Fetcher): Promise<DiagnosticReport> {
  return await client.fetch('/api/v1/doctor');
}

export async function repairDoctor(client: Fetcher, target: 'guild' | 'storage' | 'benchmark', name?: string): Promise<{ success: boolean; message: string }> {
  return await client.fetch('/api/v1/doctor/repair', {
    method: 'POST',
    body: JSON.stringify({ target, name })
  });
}

export async function probe(client: Fetcher): Promise<ProbeResult | null> {
  try {
    return await client.fetch('/api/v1/mcp/probe');
  } catch {
    return null;
  }
}

export async function getStats(client: Fetcher): Promise<{ guilds: Array<{ name: string; status: string }>; uptime: number }> {
  try {
    return await client.fetch('/api/v1/guilds/health');
  } catch {
    return { guilds: [], uptime: 0 };
  }
}

export async function getGuildHealth(client: Fetcher): Promise<Guild[]> {
  try {
    return await client.fetch('/api/v1/guilds/health');
  } catch {
    return [];
  }
}

// Approval actions are handled via tylluan_do MCP tool (plan mode).
// No dedicated HTTP endpoint for approvals exists.
export async function getApprovals(_client: Fetcher): Promise<Approval[]> {
  return []; // backend not wired — approvals go through tylluan_do plan mode
}

export async function approveAction(_client: Fetcher, _id: string): Promise<{ success: boolean }> {
  return { success: false };
}

export async function rejectAction(_client: Fetcher, _id: string): Promise<{ success: boolean }> {
  return { success: false };
}

export async function getGuilds(client: Fetcher): Promise<Guild[]> {
  return await client.fetch('/api/v1/guilds');
}

export async function getCapabilities(client: Fetcher): Promise<unknown> {
  return await client.fetch('/api/v1/capabilities');
}

export async function startGuild(client: Fetcher, name: string): Promise<{ success: boolean }> {
  return await client.fetch(`/api/v1/guilds/${name}/start`, { method: 'POST' });
}

export async function stopGuild(client: Fetcher, name: string): Promise<{ success: boolean }> {
  return await client.fetch(`/api/v1/guilds/${name}/stop`, { method: 'POST' });
}

export async function testGuild(client: Fetcher, name: string): Promise<{ success: boolean; output: string }> {
  return await client.fetch(`/api/v1/guilds/${name}/test`, { method: 'POST' });
}

export async function getGoldenSignals(client: Fetcher): Promise<GoldenSignals> {
  return await client.fetch('/api/v1/health/golden-signals');
}

export async function getGuildsUtilization(client: Fetcher): Promise<GuildsUtilization> {
  return await client.fetch('/api/v1/guilds/utilization');
}

export async function getSloSummary(client: Fetcher): Promise<SloSummary> {
  return await client.fetch('/api/v1/slo/summary');
}

export async function getInteroception(client: Fetcher): Promise<Interoception> {
  return await client.fetch('/api/v1/interoception');
}

export async function getSystemStatus(client: Fetcher): Promise<{
  silva_healthy: boolean;
  mailbox_healthy: boolean;
  curriculum_entries: number;
  stress_level: number;
  uptime_secs: number;
  embeddings_loaded: boolean;
} | null> {
  try {
    return await client.fetch('/api/v1/system/status');
  } catch {
    return null;
  }
}

export async function getSessions(client: Fetcher): Promise<McpSession[]> {
  const r = await client.fetch<{ sessions?: McpSession[] }>('/api/v1/sessions');
  return r.sessions ?? [];
}

export async function revokeSession(client: Fetcher, id: string): Promise<void> {
  await client.fetch(`/api/v1/sessions/${id}`, { method: 'DELETE' });
}

export async function getAuditTrail(client: Fetcher, agentId?: string, limit?: number): Promise<{ entries: AuditEntry[]; total: number }> {
  const params = new URLSearchParams();
  if (agentId) params.append('agent_id', agentId);
  if (limit) params.append('limit', limit.toString());
  const query = params.toString() ? `?${params.toString()}` : '';
  const raw = await client.fetch<{ entries?: Array<Record<string, unknown>>; total?: number; count?: number }>(`/api/v1/audit/trail${query}`);
  const entries = (raw.entries || []).map((row) => ({
    agent_id: row.agent_id as string,
    guild: row.guild as string,
    intent_preview: (row.intent_preview as string) || (row.intent as string) || (row.result_preview as string) || '',
    allowed: row.allowed !== undefined ? (row.allowed as boolean) : (row.status === 'ok'),
    timestamp: String(row.timestamp),
  }));
  return { entries, total: raw.total ?? raw.count ?? entries.length };
}

export async function getConfig(client: Fetcher): Promise<TylluanConfig> {
  return await client.fetch('/api/v1/config');
}

export async function saveConfig(client: Fetcher, content: string): Promise<void> {
  await client.fetch('/api/v1/config', {
    method: 'POST',
    body: JSON.stringify({ content })
  });
}

export async function setSandboxProfile(client: Fetcher, profile: 'strict' | 'balanced' | 'permissive'): Promise<{ restart_required: boolean }> {
  return await client.fetch('/api/v1/config/sandbox-profile', {
    method: 'POST',
    body: JSON.stringify({ profile })
  });
}

export async function setGuildSandboxOverride(client: Fetcher, guild: string, profile: 'strict' | 'balanced' | 'permissive'): Promise<{ restart_required: boolean }> {
  return await client.fetch('/api/v1/config/sandbox-profile/guild', {
    method: 'POST',
    body: JSON.stringify({ guild, profile })
  });
}

export async function clearGuildSandboxOverride(client: Fetcher, guild: string): Promise<{ success: boolean; restart_required?: boolean }> {
  return await client.fetch(`/api/v1/config/sandbox-profile/guild/${encodeURIComponent(guild)}`, {
    method: 'DELETE'
  });
}

export async function clearSessionSandboxOverride(client: Fetcher, agentId: string): Promise<{ success: boolean }> {
  return await client.fetch(`/api/v1/config/sandbox-profile/session/${encodeURIComponent(agentId)}`, {
    method: 'DELETE'
  });
}

export async function rotateLogs(client: Fetcher): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/maintenance/checkpoint', { method: 'POST' });
}

export async function getMetricsHistory(client: Fetcher): Promise<MetricsHistory> {
  try {
    return await client.fetch('/api/v1/metrics/history');
  } catch {
    return { snapshots: [], interval_secs: 5, capacity: 60 };
  }
}

export async function resumeSession(client: Fetcher, sessionId: string): Promise<{ success: boolean; message: string }> {
  return await client.fetch('/api/v1/sessions/resume', {
    method: 'POST',
    body: JSON.stringify({ session_id: sessionId })
  });
}

export async function maintenance_onnx_clean(client: Fetcher): Promise<{ success: boolean; message: string }> {
  return await client.fetch('/api/v1/maintenance/onnx-clean', { method: 'POST' });
}

export async function maintenance_logs_compact(client: Fetcher): Promise<{ success: boolean; message: string }> {
  return await client.fetch('/api/v1/maintenance/logs-compact', { method: 'POST' });
}
