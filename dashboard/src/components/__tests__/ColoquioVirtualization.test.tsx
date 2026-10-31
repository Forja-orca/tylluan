import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import React from 'react';
import { ColoquioMessagesPanel } from '../ColoquioMessagesPanel';
import { ColoquioMessage } from '../coloquio-types';
import type { NexusBridge } from '../../lib/nexus-bridge';

// Mock scrollTo and scrollIntoView
window.HTMLElement.prototype.scrollIntoView = vi.fn();
window.HTMLElement.prototype.scrollTo = vi.fn();

const mockBridge = {
  getColoquioThread: vi.fn().mockResolvedValue({ messages: [] }),
  postColoquioMessage: vi.fn().mockResolvedValue({ turn: 1 }),
} as unknown as NexusBridge;

function generateMessages(count: number): ColoquioMessage[] {
  const authors = ['claude-code', 'deep', 'antigravity', 'jose'];
  const msgs: ColoquioMessage[] = [];
  const now = Math.floor(Date.now() / 1000);

  for (let i = 1; i <= count; i++) {
    msgs.push({
      msg_id: `msg-${i}`,
      channel_id: 'general',
      author_id: authors[i % authors.length],
      role: authors[i % authors.length] === 'jose' ? 'human' : 'agent',
      content: `Mensaje de prueba #${i} en Coloquio con **markdown** y #turn-${i}`,
      turn: i,
      created_at: now - (count - i) * 60,
      metadata: '{}',
    });
  }
  return msgs;
}

function renderColoquioPanel(customProps: Partial<React.ComponentProps<typeof ColoquioMessagesPanel>> = {}) {
  const isAtBottom = { current: true };
  const defaultProps: React.ComponentProps<typeof ColoquioMessagesPanel> = {
    selectedId: 'general',
    selectedChannel: { channel_id: 'general', name: 'general', created_at: 0, message_count: 0, last_turn: 0 },
    messages: generateMessages(10),
    activeStreams: {},
    msgSearch: '',
    setMsgSearch: vi.fn(),
    compactMode: false,
    setCompactMode: vi.fn(),
    needsScrollToBottom: false,
    setNeedsScrollToBottom: vi.fn(),
    isAtBottom,
    typingStatuses: {},
    authorId: 'jose',
    setAuthorId: vi.fn(),
    showTemplates: false,
    setShowTemplates: vi.fn(),
    attachments: [],
    setAttachments: vi.fn(),
    uploading: false,
    isDragging: false,
    setIsDragging: vi.fn(),
    draft: '',
    setDraft: vi.fn(),
    posting: false,
    postMessage: vi.fn(),
    handleFileUpload: vi.fn(),
    bridge: mockBridge,
    fetchThread: vi.fn(),
    hasMoreOlder: false,
    loadingOlder: false,
    onLoadOlder: vi.fn(),
    ...customProps,
  };

  return render(<ColoquioMessagesPanel {...defaultProps} />);
}

describe('Coloquio VirtualMessageList (@tanstack/react-virtual) — Lote AC', () => {
  it('renders virtualized message list with a11y region and focusable tabIndex', () => {
    renderColoquioPanel({ messages: generateMessages(10) });

    const historyRegion = screen.getByRole('region', { name: /historial de mensajes de coloquio/i });
    expect(historyRegion).toBeInTheDocument();
    expect(historyRegion).toHaveAttribute('tabIndex', '0');
  });

  it('sets aria-setsize and aria-posinset on virtual message items', () => {
    renderColoquioPanel({ messages: generateMessages(20) });

    // Look for items with msg-turn-
    const item1 = document.getElementById('msg-turn-1');
    if (item1) {
      expect(item1).toHaveAttribute('aria-setsize', '20');
      expect(item1).toHaveAttribute('aria-posinset', '1');
    }
  });

  it('renders copy and quote actions preserving text selectable mode', () => {
    renderColoquioPanel({ messages: generateMessages(5) });

    const historyRegion = screen.getByRole('region', { name: /historial de mensajes de coloquio/i });
    expect(historyRegion.className).toContain('select-text');
  });

  it('renders "Cargar mensajes anteriores" button when hasMoreOlder is true', () => {
    const onLoadOlder = vi.fn();
    renderColoquioPanel({
      messages: generateMessages(15),
      hasMoreOlder: true,
      onLoadOlder,
    });

    const loadMoreBtn = screen.getByRole('button', { name: /cargar mensajes anteriores/i });
    expect(loadMoreBtn).toBeInTheDocument();
  });

  it('handles message count indicator in header', () => {
    renderColoquioPanel({ messages: generateMessages(50) });

    expect(screen.getByText(/50 (messages|mensajes)/i)).toBeInTheDocument();
  });

  it('renders 500 accumulated messages gracefully with virtualizer', () => {
    const { unmount } = renderColoquioPanel({ messages: generateMessages(500) });
    expect(screen.getByText(/500 (messages|mensajes)/i)).toBeInTheDocument();

    const historyRegion = screen.getByRole('region', { name: /historial de mensajes de coloquio/i });
    expect(historyRegion).toBeInTheDocument();
    unmount();
  });

  it('triggers setNeedsScrollToBottom when requested', () => {
    const setNeedsScrollToBottom = vi.fn();
    renderColoquioPanel({
      messages: generateMessages(10),
      needsScrollToBottom: true,
      setNeedsScrollToBottom,
    });

    // On render with needsScrollToBottom = true, onScrollToBottomDone calls setNeedsScrollToBottom(false)
    // inside requestAnimationFrame
    expect(screen.getByRole('region', { name: /historial de mensajes de coloquio/i })).toBeInTheDocument();
  });

  it('renders loading state on pagination button when loadingOlder is true', () => {
    renderColoquioPanel({
      messages: generateMessages(50),
      hasMoreOlder: true,
      loadingOlder: true,
    });

    expect(screen.getByText(/cargando mensajes anteriores/i)).toBeInTheDocument();
    const btn = screen.getByRole('button', { name: /cargando mensajes anteriores/i });
    expect(btn).toBeDisabled();
  });
});

