import {
  MessageSquare, Sparkles, Hash, Search, X, Paperclip, Loader2, Send, Clock,
  Check, Copy, Quote, FileText, Network, ChevronDown, ChevronRight
} from 'lucide-react';
import { useRef, useLayoutEffect, useCallback, useMemo, useState } from 'react';
import { cn } from '../lib/utils';
import { NexusBridge } from '../lib/nexus-bridge';

import { ColoquioChannel, ColoquioMessage } from './coloquio-types';

interface VMLProps {
  messages: ColoquioMessage[];
  renderItem: (m: ColoquioMessage, p: ColoquioMessage | null) => React.ReactNode;
  scrollToBottom: boolean;
  onScrollToBottomDone: () => void;
  isAtBottom: React.MutableRefObject<boolean>;
}

function VirtualMessageList({ messages, renderItem, scrollToBottom, onScrollToBottomDone, isAtBottom }: VMLProps) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!scrollToBottom) return;
    requestAnimationFrame(() => {
      if (ref.current) ref.current.scrollTop = ref.current.scrollHeight;
      onScrollToBottomDone();
    });
  }, [scrollToBottom, onScrollToBottomDone]);

  const onScroll = useCallback(() => {
    const c = ref.current;
    if (!c) return;
    isAtBottom.current = c.scrollHeight - c.scrollTop - c.clientHeight < 100;
  }, [isAtBottom]);

  return (
    <div ref={ref} onScroll={onScroll} className="flex-1 overflow-y-auto px-4 py-3">
      {messages.map((msg, i) => (
        <div key={msg.msg_id} id={`msg-turn-${msg.turn}`}>{renderItem(msg, i > 0 ? messages[i - 1] : null)}</div>
      ))}
    </div>
  );
}

const AGENT_META: Record<string, { color: string; bg: string; border: string; initial: string }> = {
  user: { color: 'text-emerald-300', bg: 'bg-emerald-950/40', border: 'border-emerald-500/30', initial: 'U' },
  human: { color: 'text-emerald-300', bg: 'bg-emerald-950/40', border: 'border-emerald-500/30', initial: 'U' },
};
const DA = { color: 'text-slate-300', bg: 'bg-slate-800/60', border: 'border-slate-600/30', initial: '?' };
function agentMeta(id: string) {
  const cleanId = id.toLowerCase();
  const k = Object.keys(AGENT_META).find(key => cleanId.includes(key));
  if (k) return AGENT_META[k];
  
  let hash = 0;
  for (let i = 0; i < cleanId.length; i++) {
    hash = cleanId.charCodeAt(i) + ((hash << 5) - hash);
  }
  const colors = [
    { color: 'text-violet-300', bg: 'bg-violet-950/40', border: 'border-violet-500/30' },
    { color: 'text-blue-300', bg: 'bg-blue-950/40', border: 'border-blue-500/30' },
    { color: 'text-amber-300', bg: 'bg-amber-950/40', border: 'border-indigo-500/30' },
    { color: 'text-orange-300', bg: 'bg-orange-950/40', border: 'border-orange-500/30' },
    { color: 'text-pink-300', bg: 'bg-pink-950/40', border: 'border-pink-500/30' },
    { color: 'text-indigo-300', bg: 'bg-indigo-950/40', border: 'border-indigo-500/30' },
    { color: 'text-teal-300', bg: 'bg-teal-950/40', border: 'border-teal-500/30' },
    { color: 'text-lime-300', bg: 'bg-lime-950/40', border: 'border-lime-500/30' },
    { color: 'text-rose-300', bg: 'bg-rose-950/40', border: 'border-rose-500/30' },
  ];
  const choice = colors[Math.abs(hash) % colors.length];
  return { ...choice, initial: id[0]?.toUpperCase() ?? '?' };
}

const QUICK_TEMPLATES = [
  { label: '\u{1F9F9} Graph', text: 'Consolidate SilvaDB knowledge graph' },
  { label: '\u{1F4CA} Status', text: 'Give me a fleet status report' },
  { label: '\u{1F9EC} Reindex', text: 'Reindex SilvaDB and regenerate embeddings' },
];

function parseMarkdown(text: string): string {
  let h = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  h = h.replace(/```(\w*)\n([\s\S]*?)\n```/g, (_, lang, code) =>
    `<pre class="bg-slate-950/80 border border-slate-700/50 rounded-lg p-3 my-2 font-mono text-[10px] overflow-x-auto text-slate-300">` +
    `<div class="text-[8px] text-slate-500 uppercase tracking-widest font-bold mb-1 border-b border-slate-800 pb-1">${lang || 'code'}</div>` +
    `<code>${code}</code></pre>`
  );
  h = h.replace(/`([^`]+)`/g, '<code class="bg-slate-950 border border-slate-700/50 px-1 py-0.5 rounded font-mono text-[10px] text-indigo-300">$1</code>');
  h = h.replace(/\*\*([^*]+)\*\*/g, '<strong class="text-slate-100">$1</strong>');
  h = h.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  h = h.replace(/_([^_]+)_/g, '<em>$1</em>');
  h = h.replace(/(?:^|\n)&gt;\s?(.*)/g, (_, q) =>
    `<blockquote class="border-l-2 border-indigo-500/40 pl-3 italic text-slate-400 my-1.5">${q}</blockquote>`
  );
  h = h.replace(/(?:^|\n)[-*]\s(.*)/g, (_, item) =>
    `<li class="list-disc ml-4 my-0.5 text-slate-300">${item}</li>`
  );
  h = h.replace(/(^|[\s,;().!?])#(\d{1,4})\b/g,
    '$1<span class="cursor-pointer text-cyan-400 hover:text-cyan-300 underline font-mono font-semibold" data-jump-turn="$2">#$2</span>'
  );
  h = h.replace(/(^|[\s,;()>.!?])@([A-Za-z0-9_][A-Za-z0-9_-]*)/g,
    '$1<span class="text-cyan-300 bg-cyan-950/50 border border-cyan-800/40 rounded px-1 font-semibold">@$2</span>'
  );
  return h;
}

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
}: ColoquioMessagesPanelProps) {
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [highlightedTurn, setHighlightedTurn] = useState<number | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const lastTypingTimeRef = useRef<number>(0);

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
    msgSearch.trim() ? allMessages.filter(m => m.content.toLowerCase().includes(msgSearch.toLowerCase())) : allMessages,
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

  const scrollToTurn = (turn: number) => {
    const el = document.getElementById(`msg-turn-${turn}`);
    if (el) {
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
      setHighlightedTurn(turn);
      setTimeout(() => setHighlightedTurn(null), 2000);
    }
  };

  const handleThreadClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const t = (e.target as HTMLElement).getAttribute('data-jump-turn');
    if (t) scrollToTurn(parseInt(t, 10));
  };

  const renderDateDiv = (msg: ColoquioMessage, prev: ColoquioMessage | null) => {
    const d = new Date(msg.created_at * 1000).toDateString();
    if (prev && new Date(prev.created_at * 1000).toDateString() === d) return null;
    const today = new Date().toDateString(), yday = new Date(Date.now() - 86400000).toDateString();
    let label = new Date(msg.created_at * 1000).toLocaleDateString('en', { weekday: 'long', month: 'long', day: 'numeric' });
    if (d === today) label = 'Today'; else if (d === yday) label = 'Yesterday';
    return (
      <div className="flex items-center justify-center my-5">
        <div className="h-px bg-slate-800 flex-1" />
        <span className="text-[9px] font-bold text-slate-500 uppercase tracking-widest px-3 bg-[#0d1017]">{label}</span>
        <div className="h-px bg-slate-800 flex-1" />
      </div>
    );
  };

  const renderMessage = (msg: ColoquioMessage, prev: ColoquioMessage | null) => {
    const isHuman = msg.role === 'human';
    const m = agentMeta(msg.author_id);
    const cont = !!(prev && prev.author_id === msg.author_id && msg.created_at - prev.created_at < 120);
    const highlight = highlightedTurn === msg.turn;
    const escapedSearch = msgSearch.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

    // Detect if this message is actively streaming
    const isStreaming = activeStreams[msg.msg_id] !== undefined;

    let mainContent = msg.content;
    let thoughtContent = "";
    const thoughtRegex = /\[PENSAMIENTO\]([\s\S]*?)(?:\[FIN PENSAMIENTO\]|$)/i;
    const thoughtMatch = mainContent.match(thoughtRegex);
    if (thoughtMatch) {
      thoughtContent = thoughtMatch[1].trim();
      mainContent = mainContent.replace(thoughtRegex, "").trim();
    }

    const isPlan = msg.content.toLowerCase().includes("[plan]");
    const planTurn = msg.turn;
    const isApproved = messages.some(
      otherMsg => otherMsg.turn > planTurn && 
                  otherMsg.content.toLowerCase().includes(`[aprobado] #${planTurn}`)
    );

    return (
      <>
        {renderDateDiv(msg, prev)}
        <div className={cn(
          'flex group/msg rounded-lg transition-colors hover:bg-white/[0.02]',
          compactMode ? 'gap-1.5 py-0.5 px-1' : 'gap-2.5 py-1 px-2.5',
          isHuman ? 'flex-row-reverse' : ''
        )}>
          <div className={cn('shrink-0 mt-0.5', compactMode ? 'w-6' : 'w-8')}>
            {!cont && (
              <div className={cn(
                'rounded-full flex items-center justify-center font-bold border',
                compactMode ? 'w-6 h-6 text-[10px]' : 'w-8 h-8 text-xs',
                m.bg, m.color, m.border
              )}>
                {m.initial}
              </div>
            )}
          </div>
          <div className={cn('flex flex-col gap-0.5 max-w-[85%]', isHuman ? 'items-end' : 'items-start')}>
            {!cont && (
              <div className={cn('flex items-center gap-2', isHuman ? 'flex-row-reverse' : '')}>
                <span className={cn('text-[11px] font-semibold', m.color)}>{isHuman ? msg.author_id : `@${msg.author_id}`}</span>
                <button onClick={() => scrollToTurn(msg.turn)} className="text-[9px] text-slate-600 font-mono hover:text-cyan-400 transition-colors">#{msg.turn}</button>
                <span className="text-[9px] text-slate-600">{fmtTime(msg.created_at)}</span>
                {isStreaming && (
                  <span className="text-[9px] text-violet-400 bg-violet-950/40 border border-violet-800/40 px-1 py-0.25 rounded flex items-center gap-1">
                    <span className="w-1.5 h-1.5 bg-violet-500 rounded-full animate-pulse" />
                    typing...
                  </span>
                )}
              </div>
            )}
            <div className="relative w-full">
              <div className={cn(
                'rounded-2xl shadow-sm leading-relaxed relative',
                compactMode ? 'px-2 py-1 text-[11px]' : 'px-3.5 py-2 text-[12px]',
                isHuman ? 'bg-emerald-900/30 border border-emerald-700/30 text-emerald-50 rounded-tr-sm'
                        : `${m.bg} border ${m.border} text-slate-100 rounded-tl-sm`,
                highlight ? 'ring-2 ring-cyan-400 shadow-[0_0_18px_rgba(34,211,238,0.4)]' : ''
              )}>
                <div onClick={handleThreadClick} dangerouslySetInnerHTML={{
                  __html: msgSearch.trim()
                    ? parseMarkdown(mainContent).replace(new RegExp(`(${escapedSearch})`, 'gi'), '<mark class="bg-amber-400/30 text-amber-200 rounded px-0.5">$1</mark>')
                    : parseMarkdown(mainContent)
                }} />

                {/* Show cursor indicator if streaming */}
                {isStreaming && (
                  <span className="inline-block w-1.5 h-3.5 bg-violet-400 ml-1 animate-pulse align-middle" />
                )}

                {thoughtContent && (
                  <details className="group/details bg-slate-950/40 border border-indigo-950/30 rounded-xl p-2.5 my-2 text-[11px] text-slate-400 cursor-pointer" open>
                    <summary className="font-semibold text-indigo-400/80 hover:text-indigo-300 transition-colors list-none flex items-center gap-1.5 focus:outline-none">
                      <span className="w-1.5 h-1.5 rounded-full bg-indigo-500 animate-pulse" />
                      🧠 Reasoning
                    </summary>
                    <div className="mt-2 pl-3 border-l border-indigo-500/20 whitespace-pre-wrap leading-relaxed text-slate-400/90 font-mono text-[10px] select-text cursor-text" dangerouslySetInnerHTML={{
                      __html: parseMarkdown(thoughtContent)
                    }} />
                  </details>
                )}

                {isPlan && !isApproved && (
                  <div className="mt-3.5 pt-2.5 border-t border-slate-700/30 flex items-center justify-between gap-3">
                    <span className="text-[10px] text-amber-400 font-semibold flex items-center gap-1">
                      <span className="w-1.5 h-1.5 rounded-full bg-amber-500 animate-ping" />
                      Plan pending human approval
                    </span>
                    <div className="flex gap-2">
                      <button
                        onClick={() => {
                          const tasks = parsePlanTasks(mainContent);
                          if (tasks.length > 0) {
                            window.dispatchEvent(new CustomEvent('canvas_add_plan_diagram', {
                              detail: { tasks }
                            }));
                          }
                        }}
                        className="px-3 py-1 bg-violet-600 hover:bg-violet-500 text-white font-bold text-[10px] rounded-lg shadow-lg hover:shadow-violet-500/20 transition-all flex items-center gap-1 cursor-pointer"
                      >
                        <Network className="w-3 h-3" /> Visualize on Blackboard
                      </button>
                      <button
                        onClick={async () => {
                          if (bridge) {
                            await bridge.postColoquioMessage(selectedId!, {
                              author_id: authorId || 'user',
                              role: 'human',
                              content: `[APPROVED] #${planTurn}`,
                              metadata: '{}'
                            });
                            fetchThread();
                          }
                        }}
                        className="px-3 py-1 bg-amber-500 hover:bg-amber-400 text-slate-950 font-bold text-[10px] rounded-lg shadow-lg hover:shadow-amber-500/20 transition-all flex items-center gap-1 cursor-pointer"
                      >
                        <Check className="w-3 h-3 stroke-[3]" /> Approve & Start
                      </button>
                    </div>
                  </div>
                )}

                {isPlan && isApproved && (
                  <div className="mt-3.5 pt-2.5 border-t border-slate-700/30 flex items-center justify-between gap-3">
                    <div className="flex items-center gap-2 text-[10px] text-emerald-400 font-semibold">
                      <Check className="w-3.5 h-3.5 stroke-[3]" />
                      Plan approved by human
                    </div>
                    <button
                      onClick={() => {
                        const tasks = parsePlanTasks(mainContent);
                        if (tasks.length > 0) {
                          window.dispatchEvent(new CustomEvent('canvas_add_plan_diagram', {
                            detail: { tasks }
                          }));
                        }
                      }}
                      className="px-3 py-1 bg-violet-600/40 hover:bg-violet-500/60 text-slate-200 font-bold text-[10px] rounded-lg shadow-lg transition-all flex items-center gap-1 border border-violet-500/30 cursor-pointer"
                    >
                      <Network className="w-3 h-3" /> Visualize on Blackboard
                    </button>
                  </div>
                )}
                {(() => {
                  try {
                    const meta = JSON.parse(msg.metadata || '{}');
                    if (meta.attachments?.length > 0) return (
                      <div className="flex flex-wrap gap-2 mt-2 pt-2 border-t border-slate-700/30">
                        {meta.attachments.map((att: any, idx: number) => {
                          const isImg = /\.(jpeg|jpg|gif|png|webp|svg)$/i.test(att.file);
                          const url = bridge ? `${bridge.getBaseUrl()}/api/v1/ingest/files/${att.file}` : '';
                          if (isImg) return <img key={idx} src={url} alt={att.name} className="max-w-[180px] max-h-[180px] rounded border border-slate-700 cursor-pointer hover:opacity-90" onClick={() => window.open(url, '_blank')} />;
                          return <a key={idx} href={url} target="_blank" rel="noopener noreferrer" className="flex items-center gap-1.5 px-2.5 py-1.5 bg-slate-800 border border-slate-700 rounded text-xs text-slate-300 hover:text-cyan-400 transition-colors"><FileText className="w-3 h-3" /><span className="truncate max-w-[140px]">{att.name}</span></a>;
                        })}
                      </div>
                    );
                  } catch {} return null;
                })()}
              </div>
              <div className={cn('absolute top-1/2 -translate-y-1/2 flex items-center gap-0.5 bg-slate-900 border border-slate-700 rounded-lg p-1 shadow-xl opacity-0 group-hover/msg:opacity-100 transition-opacity z-20',
                isHuman ? 'right-full mr-2' : 'left-full ml-2')}>
                <button onClick={() => handleCopy(msg.msg_id, msg.content)} className="p-1.5 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 transition-colors" title="Copy">
                  {copiedId === msg.msg_id ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                </button>
                <button onClick={() => handleQuote(msg.author_id, msg.content)} className="p-1.5 hover:bg-slate-800 rounded text-slate-400 hover:text-slate-200 transition-colors" title="Citar">
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
      <div className="flex items-center gap-3 px-4 py-2 border-b border-slate-700/60 bg-slate-900/60 shrink-0">
        <Hash className="w-4 h-4 text-slate-500" />
        <span className="text-sm font-semibold text-slate-200">{selectedChannel?.name ?? selectedId}</span>
        <span className="text-[10px] text-slate-600 font-mono">{allMessages.length} messages</span>
        <button
          onClick={() => setCompactMode(v => !v)}
          className={cn(
            "ml-auto text-[10px] px-2 py-1 rounded-lg border font-bold transition-all cursor-pointer mr-1.5",
            compactMode 
              ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-400 shadow-[0_0_8px_rgba(16,185,129,0.15)]" 
              : "border-slate-700 text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
          )}
          title="Toggle compact density"
        >
          Compact
        </button>
        <div className="flex items-center gap-2 bg-slate-800/60 border border-slate-700/60 rounded-lg px-2.5 py-1">
          <Search className="w-3 h-3 text-slate-500 shrink-0" />
          <input className="w-32 bg-transparent text-[11px] text-slate-200 placeholder-slate-600 focus:outline-none"
            placeholder="Search thread..." value={msgSearch} onChange={e => setMsgSearch(e.target.value)} />
          {msgSearch && (
            <>
              <span className="text-[9px] text-amber-400 font-mono">{filteredMessages.length}/{allMessages.length}</span>
              <button onClick={() => setMsgSearch('')} className="text-slate-500 hover:text-slate-300"><X className="w-3 h-3" /></button>
            </>
          )}
        </div>
      </div>
      {allMessages.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center gap-2 text-slate-600">
          <Sparkles className="w-6 h-6 opacity-20 animate-pulse" /><p className="text-[12px]">Empty channel</p>
        </div>
      ) : (
        <VirtualMessageList messages={filteredMessages} renderItem={renderMessage}
          scrollToBottom={needsScrollToBottom} onScrollToBottomDone={() => setNeedsScrollToBottom(false)} isAtBottom={isAtBottom} />
      )}
      {Object.keys(typingStatuses).length > 0 && (
        <div className="px-5 py-1.5 text-[10px] text-slate-500 flex items-center gap-2 shrink-0 bg-slate-950/20 border-t border-slate-900">
          <span className="flex gap-0.5">{[0, 150, 300].map(d => <span key={d} className="w-1.5 h-1.5 rounded-full bg-indigo-500 animate-bounce" style={{ animationDelay: `${d}ms` }} />)}</span>
          <div className="flex flex-col gap-0.5">
            {Object.entries(typingStatuses).map(([agent, val]) => (
              <span key={agent} className="font-mono text-[9px]"><strong className="text-slate-400">@{agent}</strong>: <span className="italic text-slate-500">{val.status}</span></span>
            ))}
          </div>
        </div>
      )}
      <div className="border-t border-slate-700/60 px-4 py-3 bg-slate-900/80 shrink-0">
        <div className="flex items-center gap-3 mb-2">
          <span className="text-[9px] text-slate-500 font-bold uppercase tracking-wider shrink-0">As:</span>
          <input className="w-24 bg-slate-800/80 border border-slate-700/60 rounded px-2 py-0.5 text-[11px] font-mono text-slate-200 focus:outline-none focus:border-indigo-600 transition-colors"
            value={authorId} onChange={e => setAuthorId(e.target.value)} placeholder="user" />
          <button onClick={() => setShowTemplates(v => !v)} className="ml-auto text-[10px] text-slate-500 hover:text-slate-300 flex items-center gap-1 transition-colors">
            <Sparkles className="w-3 h-3" />{showTemplates ? 'Hide' : 'Templates'}
          </button>
        </div>
        {showTemplates && (
          <div className="flex flex-wrap gap-1.5 mb-2">
            {QUICK_TEMPLATES.map(t => (
              <button key={t.label} onClick={() => { setDraft(t.text); setShowTemplates(false); }}
                className="text-[9px] font-bold bg-slate-800/60 hover:bg-indigo-950/30 text-slate-400 hover:text-indigo-300 px-2 py-1 rounded border border-slate-800 hover:border-indigo-500/30 transition-all">{t.label}</button>
            ))}
          </div>
        )}
        {attachments.length > 0 && (
          <div className="flex flex-wrap gap-1.5 mb-2">
            {attachments.map((att, i) => (
              <div key={i} className="flex items-center gap-1.5 px-2 py-1 bg-slate-800 border border-slate-700 rounded text-[11px] text-slate-300">
                <Paperclip className="w-2.5 h-2.5" />
                <span className="truncate max-w-[120px]">{att.name}</span>
                <button onClick={() => setAttachments(p => p.filter((_, j) => j !== i))} className="text-slate-500 hover:text-rose-400 ml-1"><X className="w-2.5 h-2.5" /></button>
              </div>
            ))}
          </div>
        )}
        <div className="flex gap-2 items-end">
          <button onClick={() => fileInputRef.current?.click()} disabled={uploading}
            className="p-2.5 bg-slate-800 hover:bg-slate-700 disabled:opacity-40 rounded-xl text-slate-400 hover:text-slate-200 transition-all shrink-0" title="Adjuntar">
            {uploading ? <Loader2 className="w-4 h-4 animate-spin" /> : <Paperclip className="w-4 h-4" />}
          </button>
          <input type="file" ref={fileInputRef} className="hidden" multiple onChange={e => e.target.files && handleFileUpload(e.target.files)} />
          <textarea
            className={cn('flex-1 bg-slate-800/60 border rounded-xl px-3.5 py-2.5 text-[12px] text-slate-100 placeholder-slate-600 focus:outline-none focus:border-indigo-600/60 resize-none font-sans transition-colors',
              isDragging ? 'border-cyan-500 bg-cyan-950/20 ring-2 ring-cyan-500/30' : 'border-slate-700/60')}
            rows={3} placeholder="Type a message... Shift+Enter for new line" value={draft}
            onChange={e => {
              setDraft(e.target.value);
              const now = Date.now();
              if (now - lastTypingTimeRef.current > 2000 && selectedId && bridge) {
                lastTypingTimeRef.current = now;
                bridge.postColoquioTyping(selectedId, authorId || 'user', 'typing...');
              }
            }}
            onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); postMessage(); } }}
            onDragOver={e => { e.preventDefault(); setIsDragging(true); }}
            onDragLeave={() => setIsDragging(false)}
            onDrop={e => { e.preventDefault(); setIsDragging(false); if (e.dataTransfer.files?.length) handleFileUpload(e.dataTransfer.files); }}
          />
          <button onClick={postMessage} disabled={posting || (!draft.trim() && !attachments.length)}
            className="p-2.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 rounded-xl text-white transition-all shrink-0 shadow-lg shadow-indigo-900/30">
            <Send className="w-4 h-4" />
          </button>
        </div>
        <div className="flex justify-between items-center mt-1.5 text-[9px] text-slate-700">
          <span>Enter to send, Shift+Enter for new line</span>
          <span className="flex items-center gap-1"><Clock className="w-2.5 h-2.5" /> Auto-refresh 5s</span>
        </div>
      </div>
    </>
  );
}
