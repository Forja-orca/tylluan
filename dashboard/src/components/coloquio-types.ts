export interface ColoquioChannel {
  channel_id: string;
  name: string;
  created_at: number;
  message_count: number;
  last_turn: number;
}

export interface ColoquioMessage {
  msg_id: string;
  channel_id: string;
  author_id: string;
  role: string;
  content: string;
  turn: number;
  created_at: number;
  metadata: string;
}

export interface CanvasNode {
  id: string;
  label: string;
  x: number;
  y: number;
  type: string;
}

export interface CanvasEdge {
  id: string;
  source: string;
  target: string;
}
