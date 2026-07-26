interface Fetcher {
  fetch(path: string, options?: RequestInit): Promise<any>;
}

export interface CoherenceGateStats {
  ok: boolean;
  total_seen: number;
  total_eliminated: number;
  total_penalized: number;
  note: string;
}

export interface RecallFeedbackStats {
  ok: boolean;
  resolved: number;
  pending: number;
  threshold: number;
  pct: number;
}

export async function getCoherenceGateStats(client: Fetcher): Promise<CoherenceGateStats> {
  return await client.fetch('/api/v1/security/coherence-gate/stats');
}

export async function getRecallFeedbackStats(client: Fetcher): Promise<RecallFeedbackStats> {
  return await client.fetch('/api/v1/memory/recall-feedback/stats');
}
