import React, { useState, useEffect } from 'react';
import { 
  FileCode, 
  ServerOff, 
  Plus, 
  Trash2, 
  RefreshCw, 
  Eye, 
  Save, 
  X,
  Code
} from 'lucide-react';
import { cn } from '../lib/utils';
import { ProjectSkill } from '../lib/nexus-bridge';

interface ProjectSkillsPanelProps {
  bridge: any;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

export default function ProjectSkillsPanel({ bridge, notify }: ProjectSkillsPanelProps) {
  // Real backend: /api/v1/skills lists names; content is fetched lazily per
  // skill via the @skill:get:<name> intent when a skill is opened (see lib/api/memory.ts).
  const [skills, setSkills] = useState<Pick<ProjectSkill, 'name'>[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Editor states
  const [editingSkill, setEditingSkill] = useState<ProjectSkill | null>(null);
  const [loadingSkill, setLoadingSkill] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [newContent, setNewContent] = useState('');



  const loadSkills = async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const data = await bridge.getProjectSkills();
      setSkills(data || []);
    } catch (err: any) {
      console.error("Project Skills API error:", err.message);
      setSkills([]);
      setError(`Error al consultar habilidades del proyecto: ${err.message}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadSkills();
  }, []);

  const handleSave = async () => {
    const nameToSave = isCreating ? newName.trim() : editingSkill?.name;
    const contentToSave = newContent.trim();

    if (!nameToSave || !contentToSave) {
      notify('Name and content are required', 'error');
      return;
    }

    setLoading(true);
    try {
      await bridge.saveProjectSkill(nameToSave, contentToSave);
      notify(`Skill '${nameToSave}' saved successfully`, 'info');
      await loadSkills();
      setIsCreating(false);
      setEditingSkill(null);
    } catch (err: any) {
      notify(`Failed to save: ${err.message}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (name: string) => {
    if (!window.confirm(`Are you sure you want to delete the skill '${name}'?`)) return;

    setLoading(true);
    try {
      await bridge.deleteProjectSkill(name);
      notify(`Skill '${name}' deleted`, 'info');
      await loadSkills();
      if (editingSkill?.name === name) {
        setEditingSkill(null);
      }
    } catch (err: any) {
      notify(`Failed to delete: ${err.message}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // Skill list endpoint only returns names — full content is fetched lazily
  // via the @skill:get:<name> intent.
  const openEditor = async (skillName?: string) => {
    if (!skillName) {
      setIsCreating(true);
      setEditingSkill(null);
      setNewName('');
      setNewContent('');
      return;
    }
    setIsCreating(false);
    setLoadingSkill(true);
    try {
      const full = await bridge.getProjectSkill(skillName);
      setEditingSkill(full);
      setNewName(full.name);
      setNewContent(full.content);
    } catch (err: any) {
      notify(`Failed to load skill '${skillName}': ${err.message}`, 'error');
    } finally {
      setLoadingSkill(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex justify-between items-start">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-50 flex items-center gap-2">
            <FileCode className="w-5 h-5 text-emerald-400" />
            Project Skills
          </h2>
          <p className="text-xs text-slate-400 mt-1">
            Manage reusable project context and instructions accessible to all agents via <code className="text-emerald-400 bg-emerald-400/10 px-1 py-0.5 rounded">tylluan_do</code> intents.
          </p>
        </div>
        <button
          onClick={loadSkills}
          disabled={loading}
          className="p-2 text-slate-400 hover:text-amber-400 hover:bg-amber-400/10 rounded-lg transition-colors"
          title="Refresh skills"
        >
          <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
        </button>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left Column: Skill List */}
        <div className="lg:col-span-1 space-y-4">
          <div className="flex justify-between items-center">
            <span className="text-[11px] font-medium font-mono text-slate-400">Available Skills</span>
            <button
              onClick={() => openEditor()}
              className="px-3 py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 text-xs font-medium font-mono rounded-lg flex items-center gap-1.5 transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              New
            </button>
          </div>

          <div className="bg-slate-900/60 rounded-2xl overflow-hidden divide-y divide-slate-800">
            {error ? (
              <div className="p-8 text-center space-y-2">
                <ServerOff className="w-8 h-8 mx-auto text-slate-600" />
                <p className="text-xs text-red-400 font-mono">{error}</p>
              </div>
            ) : skills.length === 0 ? (
              <div className="p-8 text-center text-slate-500 font-mono text-xs">
                No project skills found.
              </div>
            ) : (
              skills.map(skill => (
                <div 
                  key={skill.name}
                  className={cn(
                    "p-4 flex items-center justify-between group transition-colors cursor-pointer hover:bg-slate-800/50",
                    editingSkill?.name === skill.name && "bg-slate-800/80 border-l-2 border-emerald-500"
                  )}
                  onClick={() => openEditor(skill.name)}
                >
                  <div className="flex items-center gap-3">
                    <Code className="w-4 h-4 text-emerald-500" />
                    <div className="text-sm font-mono font-bold text-slate-200">
                      {skill.name}
                    </div>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(skill.name);
                    }}
                    className="p-1.5 text-slate-600 hover:text-red-400 hover:bg-red-400/10 rounded opacity-0 group-hover:opacity-100 transition-all"
                    title="Delete skill"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>

        {/* Right Column: Editor / Viewer */}
        <div className="lg:col-span-2">
          {isCreating || editingSkill ? (
            <div className="p-6 bg-slate-900/40 rounded-2xl space-y-4 min-h-[400px] flex flex-col">
              <div className="flex justify-between items-center border-b border-slate-800 pb-3">
                <span className="text-[11px] font-medium font-mono text-slate-400">
                  {isCreating ? 'Create New Skill' : 'Edit Skill'}
                </span>
                <button
                  onClick={() => { setIsCreating(false); setEditingSkill(null); }}
                  className="p-1 text-slate-500 hover:text-slate-300"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="space-y-4 flex-1 flex flex-col">
                <div>
                  <label className="text-[10px] font-mono text-slate-500 block mb-1 font-medium">Skill Name / Identifier</label>
                  <input
                    type="text"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    disabled={!isCreating}
                    placeholder="e.g. format-guidelines"
                    className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3 py-2 text-xs font-mono text-slate-200 focus:border-amber-500 focus:outline-none disabled:opacity-50"
                  />
                </div>
                
                <div className="flex-1 flex flex-col">
                  <label className="text-[10px] font-mono text-slate-500 block mb-1 font-medium">Content / Instructions</label>
                  <textarea
                    value={newContent}
                    onChange={(e) => setNewContent(e.target.value)}
                    placeholder="Describe the context or instructions that agents should follow when invoking this skill..."
                    className="w-full flex-1 min-h-[200px] p-4 bg-slate-950 border border-slate-800 focus:border-amber-500 focus:outline-none rounded-xl font-mono text-xs text-slate-200 placeholder-slate-600 resize-none leading-relaxed"
                  />
                </div>
              </div>

              <div className="flex justify-end pt-2">
                <button
                  onClick={handleSave}
                  disabled={loading || !newName.trim() || !newContent.trim()}
                  className="px-6 py-2 bg-amber-500 hover:bg-amber-600 text-slate-950 font-semibold font-mono text-xs rounded-xl flex items-center gap-2 transition-colors disabled:opacity-50"
                >
                  <Save className="w-4 h-4" />
                  Save Skill
                </button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full min-h-[400px] text-slate-500 font-mono text-xs py-12 gap-3 rounded-2xl bg-slate-900/20">
              <Eye className="w-12 h-12 text-slate-700 animate-pulse" />
              <div className="text-center">
                <p>No skill selected</p>
                <p className="text-[10px] text-slate-600 mt-1">Select a skill from the list to view or edit its contents.</p>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
