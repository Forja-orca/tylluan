/**
 * Shared types for API client functions.
 * Replaces per-file Fetcher interfaces with a single source of truth.
 */

/** Contract for API fetch functions — implemented by NexusBridge. */
export interface ApiFetcher {
  fetch<T = unknown>(path: string, options?: RequestInit): Promise<T>;
  fetchRaw<T = unknown>(path: string, options?: RequestInit): Promise<T>;
}

/** Generic API response wrapper. */
export interface ApiResponse<T> {
  data: T;
  status: number;
}

/** Background job status from the kernel. */
export interface BackgroundJobInfo {
  id: string;
  guild: string;
  name: string;
  status: string;
  created_at: string;
  elapsed_secs: number;
}

/** Audit trail entry. */
export interface AuditEntry {
  agent_id: string;
  guild: string;
  intent_preview: string;
  allowed: boolean;
  timestamp: string;
}
