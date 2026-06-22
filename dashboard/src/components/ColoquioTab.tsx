import { MessageSquare, Sparkles, Users, Check } from 'lucide-react';
import { useMemo, useState, useRef, useCallback, useEffect } from 'react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { useNexus } from '../hooks/useNexus';
import { ColoquioChannel, ColoquioMessage } from './coloquio-types';
import { ColoquioChannelsPanel } from './ColoquioChannelsPanel';
import { ColoquioMessagesPanel } from './ColoquioMessagesPanel';
import { ColoquioCanvasWorkspace } from './ColoquioCanvasWorkspace';
import { ColoquioAgentsPanel } from './ColoquioAgentsPanel';

interface ColoquioTabProps {
  bridge: NexusBridge | null;
}

export function ColoquioTab({ bridge }: ColoquioTabProps) {
  const { events } = useNexus();
  const [channels, setChannels] = useState<ColoquioChannel[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ColoquioMessage[]>([]);
  
  // Real-time multi-agent streaming buffer mapping msg_id -> ColoquioMessage
  const [activeStreams, setActiveStreams] = useState<Record<string, ColoquioMessage>>({});

  const [draft, setDraft] = useState('');
  const [authorId, setAuthorId] = useState('user');
  const [posting, setPosting] = useState(false);
  const [creating, setCreating] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [msgSearch, setMsgSearch] = useState('');
  const [unreadData, setUnreadData] = useState<{ channel_id: string; unread_count: number }[]>([]);
  const [attachments, setAttachments] = useState<{ file: string; name: string }[]>([]);
  const [uploading, setUploading] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [showNewChannel, setShowNewChannel] = useState(false);
  const [newChannelId, setNewChannelId] = useState('');
  const [newChannelName, setNewChannelName] = useState('');
  const [needsScrollToBottom, setNeedsScrollToBottom] = useState(false);
  const [showAgentPanel, setShowAgentPanel] = useState(true);
  const [showCanvas, setShowCanvas] = useState(true);
  const [showTemplates, setShowTemplates] = useState(false);
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const [typingStatuses, setTypingStatuses] = useState<Record<string, { ts: number; status: string }>>({});
  const [chatWidth, setChatWidth] = useState(650);
  const [compactMode, setCompactMode] = useState(false);

  const isAtBottom = useRef(true);
  const lastChRef = useRef<string | null>(null);
  const lastCntRef = useRef(0);
  const isResizingRef = useRef(false);
  const splitContainerRef = useRef<HTMLDivElement>(null);

  const startResizing = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isResizingRef.current = true;
    document.body.style.cursor = 'col-resize';
  }, []);

  const stopResizing = useCallback(() => {
    isResizingRef.current = false;
    document.body.style.cursor = '';
  }, []);

  const resize = useCallback((e: MouseEvent) => {
    if (!isResizingRef.current || !splitContainerRef.current) return;
    const rect = splitContainerRef.current.getBoundingClientRect();
    const newWidth = e.clientX - rect.left;
    const maxChatWidth = rect.width - 320;
    const finalWidth = Math.max(320, Math.min(maxChatWidth, newWidth));
    setChatWidth(finalWidth);
  }, []);

  useEffect(() => {
    window.addEventListener('mousemove', resize);
    window.addEventListener('mouseup', stopResizing);
    return () => {
      window.removeEventListener('mousemove', resize);
      window.removeEventListener('mouseup', stopResizing);
    };
  }, [resize, stopResizing]);

  const unreadMap = useMemo(() => new Map(unreadData.map(c => [c.channel_id, c.unread_count])), [unreadData]);
  const totalUnread = useMemo(() => unreadData.reduce((s, c) => s + c.unread_count, 0), [unreadData]);
  
  const agentPresence = useMemo(() => {
    const map = new Map<string, number>();
    messages.forEach(m => {
      if ((map.get(m.author_id) ?? 0) < m.created_at) {
        map.set(m.author_id, m.created_at);
      }
    });
    
    // Default online status calculation
    const nowSecs = Date.now() / 1000;
    return Array.from(map.entries())
      .sort((a, b) => b[1] - a[1])
      .map(([id, lastSeen]) => {
        const d = nowSecs - lastSeen;
        const status: 'online' | 'idle' | 'offline' = d < 300 ? 'online' : d < 1800 ? 'idle' : 'offline';
        return { id, lastSeen, status };
      });
  }, [messages]);

  const selectedChannel = useMemo(() => channels.find(c => c.channel_id === selectedId), [channels, selectedId]);

  const fetchChannels = useCallback(async () => {
    if (!bridge) return;
    try {
      const d = await bridge.getColoquioChannels();
      const chs = d.channels ?? [];
      setChannels(chs);
      if (!selectedId && chs.length > 0) setSelectedId(chs[0].channel_id);
    } catch {}
  }, [bridge, selectedId]);

  const fetchThread = useCallback(async () => {
    if (!bridge || !selectedId) return;
    try {
      const d = await bridge.getColoquioThread(selectedId);
      const nm = d.messages ?? [];
      setMessages(prev => {
        if (prev.length === nm.length && prev.length > 0 && prev[prev.length - 1].msg_id === nm[nm.length - 1].msg_id) return prev;
        return nm;
      });
    } catch {}
  }, [bridge, selectedId]);

  const fetchUnread = useCallback(async () => {
    if (!bridge) return;
    try {
      const d = await bridge.getColoquioUnread('user');
      setUnreadData(d.channels ?? []);
    } catch {}
  }, [bridge]);

  useEffect(() => {
    fetchChannels();
  }, [fetchChannels]);

  useEffect(() => {
    fetchThread();
  }, [fetchThread, selectedId]);

  // Keep mutable references of polling functions to prevent stale closure bugs inside useEffect intervals
  const fetchThreadRef = useRef(fetchThread);
  const fetchChannelsRef = useRef(fetchChannels);
  const fetchUnreadRef = useRef(fetchUnread);

  useEffect(() => {
    fetchThreadRef.current = fetchThread;
  }, [fetchThread]);

  useEffect(() => {
    fetchChannelsRef.current = fetchChannels;
  }, [fetchChannels]);

  useEffect(() => {
    fetchUnreadRef.current = fetchUnread;
  }, [fetchUnread]);

  // Stable polling effect
  useEffect(() => {
    fetchUnreadRef.current();
    const unreadInterval = setInterval(() => {
      if (document.visibilityState !== 'hidden') {
        fetchUnreadRef.current();
      }
    }, 5000);

    const threadInterval = setInterval(() => {
      if (document.visibilityState !== 'hidden') {
        fetchThreadRef.current();
        fetchChannelsRef.current();
      }
    }, 5000);

    return () => {
      clearInterval(unreadInterval);
      clearInterval(threadInterval);
    };
  }, []);

  // Handle SSE Real-time events
  useEffect(() => {
    if (!events.length) return;
    const ev = events[0];

    if (ev.type === 'coloquio:new_turn') {
      const d = ev.data as any;
      if (d.channel_id === selectedId) {
        fetchThread();
        // Remove completed streaming frames if database registers new turn
        setActiveStreams(prev => {
          const next = { ...prev };
          Object.keys(next).forEach(k => {
            if (next[k].author_id === d.author_id) {
              delete next[k];
            }
          });
          return next;
        });
      }
      fetchChannels();
    }
    if (ev.type === 'coloquio:typing') {
      const d = ev.data as any;
      setTypingStatuses(p => ({
        ...p,
        [d.author_id]: { ts: Date.now(), status: d.status || 'escribiendo...' }
      }));
    }
    if (ev.type === 'coloquio:stream') {
      const d = ev.data as any;
      if (d.channel_id === selectedId) {
        setActiveStreams(prev => {
          const existing = prev[d.msg_id];
          const content = (existing?.content ?? '') + (d.delta ?? '');

          if (d.state === 'done' || d.state === 'error') {
            const next = { ...prev };
            delete next[d.msg_id];
            fetchThread();
            return next;
          }

          return {
            ...prev,
            [d.msg_id]: {
              msg_id: d.msg_id,
              channel_id: d.channel_id,
              author_id: d.author_id,
              role: d.role || 'agent',
              content: content,
              turn: d.turn || 0,
              created_at: d.created_at || (Date.now() / 1000),
              metadata: d.metadata || '{}'
            }
          };
        });
        setNeedsScrollToBottom(true);
      }
    }
  }, [events, selectedId, fetchThread, fetchChannels]);

  // Clean up stale typing statuses
  useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      setTypingStatuses(p => {
        const next = { ...p };
        let changed = false;
        for (const [k, v] of Object.entries(next)) {
          if (now - v.ts > 4000) {
            delete next[k];
            changed = true;
          }
        }
        return changed ? next : p;
      });
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    if (!selectedId) return;
    if (selectedId !== lastChRef.current || (messages.length > lastCntRef.current && isAtBottom.current)) {
      setNeedsScrollToBottom(true);
    }
    lastChRef.current = selectedId;
    lastCntRef.current = messages.length;
  }, [messages, selectedId]);

  const selectChannel = (id: string) => {
    setSelectedId(id);
    setMsgSearch('');
    const u = unreadMap.get(id) ?? 0;
    if (u > 0 && bridge) {
      const ch = channels.find(c => c.channel_id === id);
      if (ch?.last_turn) {
        bridge.markColoquioRead(id, 'user', ch.last_turn).then(() => fetchUnread());
      }
    }
  };

  const postMessage = async () => {
    if (!bridge || !selectedId || (!draft.trim() && !attachments.length) || posting) return;
    setPosting(true);
    try {
      const meta = attachments.length ? JSON.stringify({ attachments }) : '{}';
      await bridge.postColoquioMessage(selectedId, {
        author_id: authorId || 'user',
        role: 'human',
        content: draft.trim() || ' ',
        metadata: meta
      });
      setDraft('');
      setAttachments([]);
      isAtBottom.current = true;
      await fetchThread();
    } catch {} finally {
      setPosting(false);
    }
  };

  const createChannel = async () => {
    if (!bridge || !newChannelId.trim() || creating) return;
    setCreating(true);
    try {
      await bridge.createColoquioChannel(newChannelId.trim(), newChannelName.trim() || newChannelId.trim());
      const id = newChannelId.trim();
      setNewChannelId('');
      setNewChannelName('');
      setShowNewChannel(false);
      await fetchChannels();
      setSelectedId(id);
    } catch {} finally {
      setCreating(false);
    }
  };

  const deleteChannel = async (channelId: string, archive: boolean) => {
    if (!bridge) return;
    await bridge.deleteColoquioChannel(channelId, archive);
    if (selectedId === channelId) {
      setSelectedId(null);
    }
    await fetchChannels();
  };

  const markAllRead = async () => {
    if (!bridge) return;
    for (const ch of channels) {
      if (ch.last_turn > 0) {
        await bridge.markColoquioRead(ch.channel_id, 'user', ch.last_turn);
      }
    }
    await fetchUnread();
  };

  const handleFileUpload = async (files: FileList | File[]) => {
    if (!bridge || !files.length) return;
    setUploading(true);
    try {
      const next = [...attachments];
      for (const f of Array.from(files)) {
        const r = await bridge.uploadFile(f);
        if (r.status === 'ok' || r.file) {
          next.push({ file: r.file, name: r.original_name || f.name });
        }
      }
      setAttachments(next);
    } catch {} finally {
      setUploading(false);
    }
  };

  return (
    <div className="border border-slate-700/60 rounded-xl bg-[#0f1117] overflow-hidden flex flex-col" style={{ height: 'calc(100vh - 140px)' }}>
      {/* Top bar */}
      <div className="flex items-center justify-between px-4 py-2.5 border-b border-slate-700/60 bg-slate-900/80 shrink-0">
        <div className="flex items-center gap-2.5">
          <MessageSquare className="w-4 h-4 text-indigo-400" />
          <span className="text-sm font-semibold text-slate-100">Coloquio</span>
          <span className="text-[10px] text-slate-500 hidden sm:block">frontier research group chat</span>
          {totalUnread > 0 && (
            <span className="px-2 py-0.5 rounded-full bg-indigo-500/20 border border-indigo-500/40 text-indigo-300 text-[10px] font-bold">
              {totalUnread} new
            </span>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          {totalUnread > 0 && (
            <button onClick={markAllRead} className="flex items-center gap-1 text-[11px] text-slate-400 hover:text-emerald-400 px-2.5 py-1 rounded-lg border border-slate-700 hover:border-emerald-700/50 transition-all cursor-pointer">
              Todo leído
            </button>
          )}
          <button onClick={() => setShowCanvas(v => !v)}
            className={cn('p-1.5 rounded-lg border transition-all mr-1.5 cursor-pointer', showCanvas ? 'border-violet-500/40 bg-violet-500/10 text-violet-400' : 'border-slate-700 text-slate-500 hover:text-slate-300')}
            title="Lienzo Blackboard">
            <Sparkles className="w-3.5 h-3.5" />
          </button>
          <button onClick={() => setShowAgentPanel(v => !v)}
            className={cn('p-1.5 rounded-lg border transition-all cursor-pointer', showAgentPanel ? 'border-indigo-500/40 bg-indigo-500/10 text-indigo-400' : 'border-slate-700 text-slate-500 hover:text-slate-300')}>
            <Users className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* 3-panel body */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar Channels */}
        <ColoquioChannelsPanel
          channels={channels}
          selectedId={selectedId}
          searchQuery={searchQuery}
          setSearchQuery={setSearchQuery}
          collapsedGroups={collapsedGroups}
          setCollapsedGroups={setCollapsedGroups}
          unreadMap={unreadMap}
          selectChannel={selectChannel}
          showNewChannel={showNewChannel}
          setShowNewChannel={setShowNewChannel}
          newChannelId={newChannelId}
          setNewChannelId={setNewChannelId}
          newChannelName={newChannelName}
          setNewChannelName={setNewChannelName}
          createChannel={createChannel}
          creating={creating}
          onDeleteChannel={deleteChannel}
        />

        {/* Central Area: Split screen between Chat (Thread) and Canvas */}
        <div ref={splitContainerRef} className="flex-1 flex overflow-hidden">
          {/* Thread (Chat) */}
          <div
            className="flex flex-col overflow-hidden bg-[#0d1017] border-r border-slate-800"
            style={{ width: showCanvas && selectedId ? chatWidth : '100%', flexShrink: 0 }}
          >
            {!selectedId ? (
              <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-600">
                <MessageSquare className="w-10 h-10 opacity-20" />
                <p className="text-[13px]">Select a channel</p>
              </div>
            ) : (
              <ColoquioMessagesPanel
                selectedId={selectedId}
                selectedChannel={selectedChannel}
                messages={messages}
                activeStreams={activeStreams}
                msgSearch={msgSearch}
                setMsgSearch={setMsgSearch}
                compactMode={compactMode}
                setCompactMode={setCompactMode}
                needsScrollToBottom={needsScrollToBottom}
                setNeedsScrollToBottom={setNeedsScrollToBottom}
                isAtBottom={isAtBottom}
                typingStatuses={typingStatuses}
                authorId={authorId}
                setAuthorId={setAuthorId}
                showTemplates={showTemplates}
                setShowTemplates={setShowTemplates}
                attachments={attachments}
                setAttachments={setAttachments}
                uploading={uploading}
                isDragging={isDragging}
                setIsDragging={setIsDragging}
                draft={draft}
                setDraft={setDraft}
                posting={posting}
                postMessage={postMessage}
                handleFileUpload={handleFileUpload}
                bridge={bridge}
                fetchThread={fetchThread}
              />
            )}
          </div>

          {/* Resizable Divider Splitter Bar */}
          {showCanvas && selectedId && (
            <div
              onMouseDown={startResizing}
              className="w-1 bg-slate-800/85 hover:bg-indigo-500 transition-colors cursor-col-resize select-none shrink-0"
              title="Drag to resize"
            />
          )}

          {/* Canvas View */}
          {showCanvas && selectedId && (
            <div className="flex-1 flex flex-col overflow-hidden">
              <ColoquioCanvasWorkspace channelId={selectedId} messages={messages} />
            </div>
          )}
        </div>

        {/* Agent panel */}
        {showAgentPanel && (
          <ColoquioAgentsPanel
            agentPresence={agentPresence}
            typingStatuses={typingStatuses}
            selectedChannel={selectedChannel}
          />
        )}
      </div>
    </div>
  );
}
