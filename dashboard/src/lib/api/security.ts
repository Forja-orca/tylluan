import type { ApiFetcher } from './types';

type Fetcher = ApiFetcher;

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

export interface FrictionStats {
  ok: boolean;
  total_sessions: number;
  total_workflows: number;
  total_events: number;
  manual_interventions: number;
  routing_errors: number;
  routing_ambiguous: number;
  coloquio_roundtrips: number;
  timeouts: number;
  retries: number;
  guild_errors: number;
  avg_round_trips_per_workflow: number;
  total_friction_score: number;
}

export async function getCoherenceGateStats(client: Fetcher): Promise<CoherenceGateStats> {
  return await client.fetch('/api/v1/security/coherence-gate/stats');
}

export async function getRecallFeedbackStats(client: Fetcher): Promise<RecallFeedbackStats> {
  return await client.fetch('/api/v1/memory/recall-feedback/stats');
}

export async function getFrictionStats(client: Fetcher): Promise<FrictionStats> {
  return await client.fetch('/api/v1/security/friction/stats');
}
