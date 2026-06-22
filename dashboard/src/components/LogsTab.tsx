import React, { useEffect, useRef, useState } from 'react';
import type { NexusEvent } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';
import { Search, Trash2, Scroll, Filter } from 'lucide-react';

interface Props {
  events: NexusEvent[];
  onClear?: () => void;
}

const EVENT_TYPES = ['tool_call', 'memory', 'guild_progress', 'error', 'sse'];

const EVENT_COLOR_MAP: Record<string, string> = {
  'tool_call': 'text-slate-400',
  'started': 'text-slate-400',
  'finished': 'text-emerald-400',
  'memory_added': 'text-blue-400',
  'memory_updated': 'text-blue-400',
  'guild_spawned': 'text-amber-400',
  'guild_killed': 'text-amber-400',
  'guild_progress': 'text-amber-400',
  'error': 'text-red-400',
  'failed': 'text-red-400',
  'sse_connect': 'text-violet-400',
  'sse_disconnect': 'text-violet-400',
  'heartbeat': 'text-violet-400',
  'edge_added': 'text-violet-400',
};

function eventColor(type: string): string {
  if (type.includes('error') || type.includes('failed')) return 'text-red-400';
  if (type.includes('sse_connect') || type.includes('sse_disconnect')) return 'text-violet-400';
  if (type.includes('memory_added') || type.includes('memory_updated')) return 'text-blue-400';
  if (type.includes('guild_spawned') || type.includes('guild_killed') || type.includes('guild_progress')) return 'text-amber-400';
  if (type.includes('finished')) return 'text-emerald-400';
  if (type.includes('started')) return 'text-slate-400';
  if (type.includes('tool_call')) return 'text-slate-400';
  return 'text-slate-500';
}

export function LogsTab({ events, onClear }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [searchText, setSearchText] = useState('');
  const [activeFilters, setActiveFilters] = useState<Set<string>>(new Set());
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      const lastElement = scrollRef.current.lastElementChild;
      if (lastElement) {
        lastElement.scrollIntoView({ behavior: 'smooth', block: 'end' });
      }
    }
  }, [events, autoScroll]);

  const toggleFilter = (type: string) => {
    setActiveFilters(prev => {
      const next = new Set(prev);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  };

  const clearLogs = () => {
    // Cannot modify the events prop directly, but we can signal the parent
    // by clearing via localStorage or emitting a custom event
    // For now, just clear local display — the parent controls the actual events
    // We'll use a custom event that the parent can listen to
    window.dispatchEvent(new CustomEvent('tylluan-clear-logs'));
  };

  const toggleAll = () => {
    if (activeFilters.size === EVENT_TYPES.length) {
      setActiveFilters(new Set());
    } else {
      setActiveFilters(new Set(EVENT_TYPES));
    }
  };

  const filteredEvents = events.filter(ev => {
    if (activeFilters.size > 0) {
      const typeMatch = [...activeFilters].some(f => ev.type.includes(f));
      if (!typeMatch) return false;
    }
    if (!searchText) return true;
    const lower = searchText.toLowerCase();
    return ev.type.toLowerCase().includes(lower) 
      || JSON.stringify(ev.data).toLowerCase().includes(lower);
  });

  return (
    <div className="flex flex-col h-[calc(100vh-180px)] space-y-2">
      {/* Header + Search + Filters */}
      <div className="flex items-center gap-3 flex-wrap">
        <div className="flex items-center gap-2">
          <span className="text-xs font-bold text-slate-400">Kernel Logs ({filteredEvents.length}/{events.length})</span>
        </div>
        
        {/* Search */}
        <div className="relative flex-1 min-w-[200px] max-w-md">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-slate-600" />
          <input
            type="text"
            value={searchText}
            onChange={e => setSearchText(e.target.value)}
            placeholder="Search logs..."
            className="w-full pl-8 pr-3 py-1.5 bg-slate-900 border border-slate-800 rounded text-xs text-slate-300 placeholder:text-slate-600 focus:ring-1 focus:ring-emerald-500/50 focus:border-emerald-500/50 outline-none"
          />
        </div>

        {/* Filters */}
        <div className="flex items-center gap-1.5 flex-wrap">
          <Filter className="w-3.5 h-3.5 text-slate-600" />
          {EVENT_TYPES.map(type => (
            <button
              key={type}
              type="button"
              onClick={() => toggleFilter(type)}
              className={cn(
                "px-2 py-0.5 rounded text-[10px] font-bold border transition-colors",
                activeFilters.has(type)
                  ? "bg-emerald-500/20 border-emerald-500/30 text-emerald-400"
                  : "bg-slate-900/50 border-slate-800 text-slate-600"
              )}
            >
              {type}
            </button>
          ))}
          <button
            type="button"
            onClick={toggleAll}
            className="px-2 py-0.5 rounded text-[10px] text-slate-600 hover:text-slate-400 transition-colors"
          >
            Toggle
          </button>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1.5 text-[10px] text-slate-500 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={e => setAutoScroll(e.target.checked)}
              className="accent-emerald-500"
            />
            <Scroll className="w-3 h-3" />
            Auto-scroll
          </label>
          <button
            type="button"
            onClick={() => onClear?.()}
            className="flex items-center gap-1 px-2 py-1 bg-red-500/10 hover:bg-red-500/20 text-red-400 rounded text-[10px] font-bold transition-colors"
          >
            <Trash2 className="w-3 h-3" /> Limpiar
          </button>
        </div>
      </div>

      {/* Event Stream */}
      <div 
        ref={scrollRef}
        className="flex-1 bg-slate-950 border border-slate-800 rounded-lg p-4 font-mono text-[11px] overflow-y-auto space-y-0.5"
      >
        {filteredEvents.map((ev, i) => {
          const color = eventColor(ev.type);
          return (
            <div key={i} className="flex gap-3 hover:bg-slate-900/50 px-1 rounded transition-colors group">
              <span className="text-slate-600 shrink-0">
                {new Date(ev.ts).toLocaleTimeString([], { hour12: false })}
              </span>
              <span className={cn("shrink-0 font-bold w-16", 
                ev.source === 'mcp' ? "text-emerald-500" : "text-blue-500"
              )}>
                [{ev.source.toUpperCase()}]
              </span>
              <span className={cn("shrink-0 w-32 truncate font-bold", color)}>
                {ev.type}
              </span>
              <span className="text-slate-300 break-all opacity-80 group-hover:opacity-100">
                {typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data)}
              </span>
            </div>
          );
        })}
        {filteredEvents.length === 0 && (
          <div className="h-full flex items-center justify-center text-slate-700 animate-pulse">
            {events.length === 0 ? 'Waiting for kernel events...' : 'No events match your filters'}
          </div>
        )}
      </div>
    </div>
  );
}
