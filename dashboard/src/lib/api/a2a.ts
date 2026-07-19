interface Fetcher {
  fetch(path: string, options?: RequestInit): Promise<any>;
}

export async function getAgentCard(client: Fetcher): Promise<any> {
  return await client.fetch('/.well-known/agent-card.json');
}

export async function getA2aTaskStatus(client: Fetcher, taskId: string): Promise<any> {
  const raw = await client.fetch('/a2a', {
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
  return raw.result;
}
