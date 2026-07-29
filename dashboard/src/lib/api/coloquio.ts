import type { ApiFetcher } from './types';

type Fetcher = ApiFetcher;

export async function getColoquioChannels(client: Fetcher): Promise<unknown> {
  return await client.fetch('/api/v1/coloquio/channels');
}

export async function getColoquioThread(client: Fetcher, channelId: string): Promise<unknown> {
  return await client.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}`);
}

export async function postColoquioMessage(client: Fetcher, channelId: string, req: { author_id: string; role: string; content: string; metadata: string }): Promise<{ turn: number }> {
  return await client.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/post`, {
    method: 'POST',
    body: JSON.stringify(req)
  });
}

export async function createColoquioChannel(client: Fetcher, channelId: string, name: string): Promise<{ success: boolean }> {
  return await client.fetch('/api/v1/coloquio/channels', {
    method: 'POST',
    body: JSON.stringify({ channel_id: channelId, name })
  });
}

export async function deleteColoquioChannel(client: Fetcher, channelId: string, archive: boolean): Promise<{ success: boolean }> {
  return await client.fetch(
    `/api/v1/coloquio/channels/${encodeURIComponent(channelId)}?archive=${archive}`,
    { method: 'DELETE' }
  );
}

export async function getColoquioUnread(client: Fetcher, reader: string): Promise<{ reader: string; total_unread: number; channels: Array<{ channel_id: string; unread_count: number }> }> {
  return await client.fetch(`/api/v1/coloquio/unread?reader=${encodeURIComponent(reader)}`);
}

export async function markColoquioRead(client: Fetcher, channelId: string, readerId: string, turn: number): Promise<{ success: boolean }> {
  return await client.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/read`, {
    method: 'POST',
    body: JSON.stringify({ reader_id: readerId, turn })
  });
}

export async function postColoquioTyping(client: Fetcher, channelId: string, authorId: string, status: string): Promise<{ success: boolean }> {
  return await client.fetch(`/api/v1/coloquio/channels/${encodeURIComponent(channelId)}/typing`, {
    method: 'POST',
    body: JSON.stringify({ author_id: authorId, status })
  });
}
