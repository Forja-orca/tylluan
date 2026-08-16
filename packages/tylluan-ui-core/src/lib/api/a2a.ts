import type { ApiFetcher } from './types';

type Fetcher = ApiFetcher;

export async function getAgentCard(client: Fetcher): Promise<unknown> {
  return await client.fetch('/.well-known/agent-card.json');
}

export async function getA2aTaskStatus(client: Fetcher, taskId: string): Promise<{ status: string; result?: unknown }> {
  const raw = await client.fetch<{ error?: { message: string }; result?: { status: string; result?: unknown } }>('/a2a', {
    method: 'POST',
    body: JSON.stringify({
      jsonrpc: '2.0',
      method: 'tasks/get',
      params: { id: taskId },
      id: Date.now()
    })
  });
  if (raw.error) {
    throw new Error(raw.error.message || 'JSON-RPC error');
  }
  return raw.result as { status: string; result?: unknown };
}
