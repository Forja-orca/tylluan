import React, { useState, useEffect } from 'react';
import { OverviewTab } from './OverviewTab';
import { SystemTab } from './SystemTab';
import { InteroceptionTab } from './InteroceptionTab';
import { LayoutDashboard, Wrench, Activity, MessageSquare, ClipboardList, ChevronDown, ChevronUp, CheckCircle, Clock } from 'lucide-react';

interface OverviewConsolidatedProps {
  bridge: any;
  goldenSignals: any;
  guildsUtilization: any;
  memoryRetention: any;
  sloSummary: any;
  guilds: any[];
  approvals: any[];
  memoryStats: any;
  healthDetailed: any;
  sysStatus: any;
  events: any[];
  interoception: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onClearLogs: () => void;
}

export function OverviewConsolidated(props: OverviewConsolidatedProps) {
  const [subTab, setSubTab] = useState('summary');

  return (
    <div className="space-y-6">
      {/* Sub Navigation */}
      <div className="flex border-b border-slate-800 pb-2 gap-2">
        <button
          onClick={() => setSubTab('summary')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'summary'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <LayoutDashboard className="w-3.5 h-3.5" />
          Summary
        </button>
        <button
          onClick={() => setSubTab('interoception')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'interoception'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Activity className="w-3.5 h-3.5" />
          Interoception
        </button>
        <button
          onClick={() => setSubTab('system')}
          className={`flex items-center gap-2 px-4 py-2 text-xs font-mono font-bold uppercase rounded-lg border transition-all ${
            subTab === 'system'
              ? 'bg-emerald-500/10 border-emerald-500/30 text-emerald-400'
              : 'bg-slate-900/40 border-slate-800/80 text-slate-400 hover:text-slate-200'
          }`}
        >
          <Wrench className="w-3.5 h-3.5" />
          System Config
        </button>
      </div>

      {/* Tab Panels */}
      <div>
        {subTab === 'summary' && (
          <div className="space-y-6">
            <OverviewTab
              bridge={props.bridge}
              goldenSignals={props.goldenSignals}
              guildsUtilization={props.guildsUtilization}
              memoryRetention={props.memoryRetention}
              sloSummary={props.sloSummary}
              guilds={props.guilds}
              approvals={props.approvals}
              memoryStats={props.memoryStats}
              healthDetailed={props.healthDetailed}
              sysStatus={props.sysStatus}
              events={props.events}
            />
            {/* Team Pulse Widget */}
            <TeamPulseWidget bridge={props.bridge} />
            {/* Task Registry Widget */}
            <TaskRegistryWidget bridge={props.bridge} />
          </div>
        )}
        {subTab === 'interoception' && (
          <InteroceptionTab
            interoception={props.interoception}
            memoryStats={props.memoryStats}
          />
        )}
        {subTab === 'system' && (
          <SystemTab
            bridge={props.bridge}
            notify={props.notify}
            events={props.events}
            onClearLogs={props.onClearLogs}
          />
        )}
      </div>
    </div>
  );
}

function TeamPulseWidget({ bridge }: { bridge: any }) {
  const [messages, setMessages] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchPulse = async () => {
    if (!bridge) return;
    try {
      const data = await bridge.getColoquioThread("mision-activa");
      const msgs = data.messages || [];
      // Take the last 3 messages and reverse so newest is first, or keep order. Let's show last 3 newest first.
      const last3 = [...msgs].slice(-3).reverse();
      setMessages(last3);
    } catch (e) {
      console.error("[TeamPulse] Failed to fetch thread:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPulse();
    const interval = setInterval(fetchPulse, 10000);
    return () => clearInterval(interval);
  }, [bridge]);

  const handleMessageClick = () => {
    window.dispatchEvent(new CustomEvent('nexus_switch_tab', { detail: 'team' }));
  };

  const getAgentIcon = (authorId: string): string => {
    const cleanId = authorId.toLowerCase();
    if (cleanId.includes('user') || cleanId.includes('human')) return '👤';
    if (cleanId.includes('builder') || cleanId.includes('architect')) return '🧠';
    if (cleanId.includes('visual') || cleanId.includes('painter')) return '🪐';
    if (cleanId.includes('search') || cleanId.includes('web')) return '🪁';
    return '🤖';
  };

  const formatRelativeTime = (secondsAgo: number): string => {
    if (secondsAgo < 60) return 'now';
    const mins = Math.floor(secondsAgo / 60);
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  };

  const nowSecs = Math.floor(Date.now() / 1000);
  const isIdle = messages.length === 0 || (nowSecs - messages[0].created_at) > 3600;

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900/50 overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-800 bg-slate-800/30 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-bold uppercase tracking-widest text-slate-400 font-mono">Team Pulse</span>
          {!isIdle && !loading && (
            <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
          )}
        </div>
        <MessageSquare className="w-3.5 h-3.5 text-blue-400" />
      </div>

      <div className="p-4">
        {loading ? (
          <div className="text-center text-xs text-slate-500 font-mono py-2">Loading pulse...</div>
        ) : isIdle ? (
          <div className="text-center text-xs text-slate-500 font-mono py-4 flex flex-col items-center gap-1">
            <span className="text-slate-400 font-semibold">Team Idle</span>
            <span className="text-[10px] text-slate-600">No activity in the last 1 hour</span>
          </div>
        ) : (
          <div className="divide-y divide-slate-800/40">
            {messages.map((msg: any) => {
              const secondsAgo = Math.max(0, nowSecs - msg.created_at);
              const authorColor = msg.role === 'human' ? 'text-blue-400' : 'text-emerald-400';
              const textPreview = msg.content.length > 80 ? msg.content.slice(0, 80) + '...' : msg.content;
              
              return (
                <div
                  key={msg.msg_id}
                  onClick={handleMessageClick}
                  className="py-3 first:pt-0 last:pb-0 flex items-start gap-3 hover:bg-slate-800/20 transition-all cursor-pointer rounded-lg px-2 -mx-2"
                >
                  <span className="text-base flex-shrink-0 mt-0.5" role="img" aria-label="avatar">
                    {getAgentIcon(msg.author_id)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline justify-between gap-2">
                      <span className={`text-xs font-bold font-mono ${authorColor}`}>@{msg.author_id}</span>
                      <span className="text-[9px] font-mono text-slate-500 flex-shrink-0">
                        {formatRelativeTime(secondsAgo)}
                      </span>
                    </div>
                    <p className="text-xs text-slate-300 mt-1 truncate">
                      {textPreview}
                    </p>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

interface TaskItem {
  id: string;
  title: string;
  status: 'DONE' | 'PENDING' | 'IN_PROGRESS';
  details: Record<string, string>;
}

function parseMarkdownTasks(content: string) {
  const lines = content.split('\n');
  let currentSection: 'COMPLETED' | 'IN_PROGRESS' | 'PENDING' | 'OTHER' = 'OTHER';
  const completedLines: string[] = [];
  const inProgressLines: string[] = [];
  const pendingLines: string[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('## COMPLETED')) {
      currentSection = 'COMPLETED';
      continue;
    } else if (trimmed.startsWith('## IN PROGRESS') || trimmed.startsWith('## IN-PROGRESS')) {
      currentSection = 'IN_PROGRESS';
      continue;
    } else if (trimmed.startsWith('## PENDING')) {
      currentSection = 'PENDING';
      continue;
    } else if (trimmed.startsWith('##') || trimmed.startsWith('---')) {
      currentSection = 'OTHER';
    }

    if (currentSection === 'COMPLETED') {
      completedLines.push(line);
    } else if (currentSection === 'IN_PROGRESS') {
      inProgressLines.push(line);
    } else if (currentSection === 'PENDING') {
      pendingLines.push(line);
    }
  }

  const parseSection = (secLines: string[], defaultStatus: 'DONE' | 'PENDING' | 'IN_PROGRESS'): TaskItem[] => {
    const items: TaskItem[] = [];
    let currentItem: TaskItem | null = null;

    for (const line of secLines) {
      const trimmed = line.trim();
      if (!trimmed) continue;

      if (trimmed.startsWith('[')) {
        if (currentItem) {
          items.push(currentItem);
        }
        currentItem = {
          id: Math.random().toString(36).substring(7),
          title: trimmed,
          status: defaultStatus,
          details: {}
        };
      } else if (currentItem) {
        const colonIndex = trimmed.indexOf(':');
        if (colonIndex > 0) {
          const key = trimmed.slice(0, colonIndex).trim().toUpperCase();
          const val = trimmed.slice(colonIndex + 1).trim();
          currentItem.details[key] = val;
          if (key === 'STATUS') {
            if (val.toUpperCase().includes('DONE')) {
              currentItem.status = 'DONE';
            } else if (val.toUpperCase().includes('PROGRESS')) {
              currentItem.status = 'IN_PROGRESS';
            } else {
              currentItem.status = 'PENDING';
            }
          }
        }
      }
    }
    if (currentItem) {
      items.push(currentItem);
    }
    return items;
  };

  const doneTasks = parseSection(completedLines, 'DONE');
  const inProgressTasks = parseSection(inProgressLines, 'IN_PROGRESS');
  const pendingTasks = parseSection(pendingLines, 'PENDING');

  return {
    pending: [...inProgressTasks, ...pendingTasks],
    done: doneTasks
  };
}

function parseTitle(title: string) {
  const fullMatch = title.match(/^\[([^\]]+)\]\s*\[([^\]]+)\]\s*(?:TASK:)?\s*(.*)$/i);
  if (fullMatch) {
    return {
      tag: fullMatch[1],
      agent: fullMatch[2],
      desc: fullMatch[3]
    };
  }
  const tagMatch = title.match(/^\[([^\]]+)\]\s*(.*)$/);
  if (tagMatch) {
    return {
      tag: tagMatch[1],
      agent: '',
      desc: tagMatch[2]
    };
  }
  return {
    tag: '',
    agent: '',
    desc: title
  };
}

function TaskRegistryWidget({ bridge }: { bridge: any }) {
  const [tasks, setTasks] = useState<{ pending: TaskItem[]; done: TaskItem[] }>({ pending: [], done: [] });
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const fetchTasks = async () => {
    if (!bridge) return;
    try {
      const listData = await bridge.fetchRaw('/api/v1/coloquio/documents');
      const docs = listData?.documents || [];
      const registryDoc = docs.find((d: any) => d.title && d.title.toUpperCase().includes('TASK REGISTRY'));
      if (!registryDoc) {
        setLoading(false);
        return;
      }

      const docData = await bridge.fetchRaw(`/api/v1/coloquio/documents/${registryDoc.doc_id}`);
      const content = docData?.document?.content || '';

      const parsed = parseMarkdownTasks(content);
      setTasks(parsed);
    } catch (e) {
      console.error("[TaskRegistry] Failed to fetch task registry:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchTasks();
    const interval = setInterval(fetchTasks, 10000);
    return () => clearInterval(interval);
  }, [bridge]);

  const toggleExpand = (id: string) => {
    setExpandedId(expandedId === id ? null : id);
  };

  const slicedDone = [...tasks.done].slice(-5).reverse();

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900/50 overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-800 bg-slate-800/30 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-bold uppercase tracking-widest text-slate-400 font-mono">Task Registry</span>
          {!loading && (
            <span className="text-[9px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400 font-mono">
              {tasks.pending.length} pending
            </span>
          )}
        </div>
        <ClipboardList className="w-3.5 h-3.5 text-emerald-400" />
      </div>

      <div className="p-4 space-y-4">
        {loading ? (
          <div className="text-center text-xs text-slate-500 font-mono py-2">Loading task registry...</div>
        ) : (
          <>
            {/* Pending Tasks */}
            <div className="space-y-2">
              <div className="flex items-center gap-1.5 text-xs font-mono font-semibold text-amber-500/90">
                <Clock className="w-3.5 h-3.5" />
                <span>PENDING & IN-PROGRESS</span>
              </div>
              {tasks.pending.length === 0 ? (
                <div className="text-xs text-slate-600 font-mono pl-5 py-1">No pending tasks</div>
              ) : (
                <div className="space-y-1.5 pl-2">
                  {tasks.pending.map((task) => {
                    const { tag, agent, desc } = parseTitle(task.title);
                    const isExpanded = expandedId === task.id;
                    const isWorking = task.status === 'IN_PROGRESS';
                    const badgeBg = isWorking ? 'bg-amber-500/10 text-amber-400 border-amber-500/20' : 'bg-rose-500/10 text-rose-400 border-rose-500/20';

                    return (
                      <div
                        key={task.id}
                        className="rounded-lg border border-slate-800/60 bg-slate-950/20 overflow-hidden transition-all"
                      >
                        <div
                          onClick={() => toggleExpand(task.id)}
                          className="p-2.5 flex items-start justify-between gap-3 hover:bg-slate-800/25 transition-all cursor-pointer"
                        >
                          <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-1.5">
                              {tag && (
                                <span className={`text-[9px] font-mono px-1 py-0.2 rounded border ${badgeBg}`}>
                                  {tag}
                                </span>
                              )}
                              {agent && (
                                <span className="text-[9px] font-mono px-1 py-0.2 rounded border bg-slate-800/50 text-slate-300 border-slate-700/50">
                                  @{agent}
                                </span>
                              )}
                            </div>
                            <p className="text-xs text-slate-300 font-sans font-medium leading-relaxed">
                              {desc}
                            </p>
                          </div>
                          <span className="text-slate-500 hover:text-slate-300 self-center flex-shrink-0">
                            {isExpanded ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                          </span>
                        </div>

                        {isExpanded && Object.keys(task.details).length > 0 && (
                          <div className="px-3 pb-3 pt-0.5 bg-slate-950/60 border-t border-slate-900/60 text-[10px] font-mono text-slate-400 space-y-1">
                            {Object.entries(task.details).map(([key, val]) => (
                              <div key={key} className="flex items-start gap-2">
                                <span className="text-amber-500/70 font-semibold uppercase tracking-wider min-w-[75px]">{key}:</span>
                                <span className="text-slate-300 break-all whitespace-pre-wrap">{val}</span>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Completed Tasks */}
            <div className="space-y-2 pt-2 border-t border-slate-800/50">
              <div className="flex items-center gap-1.5 text-xs font-mono font-semibold text-emerald-500/90">
                <CheckCircle className="w-3.5 h-3.5" />
                <span>COMPLETED (LATEST 5)</span>
              </div>
              {slicedDone.length === 0 ? (
                <div className="text-xs text-slate-600 font-mono pl-5 py-1">No completed tasks found</div>
              ) : (
                <div className="space-y-1.5 pl-2">
                  {slicedDone.map((task) => {
                    const { tag, agent, desc } = parseTitle(task.title);
                    const isExpanded = expandedId === task.id;

                    return (
                      <div
                        key={task.id}
                        className="rounded-lg border border-slate-800/60 bg-slate-950/20 overflow-hidden transition-all"
                      >
                        <div
                          onClick={() => toggleExpand(task.id)}
                          className="p-2.5 flex items-start justify-between gap-3 hover:bg-slate-800/25 transition-all cursor-pointer"
                        >
                          <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-1.5">
                              {tag && (
                                <span className="text-[9px] font-mono px-1 py-0.2 rounded border bg-emerald-500/10 text-emerald-400 border-emerald-500/20">
                                  {tag}
                                </span>
                              )}
                              {agent && (
                                <span className="text-[9px] font-mono px-1 py-0.2 rounded border bg-slate-800/50 text-slate-300 border-slate-700/50">
                                  @{agent}
                                </span>
                              )}
                            </div>
                            <p className="text-xs text-slate-400 font-sans font-medium leading-relaxed">
                              {desc}
                            </p>
                          </div>
                          <span className="text-slate-500 hover:text-slate-300 self-center flex-shrink-0">
                            {isExpanded ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
                          </span>
                        </div>

                        {isExpanded && Object.keys(task.details).length > 0 && (
                          <div className="px-3 pb-3 pt-0.5 bg-slate-950/60 border-t border-slate-900/60 text-[10px] font-mono text-slate-400 space-y-1">
                            {Object.entries(task.details).map(([key, val]) => (
                              <div key={key} className="flex items-start gap-2">
                                <span className="text-emerald-500/70 font-semibold uppercase tracking-wider min-w-[75px]">{key}:</span>
                                <span className="text-slate-300 break-all whitespace-pre-wrap">{val}</span>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default OverviewConsolidated;
