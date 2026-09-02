import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, ShieldCheck } from 'lucide-react';
import { cn } from '../lib/utils';

interface Props {
  bridge: NexusBridge | null;
  models: any;
}

export function ModelsRoleAssignment({ bridge, models }: Props) {
  const [rolePrimary, setRolePrimary] = useState('qwen2.5-1.5b-instruct');
  const [roleCoordinator, setRoleCoordinator] = useState('gemma-4-2b-it');
  const [roleRouting, setRoleRouting] = useState('smollm2-135m-instruct');
  const [roleVision, setRoleVision] = useState('SmolVLM2-256M-Instruct');
  const [savingRoles, setSavingRoles] = useState(false);

  const isModelInstalled = (modelVal: string) => {
    if (!models?.detected_local_models || models.detected_local_models.length === 0) return null;
    if (!modelVal) return null;
    return models.detected_local_models.find((m: any) => {
      const mId = (m.id || '').toLowerCase();
      const mName = (m.name || '').toLowerCase();
      const val = modelVal.toLowerCase();
      return mId === val || mName === val || mId.includes(val) || val.includes(mId) || mName.includes(val) || val.includes(mName);
    }) || null;
  };

  const handleSaveRoles = async () => {
    if (!bridge) return;

    if (models?.detected_local_models?.length > 0) {
      const roleChecks = [
        { role: 'Primary Inference', val: rolePrimary },
        { role: 'Night Reasoner', val: roleCoordinator },
        { role: 'Routing & Intent', val: roleRouting },
        { role: 'Visual Analysis', val: roleVision },
      ];
      const unverified = roleChecks.filter(r => r.val && !isModelInstalled(r.val));
      if (unverified.length > 0) {
        const warnMsg = `Warning: The following models were not detected in the local disk inventory:\n\n` +
          unverified.map(u => `• ${u.role}: "${u.val}"`).join('\n') +
          `\n\nDo you want to save this assignment anyway?`;
        if (!window.confirm(warnMsg)) {
          return;
        }
      }
    }

    setSavingRoles(true);
    try {
      const res = await bridge.fetch('/api/v1/config/inference-llama', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          primary_model: rolePrimary,
          coordinator_model: roleCoordinator,
          routing_model: roleRouting,
          vision_model: roleVision,
        })
      });
      if (res?.error) throw new Error(res.error);
      alert('Model role assignment saved successfully in tylluan.toml.');
    } catch (e: any) {
      alert(`Error saving model role assignment: ${e.message || String(e)}`);
    }
    setSavingRoles(false);
  };

  const roles = [
    {
      id: 'primary',
      title: '1. Primary Inference (Primary Agent)',
      desc: 'Answers user queries and executes tools (tylluan_do). Requires strong instruction following.',
      rec: 'Recommended: 1.5B – 3B (e.g. Qwen2.5-1.5B)',
      value: rolePrimary,
      setter: setRolePrimary,
      color: 'border-emerald-500/30'
    },
    {
      id: 'coordinator',
      title: '2. Night Reasoner (Coordinator)',
      desc: 'Background episodic memory consolidation. Prioritizes deep reasoning over latency.',
      rec: 'Recommended: 2B – 4B (e.g. Gemma-2B / Qwen2.5-3B)',
      value: roleCoordinator,
      setter: setRoleCoordinator,
      color: 'border-violet-500/30'
    },
    {
      id: 'routing',
      title: '3. Routing & Intent',
      desc: 'Ultra-fast intent classification and filtering. Requires latency <50ms on CPU.',
      rec: 'Recommended: <500M (e.g. SmolLM2-135M / Qwen-0.5B)',
      value: roleRouting,
      setter: setRoleRouting,
      color: 'border-blue-500/30'
    },
    {
      id: 'vision',
      title: '4. Visual Analysis (Vision Model)',
      desc: 'Text extraction (OCR) and image description for the vision guild.',
      rec: 'Recommended: SmolVLM2-256M / Moondream',
      value: roleVision,
      setter: setRoleVision,
      color: 'border-sky-500/30'
    },
  ];

  return (
    <div className="rounded-lg bg-slate-900/50 p-4 space-y-4">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-800/60 pb-3">
        <div>
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-amber-400" /> Cognitive Role Model Assignment
          </h3>
          <p className="text-xs text-slate-400 mt-1">
            Each module inside Tylluan requires different tradeoffs of model size, latency, and reasoning capability.
          </p>
        </div>
        <button
          onClick={handleSaveRoles}
          disabled={savingRoles}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 text-xs font-semibold rounded-lg transition-all disabled:opacity-50 shrink-0 cursor-pointer"
        >
          <Save className={cn("w-3.5 h-3.5", savingRoles && "animate-spin")} />
          {savingRoles ? 'Saving...' : 'Save Role Assignment'}
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {roles.map((role) => (
          <div key={role.id} className={cn("bg-slate-950 p-3.5 rounded-lg border flex flex-col justify-between space-y-2", role.color)}>
            <div>
              <label className="block text-xs font-semibold text-slate-200 tracking-wide mb-1">
                {role.title}
              </label>
              <p className="text-[10px] text-slate-400 leading-relaxed mb-2">
                {role.desc}
              </p>
            </div>

            <div>
              {models?.detected_local_models?.length > 0 ? (() => {
                const filtered = role.id === 'vision'
                  ? models.detected_local_models.filter((m: any) => m.model_type === 'vision')
                  : models.detected_local_models.filter((m: any) => (m.model_type || 'generative') === 'generative');
                return (
                <select
                  value={role.value}
                  onChange={(e) => role.setter(e.target.value)}
                  className="w-full px-3 py-1.5 bg-slate-900 border border-slate-800 rounded text-xs font-mono text-slate-200 focus:border-amber-500 focus:outline-none"
                >
                  {filtered.map((m: any) => (
                    <option key={m.id || m.name} value={m.id || m.name}>
                      {m.name} {m.size_mb ? `(${m.size_mb} MB)` : ''}
                    </option>
                  ))}
                </select>
                );
              })() : (
                <input
                  type="text"
                  value={role.value}
                  onChange={(e) => role.setter(e.target.value)}
                  className="w-full px-3 py-1.5 bg-slate-900 border border-slate-700 rounded text-xs font-mono text-slate-400"
                  placeholder="No models on disk — enter name"
                />
              )}
              {(() => {
                const match = isModelInstalled(role.value);
                return (
                  <div className="flex items-center justify-between gap-2 mt-2">
                    <span className="text-[9px] text-slate-500 font-mono">
                      💡 {role.rec}
                    </span>
                    {models?.detected_local_models?.length > 0 && (
                      <span className={cn(
                        "text-[9px] px-1.5 py-0.5 rounded font-mono border whitespace-nowrap shrink-0",
                        match
                          ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                          : "bg-amber-500/10 text-amber-400 border-amber-500/20"
                      )}>
                        {match ? `✓ On disk (${match.size_mb} MB)` : '⚠️ Not detected'}
                      </span>
                    )}
                  </div>
                );
              })()}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
