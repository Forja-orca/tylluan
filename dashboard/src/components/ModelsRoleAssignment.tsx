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
        { role: 'Inferencia Principal', val: rolePrimary },
        { role: 'Coordinador Nocturno', val: roleCoordinator },
        { role: 'Enrutamiento e Intenciones', val: roleRouting },
        { role: 'Análisis Visual', val: roleVision },
      ];
      const unverified = roleChecks.filter(r => r.val && !isModelInstalled(r.val));
      if (unverified.length > 0) {
        const warnMsg = `Atención: Los siguientes modelos no se detectaron en el inventario local de disco:\n\n` +
          unverified.map(u => `• ${u.role}: "${u.val}"`).join('\n') +
          `\n\n¿Deseas guardar esta asignación de todas formas?`;
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
      alert('Asignación de modelos por rol guardada exitosamente en tylluan.toml.');
    } catch (e: any) {
      alert(`Error guardando asignación de modelos por rol: ${e.message || String(e)}`);
    }
    setSavingRoles(false);
  };

  const roles = [
    {
      id: 'primary',
      title: '1. Inferencia Principal (Primary Agent)',
      desc: 'Responde a consultas de usuario y ejecuta herramientas (tylluan_do). Requiere buen seguimiento de instrucciones.',
      rec: 'Recomendado: 1.5B – 3B (ej. Qwen2.5-1.5B)',
      value: rolePrimary,
      setter: setRolePrimary,
      color: 'border-emerald-500/30'
    },
    {
      id: 'coordinator',
      title: '2. Coordinador Nocturno (Night Reasoner)',
      desc: 'Consolidación de memoria episódica en background. Prioriza razonamiento profundo sobre velocidad.',
      rec: 'Recomendado: 2B – 4B (ej. Gemma-2B / Qwen2.5-3B)',
      value: roleCoordinator,
      setter: setRoleCoordinator,
      color: 'border-violet-500/30'
    },
    {
      id: 'routing',
      title: '3. Enrutamiento e Intenciones (Routing & Intent)',
      desc: 'Clasificación ultra-rápida de intenciones y filtrado. Requiere latencia <50ms en CPU.',
      rec: 'Recomendado: <500M (ej. SmolLM2-135M / Qwen-0.5B)',
      value: roleRouting,
      setter: setRoleRouting,
      color: 'border-blue-500/30'
    },
    {
      id: 'vision',
      title: '4. Análisis Visual (Vision Model)',
      desc: 'Extracción de texto (OCR) y descripción de imágenes para la guild de visión.',
      rec: 'Recomendado: SmolVLM2-256M / Moondream',
      value: roleVision,
      setter: setRoleVision,
      color: 'border-cyan-500/30'
    },
  ];

  return (
    <div className="rounded-lg bg-slate-900/50 p-4 space-y-4">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-800/60 pb-3">
        <div>
          <h3 className="text-sm font-semibold text-slate-300 flex items-center gap-2">
            <ShieldCheck className="w-4 h-4 text-amber-400" /> Asignación de Modelos por Rol Cognitivo
          </h3>
          <p className="text-xs text-slate-400 mt-1">
            Cada módulo dentro de Tylluan requiere diferentes características de tamaño, velocidad y capacidad de razonamiento.
          </p>
        </div>
        <button
          onClick={handleSaveRoles}
          disabled={savingRoles}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 text-xs font-semibold rounded-lg transition-all disabled:opacity-50 shrink-0 cursor-pointer"
        >
          <Save className={cn("w-3.5 h-3.5", savingRoles && "animate-spin")} />
          {savingRoles ? 'Guardando...' : 'Guardar Asignación de Roles'}
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
                  placeholder="Sin modelos en disco — introduce nombre"
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
                        {match ? `✓ En disco (${match.size_mb} MB)` : '⚠️ No detectado'}
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
