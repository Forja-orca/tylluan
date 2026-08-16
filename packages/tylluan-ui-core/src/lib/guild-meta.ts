/**
 * Guild metadata — single source of truth for GuildsTab and any future
 * guild-related UI. Category classification + deprecation list.
 *
 * If a new guild is added or deprecated, this is the ONLY file to update.
 */

export type GuildCategory = 'Core' | 'Builder' | 'Scholar' | 'Watcher' | 'Tool';

export const CATEGORY_MAP: Record<string, GuildCategory> = {
  // Core (always-on system tools)
  bash: 'Core', filesystem: 'Core', memory: 'Core', monitor: 'Core',
  local_llm_proxy: 'Core', cron_scheduler: 'Core',
  // Builders (create, build, deploy)
  git: 'Builder', code: 'Builder', docker: 'Builder', rust_specialist: 'Builder',
  ast_surgeon: 'Builder',
  // Scholars (research, analyze, learn)
  search: 'Scholar', browser: 'Scholar', knowledge: 'Scholar', pdf: 'Scholar',
  vision: 'Scholar', code_analysis: 'Scholar', sequential_thinking: 'Scholar',
  deep_analysis: 'Scholar', ingest: 'Scholar',
  // Watchers (audit, observe, protect)
  audit: 'Watcher', system_metrics: 'Watcher', security: 'Watcher',
  biome_warden: 'Watcher',
  // Tools (utility guilds — v1-port and media)
  screenshot_tools: 'Tool', clipboard_tools: 'Tool',
  audio_tools: 'Tool', ffmpeg_tools: 'Tool',
};

export const CATEGORY_STYLE: Record<GuildCategory, { label: string; cls: string }> = {
  Core:    { label: 'Core',    cls: 'bg-slate-700 text-slate-300 border-slate-600' },
  Builder: { label: 'Builder', cls: 'bg-blue-500/15 text-blue-400 border-blue-500/25' },
  Scholar: { label: 'Scholar', cls: 'bg-violet-500/15 text-violet-400 border-violet-500/25' },
  Watcher: { label: 'Watcher', cls: 'bg-amber-500/15 text-amber-400 border-amber-500/25' },
  Tool:    { label: 'Tool',    cls: 'bg-cyan-500/15 text-cyan-400 border-cyan-500/25' },
};

export const DEPRECATED_GUILDS = new Set([
  'formatter', 'web_search', 'data_tools', 'database',
  'code_analysis', 'pdf', 'browser', 'n8n',
]);

export function getGuildCategory(name: string): GuildCategory {
  return CATEGORY_MAP[name.toLowerCase().replace(/-/g, '_')] ?? 'Core';
}
