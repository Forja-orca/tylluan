import { Search, ChevronRight, ChevronDown, Plus, Hash, Trash2, AlertTriangle } from 'lucide-react';
import { useMemo, useState } from 'react';
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
  const [archiveOnDelete, setArchiveOnDelete] = useState(true);
  const [deleting, setDeleting] = useState(false);

  const filteredChannels = useMemo(() =>
    channels.filter(ch => ch.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      ch.channel_id.toLowerCase().includes(searchQuery.toLowerCase())), [channels, searchQuery]);

  const groupedChannels = useMemo(() => {
    const g: Record<string, ColoquioChannel[]> = { Daily: [], 'Misión': [], 'Debates Privados': [], Canales: [] };
    filteredChannels.forEach(ch => {
      if (ch.channel_id.startsWith('daily-')) g['Daily'].push(ch);
      else if (ch.channel_id.startsWith('mision-')) g['Misión'].push(ch);
      else if (ch.channel_id.startsWith('private-') || ch.channel_id.startsWith('agent-')) g['Debates Privados'].push(ch);
      else g['Canales'].push(ch);
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
        className={cn('group/ch relative flex items-center gap-2 px-3 py-1.5 cursor-pointer rounded-md mx-1 transition-all',
          active ? 'bg-slate-700/60 text-slate-100' : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50')}>
        <Hash className="w-3.5 h-3.5 shrink-0 opacity-60" />
        <span className="flex-1 text-[12px] truncate font-medium">{ch.name}</span>
        {u > 0 && <span className="px-1.5 py-0.5 rounded-full bg-indigo-500 text-white text-[9px] font-bold leading-none">{u}</span>}
        <button onClick={e => { e.stopPropagation(); setDeleteTarget(ch); }}
          className="opacity-0 group-hover/ch:opacity-100 p-0.5 text-slate-600 hover:text-rose-400 transition-all rounded cursor-pointer">
          <Trash2 className="w-3.5 h-3.5" />
        </button>
      </div>
    );
  };

  return (
    <div className="w-52 shrink-0 border-r border-slate-700/60 flex flex-col bg-slate-900/50 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-slate-800/80">
        <Search className="w-3.5 h-3.5 text-slate-500 shrink-0" />
        <input className="flex-1 bg-transparent text-[11px] text-slate-200 placeholder-slate-600 focus:outline-none"
          placeholder="Search..." value={searchQuery} onChange={e => setSearchQuery(e.target.value)} />
      </div>
      <div className="flex-1 overflow-y-auto py-2">
        {Object.entries(groupedChannels).map(([group, chs]) => (
          <div key={group}>
            <button onClick={() => setCollapsedGroups(p => ({ ...p, [group]: !p[group] }))}
              className="w-full flex items-center gap-1.5 px-3 py-1 text-[10px] font-bold text-slate-500 uppercase tracking-wider hover:text-slate-400 transition-colors">
              {collapsedGroups[group] ? <ChevronRight className="w-2.5 h-2.5" /> : <ChevronDown className="w-2.5 h-2.5" />}
              {group}<span className="ml-auto text-slate-700 font-mono">{chs.length}</span>
            </button>
            {!collapsedGroups[group] && chs.map(renderChannelItem)}
          </div>
        ))}
        {filteredChannels.length === 0 && <div className="px-4 py-6 text-center text-[11px] text-slate-600 italic">No channels</div>}
      </div>
      <div className="border-t border-slate-800/80 p-2">
        {showNewChannel ? (
          <div className="flex flex-col gap-1.5">
            <input className="bg-slate-800 border border-slate-600 rounded px-2.5 py-1.5 text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-600 w-full"
              placeholder="Channel ID" value={newChannelId} autoFocus
              onChange={e => setNewChannelId(e.target.value)} onKeyDown={e => e.key === 'Enter' && createChannel()} />
            <input className="bg-slate-800 border border-slate-600 rounded px-2.5 py-1.5 text-[11px] text-slate-200 placeholder-slate-500 focus:outline-none focus:border-indigo-600 w-full"
              placeholder="Name (optional)" value={newChannelName}
              onChange={e => setNewChannelName(e.target.value)} onKeyDown={e => e.key === 'Enter' && createChannel()} />
            <div className="flex gap-1">
              <button onClick={() => setShowNewChannel(false)} className="flex-1 py-1 text-[10px] text-slate-500 hover:text-slate-300 border border-slate-700 rounded transition-colors">Cancel</button>
              <button onClick={createChannel} disabled={creating || !newChannelId.trim()} className="flex-1 py-1 text-[10px] bg-indigo-600 hover:bg-indigo-500 disabled:opacity-40 text-white rounded transition-colors">{creating ? '...' : 'Create'}</button>
            </div>
          </div>
        ) : (
          <button onClick={() => setShowNewChannel(true)} className="w-full flex items-center gap-2 px-3 py-1.5 text-[11px] text-slate-500 hover:text-slate-300 hover:bg-slate-800/50 rounded-lg transition-all">
            <Plus className="w-3.5 h-3.5" /> New channel
          </button>
        )}
      </div>

      {/* Delete modal overlay */}
      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm">
          <div className="bg-slate-900 border border-rose-500/30 rounded-2xl shadow-2xl p-6 w-96 flex flex-col gap-4">
            <div className="flex items-center gap-2 text-rose-400">
              <AlertTriangle className="w-5 h-5" />
              <span className="font-bold text-sm">Delete channel</span>
            </div>
            <p className="text-xs text-slate-300">
              Delete <span className="font-mono text-cyan-400">#{deleteTarget.name}</span> ({deleteTarget.message_count} messages)? Irreversible.
            </p>
            <label className="flex items-center gap-2 cursor-pointer text-xs text-slate-400 hover:text-slate-200 transition-colors">
              <input type="checkbox" checked={archiveOnDelete} onChange={e => setArchiveOnDelete(e.target.checked)} className="rounded accent-indigo-500" />
              Convert to memory before deleting<span className="text-[9px] text-indigo-600">(SilvaDB)</span>
            </label>
            <div className="flex gap-2 justify-end">
              <button onClick={() => setDeleteTarget(null)} className="px-4 py-2 text-xs text-slate-400 hover:text-slate-200 border border-slate-700 rounded-lg transition-colors cursor-pointer">Cancel</button>
              <button onClick={handleDeleteConfirm} disabled={deleting} className="px-4 py-2 text-xs text-white bg-rose-700 hover:bg-rose-600 disabled:opacity-50 rounded-lg transition-colors flex items-center gap-1.5 cursor-pointer">
                {deleting ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
