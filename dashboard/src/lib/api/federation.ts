import type { Approval } from '../api-client';

interface Fetcher {
  fetch(path: string, options?: RequestInit): Promise<any>;
}

// MCP Registry
export async function listMcpExternal(client: Fetcher): Promise<any[]> {
  return await client.fetch('/api/v1/mcp/external');
}

export async function addMcpExternal(client: Fetcher, req: { name: string; url?: string; command?: string; args?: string[] }): Promise<any> {
  return await client.fetch('/api/v1/mcp/external', {
    method: 'POST',
    body: JSON.stringify(req)
  });
}

export async function removeMcpExternal(client: Fetcher, name: string): Promise<any> {
  return await client.fetch(`/api/v1/mcp/external/${encodeURIComponent(name)}`, {
    method: 'DELETE'
  });
}

export async function toggleMcpExternal(client: Fetcher, name: string, active: boolean): Promise<any> {
  return await client.fetch(`/api/v1/mcp/external/${encodeURIComponent(name)}`, {
    method: 'PUT',
    body: JSON.stringify({ active })
  });
}

// Federation
export async function listFederationPeers(client: Fetcher): Promise<any[]> {
  return await client.fetch('/api/v1/federation/peers');
}

export async function addFederationPeer(client: Fetcher, req: { name: string; url: string; token: string }): Promise<any> {
  return await client.fetch('/api/v1/federation/peers', {
    method: 'POST',
    body: JSON.stringify(req)
  });
}

export async function removeFederationPeer(client: Fetcher, name: string): Promise<any> {
  return await client.fetch(`/api/v1/federation/peers/${encodeURIComponent(name)}`, {
    method: 'DELETE'
  });
}

export async function federationSync(client: Fetcher, peerName: string): Promise<{ synced: number }> {
  return await client.fetch('/api/v1/federation/sync', {
    method: 'POST',
    body: JSON.stringify({ name: peerName })
  });
}

// Silva shareable
export async function setSilvaShareable(client: Fetcher, nodeId: string, shareable: boolean): Promise<{ shareable: boolean }> {
  return await client.fetch(`/api/v1/silva/node/${encodeURIComponent(nodeId)}/shareable`, {
    method: 'POST',
    body: JSON.stringify({ shareable })
  });
}
