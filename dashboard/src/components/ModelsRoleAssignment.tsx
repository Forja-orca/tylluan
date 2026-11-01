import React, { useState } from 'react';
import type { NexusBridge } from '../lib/nexus-bridge';
import { Save, ShieldCheck } from 'lucide-react';
import { cn } from '../lib/utils';

interface DetectedLocalModel {
  id?: string;
  name: string;
  model_type?: string;
  size_mb?: number;
}

interface Props {
  bridge: NexusBridge | null;
  models: { detected_local_models?: DetectedLocalModel[] } | null;
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
    return models.detected_local_models.find((m) => {
      const mId = (m.id || '').toLowerCase();
      const mName = (m.name || '').toLowerCase();
      const val = modelVal.toLowerCase();
      return mId === val || mName === val || mId.includes(val) || val.includes(mId) || mName.includes(val) || val.includes(mName);
    }) || null;
  };

  const handleSaveRoles = async () => {
    if (!bridge) return;

    if (models?.detected_local_models && models.detected_local_models.length > 0) {
      const roleChecks = [
        { role: 'Inferencia Principal', val: rolePrimary },
        { role: 'Razonador Nocturno', val: roleCoordinator },
        { role: 'Enrutamiento e Intención', val: roleRouting },
        { role: 'Análisis Visual', val: roleVision },
      ];
      const unverified = roleChecks.filter(r => r.val && !isModelInstalled(r.val));
      if (unverified.length > 0) {
        const warnMsg = `Aviso: Los siguientes modelos no fueron detectados en el inventario local de disco:\n\n` +
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
      alert('Model role assignment saved successfully in tylluan.toml.');
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      alert(`Error saving model role assignment: ${msg}`);
    }
    setSavingRoles(false);
  };

  const roles = [
    {
      id: 'primary',
      title: '1. Inferencia Principal (Agente Primario)',
      desc: 'Responde consultas de usuario y ejecuta herramientas (tylluan_do). Requiere alta fidelidad en seguimiento de instrucciones.',
      rec: 'Recomendado: 1.5B – 3B (ej. Qwen2.5-1.5B)',
      value: rolePrimary,
      setter: setRolePrimary,
      color: 'border-emerald-500/30'
    },
    {
      id: 'coordinator',
      title: '2. Razonador Nocturno (Coordinador)',
      desc: 'Consolidación en segundo plano de memoria episódica. Prioriza razonamiento profundo sobre latencia.',
      rec: 'Recomendado: 2B – 4B (ej. Gemma-2B / Qwen2.5-3B)',
      value: roleCoordinator,
      setter: setRoleCoordinator,
      color: 'border-violet-500/30'
    },
    {
      id: 'routing',
      title: '3. Enrutamiento e Intención',
      desc: 'Clasificación ultra-rápida de intención y filtrado. Requiere latencia <50ms en CPU.',
      rec: 'Recomendado: <500M (ej. SmolLM2-135M / Qwen-0.5B)',
      value: roleRouting,
      setter: setRoleRouting,
      color: 'border-blue-500/30'
    },
    {
      id: 'vision',
      title: '4. Análisis Visual (Modelo de Visión)',
      desc: 'Extracción de texto (OCR) y descripción de imágenes para el guild de visión.',
      rec: 'Recomendado: SmolVLM2-256M / Moondream',
      value: roleVision,
      setter: setRoleVision,
      color: 'border-sky-500/30'
    },
  ];

  return (
    <div className="rounded-none bg-white dark:bg-card border border-[#ACA899] dark:border-border p-3 space-y-3 font-sans">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 border-b border-[#ACA899] dark:border-border pb-2.5">
        <div>
          <h3 className="text-xs font-bold text-[#01285F] dark:text-foreground flex items-center gap-1.5">
            <ShieldCheck className="w-3.5 h-3.5 text-[#9E4024]" aria-hidden="true" />
            Asignación de Roles Cognitivos
          </h3>
          <p className="text-[11px] text-[#444444] dark:text-muted-foreground mt-0.5">
            Cada módulo interno de Tylluan requiere diferentes compromisos entre tamaño de modelo, latencia y capacidad de razonamiento.
          </p>
        </div>
        <button
          onClick={handleSaveRoles}
          disabled={savingRoles}
          className="flex items-center gap-1.5 px-3 py-1 bg-white hover:bg-[#F5F4EA] border border-[#ACA899] text-[#000000] dark:text-foreground text-[11px] font-bold rounded-sm transition-colors disabled:opacity-50 shrink-0 cursor-pointer self-start sm:self-auto"
        >
          <Save className={cn("w-3 h-3 text-[#9E4024]", savingRoles && "animate-spin")} />
          {savingRoles ? 'Guardando...' : 'Guardar Asignación'}
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-2.5">
        {roles.map((role) => (
          <fieldset key={role.id} className="bg-white dark:bg-card border border-[#ACA899] dark:border-border p-2.5 rounded-none flex flex-col justify-between space-y-2">
            <legend className="text-[11px] font-bold text-[#9E4024] dark:text-primary px-1">
              {role.title}
            </legend>
            <div>
              <p className="text-[10px] text-[#444444] dark:text-muted-foreground leading-normal mb-1.5">
                {role.desc}
              </p>
            </div>

            <div>
              {models?.detected_local_models && models.detected_local_models.length > 0 ? (() => {
                const filtered = role.id === 'vision'
                  ? models.detected_local_models.filter((m) => m.model_type === 'vision')
                  : models.detected_local_models.filter((m) => (m.model_type || 'generative') === 'generative');
                return (
                <select
                  value={role.value}
                  onChange={(e) => role.setter(e.target.value)}
                  className="w-full px-2 py-1 bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none text-[11px] font-mono text-[#000000] dark:text-foreground focus:outline-none"
                >
                  {filtered.map((m) => (
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
                  className="w-full px-2 py-1 bg-white dark:bg-muted border border-[#7F9DB9] dark:border-border rounded-none text-[11px] font-mono text-[#000000] dark:text-foreground focus:outline-none"
                  placeholder="Sin modelos en disco — introduzca nombre"
                />
              )}
              {(() => {
                const match = isModelInstalled(role.value);
                return (
                  <div className="flex items-center justify-between gap-2 mt-1.5 pt-1 border-t border-[#ECE9D8] dark:border-border text-[10px]">
                    <span className="text-[#555555] dark:text-muted-foreground font-mono">
                      {role.rec}
                    </span>
                    {(models?.detected_local_models?.length ?? 0) > 0 && (
                      <span className={cn(
                        "px-1.5 py-0.2 rounded-none font-mono text-[10px] border whitespace-nowrap shrink-0",
                        match
                          ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/30"
                          : "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-500/30"
                      )}>
                        {match ? `En disco (${match.size_mb} MB)` : 'No detectado'}
                      </span>
                    )}
                  </div>
                );
              })()}
            </div>
          </fieldset>
        ))}
      </div>
    </div>
  );
}
