import { Search, ChevronRight, ChevronDown, Plus, Hash, Trash2, AlertTriangle } from 'lucide-react';
import { useMemo, useState, useEffect, useRef } from 'react';
import { cn } from '../lib/utils';
import { ColoquioChannel } from './coloquio-types';

interface ColoquioChannelsPanelProps {
  channels: ColoquioChannel[];
  selectedId: string | null;
  searchQuery: string;
  setSearchQuery: (q: string) => void;
  collapsedGroups: Record<string, boolean>;
  setCollapsedGroups: React.Dispatch<React.SetStateAction<Record<string, boolean>>>;
  unreadMap: Map<string, number>;
  selectChannel: (id: string) => void;
  showNewChannel: boolean;
  setShowNewChannel: (b: boolean) => void;
  newChannelId: string;
  setNewChannelId: (q: string) => void;
  newChannelName: string;
  setNewChannelName: (q: string) => void;
  createChannel: () => void;
  creating: boolean;
  onDeleteChannel: (channelId: string, archive: boolean) => Promise<void>;
}

export function ColoquioChannelsPanel({
  channels,
  selectedId,
  searchQuery,
  setSearchQuery,
  collapsedGroups,
  setCollapsedGroups,
  unreadMap,
  selectChannel,
  showNewChannel,
  setShowNewChannel,
  newChannelId,
  setNewChannelId,
  newChannelName,
  setNewChannelName,
  createChannel,
  creating,
  onDeleteChannel
}: ColoquioChannelsPanelProps) {
  const [deleteTarget, setDeleteTarget] = useState<ColoquioChannel | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        const active = document.activeElement;
        const target = e.target as Node;
        const isInside = containerRef.current && (
          containerRef.current.contains(active) || containerRef.current.contains(target)
        );
        if (!isInside) return;

        if (deleteTarget) {
          setDeleteTarget(null);
        } else if (showNewChannel) {
          setShowNewChannel(false);
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [deleteTarget, showNewChannel, setShowNewChannel]);
  const [archiveOnDelete, setArchiveOnDelete] = useState(true);
  const [deleting, setDeleting] = useState(false);

  const filteredChannels = useMemo(() =>
    channels.filter(ch => ch.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      ch.channel_id.toLowerCase().includes(searchQuery.toLowerCase())), [channels, searchQuery]);

  const groupedChannels = useMemo(() => {
    const g: Record<string, ColoquioChannel[]> = { Daily: [], Missions: [], 'Private Debates': [], Channels: [] };
    filteredChannels.forEach(ch => {
      if (ch.channel_id.startsWith('daily-')) g['Daily'].push(ch);
      else if (ch.channel_id.startsWith('mision-')) g['Missions'].push(ch);
      else if (ch.channel_id.startsWith('private-') || ch.channel_id.startsWith('agent-')) g['Private Debates'].push(ch);
      else g['Channels'].push(ch);
    });
    return g;
  }, [filteredChannels]);

  const handleDeleteConfirm = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      await onDeleteChannel(deleteTarget.channel_id, archiveOnDelete);
      setDeleteTarget(null);
    } catch (err) {
      console.error('Error deleting channel:', err);
    } finally {
      setDeleting(false);
    }
  };

  const renderChannelItem = (ch: ColoquioChannel) => {
    const u = unreadMap.get(ch.channel_id) ?? 0;
    const active = selectedId === ch.channel_id;
    return (
      <div key={ch.channel_id} onClick={() => selectChannel(ch.channel_id)}
        className={cn('group/ch relative flex items-center gap-2 px-3 py-1.5 cursor-pointer rounded mx-1 transition-colors',
          active ? 'bg-primary/15 text-primary border-l-2 border-primary font-medium' : 'text-muted-foreground hover:text-foreground hover:bg-muted font-medium')}>
        <Hash className="w-3.5 h-3.5 shrink-0 opacity-60" />
        <span className="flex-1 text-[12px] truncate font-medium">{ch.name}</span>
        {u > 0 && <span className="px-1.5 py-0.5 rounded border border-amber-500/40 bg-amber-500/10 text-amber-600 text-[11px] font-bold leading-none">{u}</span>}
        <button
          type="button"
          onClick={e => { e.stopPropagation(); setDeleteTarget(ch); }}
          aria-label={`Delete channel ${ch.name}`}
          title="Delete channel"
          className="opacity-0 group-hover/ch:opacity-100 p-0.5 text-muted-foreground hover:text-rose-500 transition-all rounded cursor-pointer focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none focus:opacity-100"
        >
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>
    );
  };

  return (
    <div ref={containerRef} className="w-full md:w-52 shrink-0 border-r border-border flex flex-col bg-card overflow-hidden h-full font-sans">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border">
        <Search className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
        <input
          className="flex-1 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          placeholder="Search channels..."
          aria-label="Search channels"
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
        />
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {Object.entries(groupedChannels).map(([group, chs]) => (
          <div key={group}>
            <button
              type="button"
              onClick={() => setCollapsedGroups(p => ({ ...p, [group]: !p[group] }))}
              aria-label={`Toggle channel group ${group}`}
              className="w-full flex items-center gap-1.5 px-3 py-1 text-xs font-bold text-muted-foreground hover:text-foreground transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none rounded"
            >
              {collapsedGroups[group] ? <ChevronRight className="w-2.5 h-2.5" /> : <ChevronDown className="w-2.5 h-2.5" />}
              {group}<span className="ml-auto text-muted-foreground font-mono">{chs.length}</span>
            </button>
            {!collapsedGroups[group] && chs.map(renderChannelItem)}
          </div>
        ))}
        {filteredChannels.length === 0 && <div className="px-4 py-6 text-center text-xs text-muted-foreground italic">No channels</div>}
      </div>
      <div className="border-t border-border p-2">
        {showNewChannel ? (
          <div className="flex flex-col gap-1.5">
            <input className="bg-card border border-border rounded px-2.5 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary w-full"
              placeholder="Channel ID" value={newChannelId} autoFocus
              onChange={e => setNewChannelId(e.target.value)} onKeyDown={e => {
                if (e.key === 'Enter') createChannel();
                if (e.key === 'Escape') setShowNewChannel(false);
              }} />
            <input className="bg-card border border-border rounded px-2.5 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:border-primary w-full"
              placeholder="Name (optional)" value={newChannelName}
              onChange={e => setNewChannelName(e.target.value)} onKeyDown={e => {
                if (e.key === 'Enter') createChannel();
                if (e.key === 'Escape') setShowNewChannel(false);
              }} />
            <div className="flex gap-1">
              <button
                type="button"
                onClick={() => setShowNewChannel(false)}
                className="flex-1 py-1 text-xs text-muted-foreground hover:text-foreground border border-border rounded transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={createChannel}
                disabled={creating || !newChannelId.trim()}
                className="flex-1 py-1 text-xs bg-primary hover:bg-primary/90 disabled:opacity-40 text-primary-foreground rounded transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                {creating ? '...' : 'Create'}
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setShowNewChannel(true)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted rounded transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
          >
            <Plus className="w-3.5 h-3.5" /> New channel
          </button>
        )}
      </div>

      {/* Delete modal overlay */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80">
          <div className="bg-card border border-border rounded p-5 w-96 flex flex-col gap-4 font-sans">
            <div className="flex items-center gap-2 text-rose-500">
              <AlertTriangle className="w-5 h-5" />
              <span className="font-bold text-sm">Delete channel</span>
            </div>
            <p className="text-xs text-foreground">
              Delete <span className="font-mono text-primary font-bold">#{deleteTarget.name}</span> ({deleteTarget.message_count} messages)? Irreversible.
            </p>
            <label className="flex items-center gap-2 cursor-pointer text-xs text-muted-foreground hover:text-foreground transition-colors">
              <input type="checkbox" checked={archiveOnDelete} onChange={e => setArchiveOnDelete(e.target.checked)} className="rounded accent-primary" />
              Archive to memory before deleting<span className="text-[11px] text-primary font-mono">(SilvaDB)</span>
            </label>
            <div className="flex gap-2 justify-end">
              <button
                type="button"
                onClick={() => setDeleteTarget(null)}
                className="px-4 py-2 text-xs text-muted-foreground hover:text-foreground border border-border rounded transition-colors cursor-pointer focus-visible:ring-2 focus-visible:ring-primary focus-visible:outline-none"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={handleDeleteConfirm}
                disabled={deleting}
                className="px-4 py-2 text-xs text-white bg-rose-700 hover:bg-rose-600 disabled:opacity-50 rounded transition-colors flex items-center gap-1.5 cursor-pointer focus-visible:ring-2 focus-visible:ring-rose-500 focus-visible:outline-none"
              >
                {deleting ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
