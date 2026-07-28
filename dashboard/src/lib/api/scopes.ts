import type { GraphNode } from '../api-client';

interface Fetcher {
  fetch(path: string, options?: RequestInit): Promise<any>;
}

export async function getNodesByScopePrefix(client: Fetcher, prefix: string, limit = 100): Promise<GraphNode[]> {
  const res = await client.fetch(`/api/v1/graph/scope?prefix=${encodeURIComponent(prefix)}&limit=${limit}`);
  return Array.isArray(res) ? res : (res.nodes || []);
}

export async function getSecurityScopes(client: Fetcher): Promise<{ roles: { role: string; scopes: string[] }[] }> {
  return await client.fetch('/api/v1/security/scopes');
}

export async function saveSecurityScopes(client: Fetcher, roles: { role: string; scopes: string[] }[]): Promise<{ success: boolean; message: string }> {
  return await client.fetch('/api/v1/security/scopes', {
    method: 'POST',
    body: JSON.stringify(roles),
  });
}
