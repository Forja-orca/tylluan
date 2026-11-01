import { describe, expect, it } from 'vitest';
import { parseAgUiMessage } from '../../lib/ag-ui-parser';
import { ColoquioMessage } from '../coloquio-types';

function makeMsg(overrides: Partial<ColoquioMessage> = {}): ColoquioMessage {
  return {
    msg_id: 'msg-1',
    channel_id: 'general',
    author_id: 'antigravity',
    role: 'agent',
    content: 'Hola mundo',
    turn: 2408,
    created_at: 1793547961,
    metadata: '{}',
    ...overrides,
  };
}

describe('AG-UI / CopilotKit Message Parser (parseAgUiMessage)', () => {
  it('parses delivery handoff header [AG-24 ENTREGADO -> @claude-code @jose]', () => {
    const msg = makeMsg({
      author_id: 'antigravity',
      content: '[AG-24 ENTREGADO -> @claude-code @jose] Completados P1 + P2 en el worktree.',
    });
    const res = parseAgUiMessage(msg);
    expect(res.handoff).not.toBeNull();
    expect(res.handoff?.kind).toBe('delivery');
    expect(res.handoff?.taskId).toBe('AG-24');
    expect(res.handoff?.toAgents).toEqual(['claude-code', 'jose']);
  });

  it('parses [Task Completed] header and trailing | T-Delivered tag', () => {
    const msg = makeMsg({
      author_id: 'dsh',
      content: '[Task Completed] T-DSH-170 — silva_index ok=false diagnosticado. | T-Delivered',
    });
    const res = parseAgUiMessage(msg);
    expect(res.handoff).not.toBeNull();
    expect(res.handoff?.kind).toBe('completed');
    expect(res.handoff?.taskId).toBe('T-DSH-170');
    expect(res.handoff?.deliveredTag).toBe(true);
    expect(res.cleanContent.endsWith('T-Delivered')).toBe(false);
  });

  it('extracts inline ACTION_REPORT into structured AgUiToolCall and strips raw block', () => {
    const msg = makeMsg({
      content: `Resultado listo.\n\n---\nACTION_REPORT: {\n  "changed": "✅ Posted to 'general' as @antigravity",\n  "intended": "post to general",\n  "performed": "guild='coloquio' tool='post_to_channel' (ok)",\n  "verified": {\n    "method": "status_check_or_exit_code",\n    "status": "passed"\n  }\n}\n\n---\nRouting: guild=coloquio tool=post_to_channel`,
    });
    const res = parseAgUiMessage(msg);
    expect(res.toolCalls).toHaveLength(1);
    expect(res.toolCalls[0].guild).toBe('coloquio');
    expect(res.toolCalls[0].tool).toBe('post_to_channel');
    expect(res.toolCalls[0].status).toBe('passed');
    expect(res.cleanContent).toBe('Resultado listo.');
  });

  it('extracts [PENSAMIENTO] and <think> traces into thoughtContent', () => {
    const msg = makeMsg({
      content: '<think>Analizando tokens CSS\nSegunda línea</think>\nRespuesta final limpia.',
    });
    const res = parseAgUiMessage(msg);
    expect(res.thoughtContent).toContain('Analizando tokens CSS');
    expect(res.cleanContent).toBe('Respuesta final limpia.');
  });

  it('detects [PLAN] messages at line start but ignores inline prose mentions (T2413 regression)', () => {
    const validPlan = makeMsg({
      content: '[PLAN]\n1. Actualizar tokens\n2. Verificar contraste AA',
    });
    expect(parseAgUiMessage(validPlan).isPlan).toBe(true);

    const proseMention = makeMsg({
      content: 'Extrae trazas <think> y planes [PLAN] sin tocar coloquio-markdown.ts.',
    });
    expect(parseAgUiMessage(proseMention).isPlan).toBe(false);
  });
});
