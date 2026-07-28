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
  // Real backend (@skill:list) only returns names -- content is fetched lazily
  // per-skill via @skill:get:<name> when a skill is opened. Mock data carries
  // full content upfront since there's no real intent round-trip to lazy-load from.
  const [skills, setSkills] = useState<Pick<ProjectSkill, 'name'>[]>([]);
  const [loading, setLoading] = useState(false);
  const [isMock, setIsMock] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Editor states
  const [editingSkill, setEditingSkill] = useState<ProjectSkill | null>(null);
  const [loadingSkill, setLoadingSkill] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState('');
  const [newContent, setNewContent] = useState('');

  const mockSkills: ProjectSkill[] = [
    {
      name: 'format-guidelines',
      content: 'Always format code to 100 column width, use 4 spaces indent, and prioritize early returns.',
      created_at: new Date(Date.now() - 86400000).toISOString()
    },
    {
      name: 'deploy-prod',
      content: 'Build the binary with --release --features bundled-dashboard, and push to ECR before reloading the service.',
      created_at: new Date(Date.now() - 3600000 * 4).toISOString()
    }
  ];

  const loadSkills = async () => {
    if (!bridge) return;
    setLoading(true);
    setError(null);
    try {
      const data = await bridge.getProjectSkills();
      setSkills(data || []);
      setIsMock(false);
    } catch (err: any) {
      console.error("Project Skills API error:", err.message);
      setSkills([]);
      setIsMock(false);
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
      if (!isMock) {
        await bridge.saveProjectSkill(nameToSave, contentToSave);
        notify(`Skill '${nameToSave}' saved successfully`, 'info');
        await loadSkills();
      } else {
        // Simulate save (mock mode only tracks names in `skills`; content lives
        // in editingSkill for immediate display)
        setSkills(prev => prev.some(s => s.name === nameToSave) ? prev : [...prev, { name: nameToSave }]);
        notify(`[SIMULATED] Skill '${nameToSave}' saved`, 'info');
      }
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
      if (!isMock) {
        await bridge.deleteProjectSkill(name);
        notify(`Skill '${name}' deleted`, 'info');
        await loadSkills();
      } else {
        setSkills(prev => prev.filter(s => s.name !== name));
        notify(`[SIMULATED] Skill '${name}' deleted`, 'info');
      }
      if (editingSkill?.name === name) {
        setEditingSkill(null);
      }
    } catch (err: any) {
      notify(`Failed to delete: ${err.message}`, 'error');
    } finally {
      setLoading(false);
    }
  };

  // @skill:list only returns names -- full content is fetched lazily here,
  // either via the real @skill:get:<name> intent or from local mock data.
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
      const full = isMock
        ? mockSkills.find(s => s.name === skillName) ?? { name: skillName, content: '', created_at: '' }
        : await bridge.getProjectSkill(skillName);
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
          className="p-2 text-slate-400 hover:text-emerald-400 hover:bg-emerald-400/10 rounded-lg transition-colors"
          title="Refresh skills"
        >
          <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
        </button>
      </div>

      {/* Mock status warning */}
      {isMock && (
        <div className="p-3 bg-amber-500/10 border border-amber-500/20 text-amber-400 rounded-2xl flex items-center gap-3 text-xs leading-normal font-mono">
          <ServerOff className="w-4 h-4 flex-shrink-0 animate-pulse text-amber-500" />
          <div>
            <span className="font-bold">[SIMULATED SKILLS MODULE] </span>
            {error || "Project Skills API is pending backend implementation. Operating on mock state."}
          </div>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left Column: Skill List */}
        <div className="lg:col-span-1 space-y-4">
          <div className="flex justify-between items-center">
            <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-400">Available Skills</span>
            <button
              onClick={() => openEditor()}
              className="px-3 py-1.5 bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-400 text-xs font-bold font-mono rounded-lg flex items-center gap-1.5 transition-colors"
            >
              <Plus className="w-3.5 h-3.5" />
              New
            </button>
          </div>

          <div className="bg-slate-900/60 border border-slate-850 rounded-2xl overflow-hidden divide-y divide-slate-800">
            {skills.length === 0 ? (
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
            <div className="p-6 bg-slate-900/40 border border-slate-850 rounded-2xl space-y-4 min-h-[400px] flex flex-col">
              <div className="flex justify-between items-center border-b border-slate-800 pb-3">
                <span className="text-xs font-bold uppercase tracking-wider font-mono text-slate-400">
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
                  <label className="text-[10px] font-mono text-slate-500 block mb-1 uppercase font-bold">Skill Name / Identifier</label>
                  <input
                    type="text"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    disabled={!isCreating}
                    placeholder="e.g. format-guidelines"
                    className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3 py-2 text-xs font-mono text-slate-200 focus:border-emerald-500 focus:outline-none disabled:opacity-50"
                  />
                </div>
                
                <div className="flex-1 flex flex-col">
                  <label className="text-[10px] font-mono text-slate-500 block mb-1 uppercase font-bold">Content / Instructions</label>
                  <textarea
                    value={newContent}
                    onChange={(e) => setNewContent(e.target.value)}
                    placeholder="Describe the context or instructions that agents should follow when invoking this skill..."
                    className="w-full flex-1 min-h-[200px] p-4 bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none rounded-xl font-mono text-xs text-slate-200 placeholder-slate-600 resize-none leading-relaxed"
                  />
                </div>
              </div>

              <div className="flex justify-end pt-2">
                <button
                  onClick={handleSave}
                  disabled={loading || !newName.trim() || !newContent.trim()}
                  className="px-6 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 font-bold font-mono text-xs rounded-xl flex items-center gap-2 transition-colors disabled:opacity-50"
                >
                  <Save className="w-4 h-4" />
                  Save Skill
                </button>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center h-full min-h-[400px] text-slate-500 font-mono text-xs py-12 gap-3 border border-slate-850 border-dashed rounded-2xl bg-slate-900/20">
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
