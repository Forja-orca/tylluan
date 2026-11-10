import {
  Hash, Search, X, Paperclip, Loader2, Clock,
  Check, Copy, Quote, FileText, ChevronUp, ChevronLeft
} from 'lucide-react';
import { useRef, useLayoutEffect, useEffect, useCallback, useMemo, useState } from 'react';
import { cn } from '../lib/utils';
import { NexusBridge } from '../lib/nexus-bridge';
import { agentStyle } from '../lib/agent-meta';
import { parseMarkdown, highlightHtmlContent, sanitizeHref } from '../lib/coloquio-markdown';
import { parseAgUiMessage } from '../lib/ag-ui-parser';
import { ColoquioHandoffBadge } from './coloquio/ColoquioHandoffBadge';
import { ColoquioThinkingBlock } from './coloquio/ColoquioThinkingBlock';
import { ColoquioToolCard } from './coloquio/ColoquioToolCard';
import { ColoquioHitlCard } from './coloquio/ColoquioHitlCard';
import { PatchAttachmentViewer } from './coloquio/PatchAttachmentViewer';

import { useVirtualizer } from '@tanstack/react-virtual';
import { ColoquioChannel, ColoquioMessage } from './coloquio-types';

interface VMLProps {
  messages: ColoquioMessage[];
  renderItem: (m: ColoquioMessage, p: ColoquioMessage | null) => React.ReactNode;
  scrollToBottom: boolean;
  onScrollToBottomDone: () => void;
  isAtBottom: React.MutableRefObject<boolean>;
  hasMoreOlder?: boolean;
  loadingOlder?: boolean;
  onLoadOlder?: () => void;
  jumpTurn?: number | null;
  onJumpTurnDone?: () => void;
}

function VirtualMessageList({
  messages,
  renderItem,
  scrollToBottom,
  onScrollToBottomDone,
  isAtBottom,
  hasMoreOlder,
  loadingOlder,
  onLoadOlder,
  jumpTurn,
  onJumpTurnDone,
}: VMLProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const prevScrollHeightRef = useRef<number>(0);
  const prevCountRef = useRef<number>(messages.length);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 80,
    overscan: 12,
  });

  // Preserve scroll position when loading older messages (prepending)
  useLayoutEffect(() => {
    if (loadingOlder && parentRef.current) {
      prevScrollHeightRef.current = parentRef.current.scrollHeight;
    }
  }, [loadingOlder]);

  useLayoutEffect(() => {
    if (!loadingOlder && prevScrollHeightRef.current > 0 && parentRef.current) {
      const diff = parentRef.current.scrollHeight - prevScrollHeightRef.current;
      if (diff > 0) {
        parentRef.current.scrollTop += diff;
      }
      prevScrollHeightRef.current = 0;
    }
  }, [messages, loadingOlder]);

  // Scroll to bottom when explicitly requested
  useLayoutEffect(() => {
    if (!scrollToBottom || messages.length === 0) return;
    requestAnimationFrame(() => {
      if (parentRef.current) {
        parentRef.current.scrollTop = parentRef.current.scrollHeight;
      }
      virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
      onScrollToBottomDone();
    });
  }, [scrollToBottom, messages.length, onScrollToBottomDone, virtualizer]);

  // Handle jump to specific turn
  useEffect(() => {
    if (jumpTurn === undefined || jumpTurn === null) return;
    const idx = messages.findIndex(m => m.turn === jumpTurn);
    if (idx !== -1) {
      virtualizer.scrollToIndex(idx, { align: 'center', behavior: 'smooth' });
    }
    onJumpTurnDone?.();
  }, [jumpTurn, messages, virtualizer, onJumpTurnDone]);

  // Track bottom position on scroll
  const onScroll = useCallback(() => {
    const c = parentRef.current;
    if (!c) return;
    isAtBottom.current = c.scrollHeight - c.scrollTop - c.clientHeight < 120;
  }, [isAtBottom]);

  // Auto-scroll on new incoming messages if already at bottom
  useEffect(() => {
    if (messages.length > prevCountRef.current && isAtBottom.current && parentRef.current) {
      requestAnimationFrame(() => {
        virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
      });
    }
    prevCountRef.current = messages.length;
  }, [messages.length, isAtBottom, virtualizer]);

  const virtualItems = virtualizer.getVirtualItems();

  return (
    <div
      ref={parentRef}
      onScroll={onScroll}
      tabIndex={0}
      role="region"
      aria-label="Historial de mensajes de Coloquio"
      className="flex-1 overflow-y-auto px-4 py-3 select-text focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
    >
      {hasMoreOlder && (
        <div className="flex justify-center mb-4">
          <button
            onClick={onLoadOlder}
            disabled={loadingOlder}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-primary hover:text-primary/80 bg-primary/10 hover:bg-primary/20 border border-primary/30 rounded transition-all disabled:opacity-50 cursor-pointer"
          >
            {loadingOlder ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                <span>Cargando mensajes anteriores...</span>
              </>
            ) : (
              <>
                <ChevronUp className="w-3.5 h-3.5" />
                <span>Cargar mensajes anteriores</span>
              </>
            )}
          </button>
        </div>
      )}
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualItems.map(virtualRow => {
          const msg = messages[virtualRow.index];
          if (!msg) return null;
          const prev = virtualRow.index > 0 ? messages[virtualRow.index - 1] : null;
          return (
            <div
              key={msg.msg_id}
              ref={virtualizer.measureElement}
              data-index={virtualRow.index}
              id={`msg-turn-${msg.turn}`}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualRow.start}px)`,
              }}
              aria-setsize={messages.length}
              aria-posinset={virtualRow.index + 1}
            >
              {renderItem(msg, prev)}
            </div>
          );
        })}
      </div>
    </div>
  );
}

const QUICK_TEMPLATES = [
  { label: 'Consolidar grafo', text: 'Consolidar el grafo de conocimiento SilvaDB' },
  { label: 'Informe de estado', text: 'Proporcionar informe del estado de la flota' },
  { label: 'Reindexar memoria', text: 'Reindexar SilvaDB y regenerar vectores de búsqueda' },
];


function parsePlanTasks(content: string): string[] {
  const lines = content.split('\n');
  const tasks: string[] = [];
  for (const line of lines) {
    const match = line.match(/^\s*[-*]\s*\[\s*[xX ]\s*\]\s*(.+)$/i);
    if (match) {
      tasks.push(match[1].trim());
    }
  }
  return tasks;
}

function fmtTime(u: number) { return new Date(u * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }); }

interface ColoquioMessagesPanelProps {
  selectedId: string | null;
  selectedChannel: ColoquioChannel | undefined;
  messages: ColoquioMessage[];
  activeStreams: Record<string, ColoquioMessage>;
  msgSearch: string;
  setMsgSearch: (s: string) => void;
  compactMode: boolean;
  setCompactMode: React.Dispatch<React.SetStateAction<boolean>>;
  needsScrollToBottom: boolean;
  setNeedsScrollToBottom: (b: boolean) => void;
  isAtBottom: React.MutableRefObject<boolean>;
  typingStatuses: Record<string, { ts: number; status: string }>;
  authorId: string;
  setAuthorId: (s: string) => void;
  showTemplates: boolean;
  setShowTemplates: React.Dispatch<React.SetStateAction<boolean>>;
  attachments: { file: string; name: string }[];
  setAttachments: React.Dispatch<React.SetStateAction<{ file: string; name: string }[]>>;
  uploading: boolean;
  isDragging: boolean;
  setIsDragging: (b: boolean) => void;
  draft: string;
  setDraft: React.Dispatch<React.SetStateAction<string>>;
  posting: boolean;
  postMessage: () => void;
  handleFileUpload: (files: FileList | File[]) => void;
  bridge: NexusBridge | null;
  fetchThread: () => void;
  hasMoreOlder?: boolean;
  loadingOlder?: boolean;
  onLoadOlder?: () => void;
  onBackToChannels?: () => void;
}

export function ColoquioMessagesPanel({
  selectedId,
  selectedChannel,
  messages,
  activeStreams,
  msgSearch,
  setMsgSearch,
  compactMode,
  setCompactMode,
  needsScrollToBottom,
  setNeedsScrollToBottom,
  isAtBottom,
  typingStatuses,
  authorId,
  setAuthorId,
  showTemplates,
  setShowTemplates,
  attachments,
  setAttachments,
  uploading,
  isDragging,
  setIsDragging,
  draft,
  setDraft,
  posting,
  postMessage,
  handleFileUpload,
  bridge,
  fetchThread,
  hasMoreOlder = false,
  loadingOlder = false,
  onLoadOlder,
  onBackToChannels,
}: ColoquioMessagesPanelProps) {
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [highlightedTurn, setHighlightedTurn] = useState<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const lastTypingTimeRef = useRef<number>(0);
  const composerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && showTemplates) {
        const active = document.activeElement;
        const target = e.target as Node;
        const isInside = composerRef.current && (
          composerRef.current.contains(active) || composerRef.current.contains(target)
        );
        if (!isInside) return;

        setShowTemplates(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [showTemplates, setShowTemplates]);

  // Merge database messages with real-time active streaming channels
  const allMessages = useMemo(() => {
    const list = [...messages];
    Object.values(activeStreams).forEach(streamMsg => {
      // Avoid duplication if the message just saved to database
      if (!list.some(m => m.msg_id === streamMsg.msg_id)) {
        list.push(streamMsg);
      }
    });
    return list.sort((a, b) => a.created_at - b.created_at || a.turn - b.turn);
  }, [messages, activeStreams]);

  const filteredMessages = useMemo(() =>
    (msgSearch || '').trim() ? allMessages.filter(m => m.content.toLowerCase().includes((msgSearch || '').toLowerCase())) : allMessages,
    [allMessages, msgSearch]);

  const handleCopy = (id: string, text: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleQuote = (author: string, content: string) => {
    const line = content.split('\n')[0];
    setDraft(p => `> @${author}: ${line}${content.includes('\n') ? '...' : ''}\n${p}`);
  };

  const [jumpTurn, setJumpTurn] = useState<number | null>(null);

  const scrollToTurn = (turn: number) => {
    setJumpTurn(turn);
    const el = document.getElementById(`msg-turn-${turn}`);
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }
    setHighlightedTurn(turn);
    setTimeout(() => setHighlightedTurn(null), 2500);
  };

  const handleThreadClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const t = (e.target as HTMLElement).getAttribute('data-jump-turn');
    if (t) scrollToTurn(parseInt(t, 10));
  };

  const renderDateDiv = (msg: ColoquioMessage, prev: ColoquioMessage | null) => {
    const d = new Date(msg.created_at * 1000).toDateString();
    if (prev && new Date(prev.created_at * 1000).toDateString() === d) return null;
    const today = new Date().toDateString(), yday = new Date(Date.now() - 86400000).toDateString();
    let label = new Date(msg.created_at * 1000).toLocaleDateString('es', { weekday: 'long', month: 'long', day: 'numeric' });
    if (d === today) label = 'Hoy'; else if (d === yday) label = 'Ayer';
    return (
      <div className="flex items-center justify-center my-5">
        <div className="h-px bg-slate-800 flex-1" />
        <span className="text-xs font-semibold text-muted-foreground px-3 bg-background">{label}</span>
        <div className="h-px bg-slate-800 flex-1" />
      </div>
    );
  };

  const renderMessage = (msg: ColoquioMessage, prev: ColoquioMessage | null) => {
    const isHuman = msg.role === 'human';
    const m = agentStyle(msg.author_id);
    const cont = !!(prev && prev.author_id === msg.author_id && msg.created_at - prev.created_at < 120);
    const highlight = highlightedTurn === msg.turn;

    // Detect if this message is actively streaming
    const isStreaming = activeStreams[msg.msg_id] !== undefined;

    const agUi = parseAgUiMessage(msg);
    const mainContent = agUi.cleanContent;
    const thoughtContent = agUi.thoughtContent;
    const isPlan = agUi.isPlan;
    const planTurn = msg.turn;
    const isApproved = messages.some(
      otherMsg =>
        otherMsg.turn > planTurn &&
        (otherMsg.content.toLowerCase().includes(`[aprobado] #${planTurn}`) ||
          otherMsg.content.toLowerCase().includes(`[approved] #${planTurn}`))
    );

    return (
      <>
        {renderDateDiv(msg, prev)}
        <div className={cn(
          'flex group/msg rounded-md transition-colors hover:bg-foreground/[0.02]',
          compactMode ? 'gap-1.5 py-0.5 px-1' : 'gap-2.5 py-1.5 px-2.5',
          isHuman ? 'flex-row-reverse' : ''
        )}>
          <div className={cn('shrink-0 mt-0.5', compactMode ? 'w-6' : 'w-8')}>
            {!cont && (
              <div className={cn(
                'rounded-md flex items-center justify-center font-bold border shadow-sm',
                compactMode ? 'w-6 h-6 text-[11px]' : 'w-8 h-8 text-xs',
                m.bg, m.color, m.border
              )}>
                {m.initial}
              </div>
            )}
          </div>
          <div className={cn('flex flex-col gap-1 max-w-[85%] min-w-0', isHuman ? 'items-end' : 'items-start')}>
            {!cont && (
              <div className={cn('flex items-center gap-2', isHuman ? 'flex-row-reverse' : '')}>
                <span className={cn('text-xs font-bold tracking-tight', m.color)}>{isHuman ? msg.author_id : `@${msg.author_id}`}</span>
                <button
                  type="button"
                  onClick={() => scrollToTurn(msg.turn)}
                  className="text-[11px] text-muted-foreground font-mono hover:text-primary focus-visible:outline-none rounded transition-colors"
                >
                  #{msg.turn}
                </button>
                <span className="text-[11px] text-muted-foreground font-mono">{fmtTime(msg.created_at)}</span>
                {isStreaming && (
                  <span className="text-[11px] text-cyan-700 dark:text-cyan-300 bg-cyan-500/10 border border-cyan-500/30 px-1.5 py-0.5 rounded flex items-center gap-1.5 font-semibold">
                    <span className="w-1.5 h-1.5 bg-cyan-500 dark:bg-cyan-400 rounded-full animate-pulse" />
                    escribiendo...
                  </span>
                )}
              </div>
            )}
            <div className="relative w-full min-w-0">
              <div className={cn(
                'rounded-lg border border-border bg-card text-card-foreground shadow-sm relative border-l-2',
                compactMode ? 'px-2.5 py-1 text-[11.5px] leading-[1.45]' : 'px-3.5 py-2.5 text-[13px] leading-[1.55]',
                m.accentBorder,
                highlight ? 'ring-2 ring-primary shadow-md' : ''
              )}>
                {agUi.handoff && !compactMode && (
                  <ColoquioHandoffBadge handoff={agUi.handoff} />
                )}

                {thoughtContent && (
                  <ColoquioThinkingBlock thoughtContent={thoughtContent} isStreaming={isStreaming} />
                )}

                <div
                  onClick={handleThreadClick}
                  className="break-words"
                  dangerouslySetInnerHTML={{
                    __html: highlightHtmlContent(parseMarkdown(mainContent), msgSearch),
                  }}
                />

                {/* Show cursor indicator if streaming */}
                {isStreaming && (
                  <span className="inline-block w-1.5 h-3.5 bg-cyan-500 dark:bg-cyan-400 ml-1 animate-pulse align-middle" />
                )}

                {agUi.toolCalls.length > 0 && (
                  <div className="mt-2 space-y-1.5">
                    {agUi.toolCalls.map(tc => (
                      <ColoquioToolCard key={tc.id} toolCall={tc} />
                    ))}
                  </div>
                )}

                {isPlan && (
                  <ColoquioHitlCard
                    planTurn={planTurn}
                    isApproved={isApproved}
                    tasks={parsePlanTasks(mainContent)}
                    onVisualize={() => {
                      const tasks = parsePlanTasks(mainContent);
                      if (tasks.length > 0) {
                        window.dispatchEvent(
                          new CustomEvent('canvas_add_plan_diagram', {
                            detail: { tasks },
                          })
                        );
                      }
                    }}
                    onApprove={async () => {
                      if (bridge) {
                        await bridge.postColoquioMessage(selectedId!, {
                          author_id: authorId || 'jose',
                          role: 'human',
                          content: `[APPROVED] #${planTurn}`,
                          metadata: '{}',
                        });
                        fetchThread();
                      }
                    }}
                  />
                )}
                {(() => {
                  try {
                    const meta = JSON.parse(msg.metadata || '{}');
                    if (meta.attachments?.length > 0) return (
                      <div className="flex flex-wrap gap-2 mt-2 pt-2 border-t border-border">
                        {meta.attachments.map((att: { file: string; name: string }, idx: number) => {
                          const isImg = /\.(jpeg|jpg|gif|png|webp|svg)$/i.test(att.file);
                          const isPatch = /\.(patch|diff)$/i.test(att.name || att.file);
                          const url = bridge ? `${bridge.getBaseUrl()}/api/v1/ingest/files/${att.file}` : '';
                          const safeUrl = sanitizeHref(url);
                          if (isImg) {
                            return (
                              <img
                                key={idx}
                                src={safeUrl}
                                alt={att.name}
                                className="max-w-[180px] max-h-[180px] rounded border border-border cursor-pointer hover:opacity-90"
                                onClick={() => safeUrl !== '#' && window.open(safeUrl, '_blank', 'noopener,noreferrer')}
                              />
                            );
                          }
                          if (isPatch) return <PatchAttachmentViewer key={idx} url={url} name={att.name || att.file} token={bridge?.getToken()} />;
                          return (
                            <a
                              key={idx}
                              href={safeUrl}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="flex items-center gap-1.5 px-2.5 py-1.5 bg-muted border border-border rounded text-xs text-foreground hover:text-primary transition-colors"
                            >
                              <FileText className="w-3 h-3" />
                              <span className="truncate max-w-[140px]">{att.name}</span>
                            </a>
                          );
                        })}
                      </div>
                    );
                  } catch {} return null;
                })()}
              </div>
              <div className={cn('absolute top-1/2 -translate-y-1/2 flex items-center gap-0.5 bg-card border border-border shadow-sm rounded-md p-1 opacity-0 group-hover/msg:opacity-100 transition-opacity z-20',
                isHuman ? 'right-full mr-2' : 'left-full ml-2')}>
                <button
                  type="button"
                  onClick={() => handleCopy(msg.msg_id, msg.content)}
                  aria-label="Copy message"
                  title="Copy message"
                  className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:outline-none"
                >
                  {copiedId === msg.msg_id ? <Check className="w-3 h-3 text-emerald-700 dark:text-emerald-400" /> : <Copy className="w-3 h-3" />}
                </button>
                <button
                  type="button"
                  onClick={() => handleQuote(msg.author_id, msg.content)}
                  aria-label="Quote message"
                  title="Quote message"
                  className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:outline-none"
                >
                  <Quote className="w-3 h-3" />
                </button>
              </div>
            </div>
          </div>
        </div>
      </>
    );
  };

  return (
    <>
      <div className="flex items-center gap-2 sm:gap-3 px-3 sm:px-4 py-2 border-b border-border bg-muted/40 shrink-0">
        {onBackToChannels && (
          <button
            type="button"
            onClick={onBackToChannels}
            className="md:hidden p-2.5 -ml-2.5 text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded"
            title="Volver a lista de canales"
            aria-label="Volver a lista de canales"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
        )}
        <Hash className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
        <span className="text-xs font-bold text-foreground truncate">{selectedChannel?.name ?? selectedId}</span>
        <span className="text-[11px] text-muted-foreground font-mono hidden sm:inline">{allMessages.length} mensajes</span>
        <button
          type="button"
          onClick={() => setCompactMode(v => !v)}
          className={cn(
            "ml-auto text-[11px] px-2.5 py-0.5 rounded border font-semibold transition-all cursor-pointer mr-1.5",
            compactMode 
              ? "border-primary bg-primary/10 text-primary" 
              : "border-border bg-card text-muted-foreground hover:text-foreground hover:bg-muted/60"
          )}
          title="Alternar densidad compacta"
          aria-label="Alternar densidad compacta"
        >
          Compacto
        </button>
        <div className="flex items-center gap-1.5 bg-card border border-input rounded px-2 py-0.5 shadow-sm">
          <Search className="w-3 h-3 text-muted-foreground shrink-0" />
          <input
            className="w-32 bg-transparent text-[11px] text-foreground placeholder:text-muted-foreground focus:outline-none"
            placeholder="Buscar en el canal..."
            aria-label="Buscar mensajes en el canal"
            value={msgSearch}
            onChange={e => setMsgSearch(e.target.value)}
          />
          {msgSearch && (
            <>
              <span className="text-[10px] text-primary font-mono font-semibold">{filteredMessages.length}/{allMessages.length}</span>
              <button
                type="button"
                onClick={() => setMsgSearch('')}
                aria-label="Borrar búsqueda"
                className="text-muted-foreground hover:text-foreground cursor-pointer focus-visible:outline-none"
              >
                <X className="w-3 h-3" />
              </button>
            </>
          )}
        </div>
      </div>
      {allMessages.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 p-6 text-center">
          <p className="text-xs font-bold text-foreground">Canal sin mensajes</p>
          <p className="text-[11px] text-muted-foreground max-w-sm">Escriba el primer mensaje en el campo inferior para iniciar la deliberación en este canal.</p>
        </div>
      ) : (
        <VirtualMessageList
          messages={filteredMessages}
          renderItem={renderMessage}
          scrollToBottom={needsScrollToBottom}
          onScrollToBottomDone={() => setNeedsScrollToBottom(false)}
          isAtBottom={isAtBottom}
          hasMoreOlder={!msgSearch && hasMoreOlder}
          loadingOlder={loadingOlder}
          onLoadOlder={onLoadOlder}
          jumpTurn={jumpTurn}
          onJumpTurnDone={() => setJumpTurn(null)}
        />
      )}
      {Object.keys(typingStatuses).length > 0 && (
        <div className="px-3 py-1 text-[10px] text-muted-foreground flex items-center gap-2 shrink-0 bg-muted/50 border-t border-border">
          <div className="flex flex-col gap-0.5">
            {Object.entries(typingStatuses).map(([agent, val]) => (
              <span key={agent} className="font-mono text-[10px]"><strong className="text-primary">@{agent}</strong>: <span className="italic">{val.status}</span></span>
            ))}
          </div>
        </div>
      )}
      <div ref={composerRef} className="border-t border-border px-3 py-2.5 bg-muted/30 shrink-0 font-sans">
        <div className="flex items-center gap-2 mb-1.5">
          <span className="text-[11px] font-semibold text-foreground shrink-0">Autor:</span>
          <input className="w-24 bg-card border border-input rounded px-2 py-0.5 text-[11px] font-mono text-foreground focus:outline-none shadow-sm"
            value={authorId} onChange={e => setAuthorId(e.target.value)} placeholder="jose" />
          <button
            type="button"
            onClick={() => setShowTemplates(v => !v)}
            className="ml-auto text-[11px] font-semibold text-primary hover:underline flex items-center gap-1 cursor-pointer px-1 py-0.5 rounded"
          >
            {showTemplates ? 'Ocultar plantillas' : 'Plantillas rápidas'}
          </button>
        </div>
        {showTemplates && (
          <div className="flex flex-wrap gap-1 mb-1.5">
            {QUICK_TEMPLATES.map(t => (
              <button
                type="button"
                key={t.label}
                onClick={() => { setDraft(t.text); setShowTemplates(false); }}
                className="text-[11px] font-semibold bg-card hover:bg-muted text-primary px-2.5 py-0.5 rounded border border-border shadow-sm transition-all cursor-pointer"
              >
                {t.label}
              </button>
            ))}
          </div>
        )}
        {attachments.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mb-1.5">
            {attachments.map((att, i) => (
              <div key={i} className="flex items-center gap-1.5 px-2 py-0.5 bg-card border border-border rounded text-[11px] text-foreground shadow-sm">
                <Paperclip className="w-2.5 h-2.5" />
                <span className="truncate max-w-[120px]">{att.name}</span>
                <button
                  type="button"
                  onClick={() => setAttachments(p => p.filter((_, j) => j !== i))}
                  aria-label={`Quitar adjunto ${att.name}`}
                  className="text-muted-foreground hover:text-rose-500 ml-1 p-0.5 rounded cursor-pointer"
                >
                  <X className="w-2.5 h-2.5" />
                </button>
              </div>
            ))}
          </div>
        )}
        <div className="flex gap-2 items-end">
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            disabled={uploading}
            aria-label="Adjuntar archivo"
            title="Adjuntar archivo"
            className="px-2.5 py-2 bg-card hover:bg-muted disabled:opacity-40 rounded border border-border text-foreground text-[11px] font-semibold shrink-0 cursor-pointer shadow-sm transition-colors"
          >
            <Paperclip className="w-3.5 h-3.5" />
          </button>
          <input type="file" ref={fileInputRef} className="hidden" multiple onChange={e => e.target.files && handleFileUpload(e.target.files)} />
          <textarea
            className={cn('flex-1 bg-card border rounded-md px-3 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none resize-none font-sans shadow-sm',
              isDragging ? 'border-primary bg-primary/5' : 'border-input')}
            rows={2} placeholder="Escriba un mensaje... Mayús+Intro para nueva línea" value={draft}
            onChange={e => {
              setDraft(e.target.value);
              const now = Date.now();
              if (now - lastTypingTimeRef.current > 2000 && selectedId && bridge) {
                lastTypingTimeRef.current = now;
                bridge.postColoquioTyping(selectedId, authorId || 'jose', 'escribiendo...');
              }
            }}
            onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); postMessage(); } }}
            onDragOver={e => { e.preventDefault(); setIsDragging(true); }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={e => { e.preventDefault(); setIsDragging(false); if (e.dataTransfer.files?.length) handleFileUpload(e.dataTransfer.files); }}
          />
          <button
            type="button"
            onClick={postMessage}
            disabled={posting || (!draft.trim() && !attachments.length)}
            aria-label="Enviar mensaje"
            className="px-4 py-2 bg-primary hover:bg-primary/90 disabled:opacity-40 rounded-md text-primary-foreground font-semibold border border-primary text-xs shrink-0 cursor-pointer shadow-sm transition-colors"
          >
            Enviar
          </button>
        </div>
        <div className="flex justify-between items-center mt-1.5 text-[10px] text-muted-foreground">
          <span>Intro para enviar, Mayús+Intro para salto de línea</span>
          <span className="flex items-center gap-1"><Clock className="w-2.5 h-2.5" /> Actualización cada 5 s</span>
        </div>
      </div>
    </>
  );
}
