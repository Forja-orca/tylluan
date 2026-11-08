import React, { useState, useEffect, useMemo } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Cpu,
  Cloud,
  Check,
  Search,
  SlidersHorizontal,
  Layers,
  Binary
} from 'lucide-react';
import type { ModelCatalogEntry, ModelCapability } from './model-catalog-contract';

export interface TwoLevelModelSelectorProps {
  models: ModelCatalogEntry[];
  selectedModelId?: string;
  onSelectModel: (model: ModelCatalogEntry) => void;
  showAdminView?: boolean;
  requiredCapabilities?: ModelCapability[];
  storageKey?: string;
  className?: string;
}

const CAPABILITY_LABELS: Record<ModelCapability, { label: string; icon?: string }> = {
  reasoning: { label: 'Razonamiento' },
  coding: { label: 'Código' },
  tools: { label: 'Herramientas (Tools)' },
  json_output: { label: 'Salida JSON' },
  vision: { label: 'Visión' }
};

export const TwoLevelModelSelector: React.FC<TwoLevelModelSelectorProps> = ({
  models,
  selectedModelId,
  onSelectModel,
  showAdminView = false,
  requiredCapabilities = [],
  storageKey = 'tylluan_selected_model_id',
  className = ''
}) => {
  const [activeTab, setActiveTab] = useState<'generative' | 'decision'>('generative');
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCapabilities, setSelectedCapabilities] = useState<ModelCapability[]>(requiredCapabilities);
  const [isLevel2Open, setIsLevel2Open] = useState(false);
  const [currentSelectedId, setCurrentSelectedId] = useState<string>(() => {
    return selectedModelId || localStorage.getItem(storageKey) || '';
  });

  // Sincronizar selección externa o inicializar con el primer modelo disponible
  useEffect(() => {
    if (selectedModelId) {
      setCurrentSelectedId(selectedModelId);
      localStorage.setItem(storageKey, selectedModelId);
    } else if (!currentSelectedId && models.length > 0) {
      const firstAvailable = models.find(m => m.available && m.model_type === activeTab);
      if (firstAvailable) {
        setCurrentSelectedId(firstAvailable.id);
        localStorage.setItem(storageKey, firstAvailable.id);
        onSelectModel(firstAvailable);
      }
    }
  }, [selectedModelId, models, currentSelectedId, activeTab, onSelectModel, storageKey]);

  // Filtrado de modelos
  const filteredModels = useMemo(() => {
    return models.filter(m => {
      // Filtro de tipo
      if (m.model_type !== activeTab) return false;

      // Filtro de disponibilidad: en modo normal solo usables
      if (!showAdminView && !m.available) return false;

      // Filtro de búsqueda
      if (searchQuery) {
        const q = searchQuery.toLowerCase();
        const matchName = m.name.toLowerCase().includes(q);
        const matchProvider = m.provider.toLowerCase().includes(q);
        const matchFamily = m.family.toLowerCase().includes(q);
        if (!matchName && !matchProvider && !matchFamily) return false;
      }

      // Filtro de capacidades requeridas
      for (const cap of selectedCapabilities) {
        if (!m.capabilities.includes(cap)) return false;
      }

      return true;
    });
  }, [models, activeTab, showAdminView, searchQuery, selectedCapabilities]);

  // Nivel 1: Modelos más recientes / punteros
  const level1Models = useMemo(() => {
    return filteredModels.filter(m => m.is_latest_in_family);
  }, [filteredModels]);

  // Nivel 2: Modelos secundarios agrupados por proveedor
  const level2Grouped = useMemo(() => {
    const secondary = filteredModels.filter(m => !m.is_latest_in_family);
    const groups: Record<string, ModelCatalogEntry[]> = {};
    for (const model of secondary) {
      if (!groups[model.provider]) {
        groups[model.provider] = [];
      }
      groups[model.provider].push(model);
    }
    return groups;
  }, [filteredModels]);

  const currentModel = useMemo(() => {
    return models.find(m => m.id === currentSelectedId);
  }, [models, currentSelectedId]);

  const handleSelect = (model: ModelCatalogEntry) => {
    if (!model.available && !showAdminView) return;
    setCurrentSelectedId(model.id);
    localStorage.setItem(storageKey, model.id);
    onSelectModel(model);
  };

  const toggleCapability = (cap: ModelCapability) => {
    setSelectedCapabilities(prev =>
      prev.includes(cap) ? prev.filter(c => c !== cap) : [...prev, cap]
    );
  };

  return (
    <div className={`space-y-4 rounded border border-border bg-card p-4 text-foreground ${className}`} data-testid="two-level-model-selector">
      {/* Cabecera y Tabs Generativo / Decisión */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-border/60 pb-3">
        <div className="flex items-center gap-2">
          <Layers className="w-5 h-5 text-amber-400" />
          <h3 className="text-sm font-semibold font-mono tracking-tight">Selector de Modelos Soberano</h3>
        </div>
        <div className="flex items-center gap-1 rounded bg-muted/50 p-1 border border-border/50 text-xs font-mono" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'generative'}
            onClick={() => setActiveTab('generative')}
            className={`px-3 py-1 rounded transition-colors ${
              activeTab === 'generative'
                ? 'bg-background text-foreground font-semibold shadow-xs'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            Generativos (LLM)
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'decision'}
            onClick={() => setActiveTab('decision')}
            className={`flex items-center gap-1 px-3 py-1 rounded transition-colors ${
              activeTab === 'decision'
                ? 'bg-background text-foreground font-semibold shadow-xs'
                : 'text-muted-foreground hover:text-foreground'
            }`}
          >
            <Binary className="w-3.5 h-3.5 text-amber-400" />
            Decisión (T3144)
          </button>
        </div>
      </div>

      {/* Barra de Búsqueda y Filtros de Capacidad */}
      <div className="space-y-2.5">
        <div className="relative">
          <Search className="absolute left-3 top-2.5 w-4 h-4 text-muted-foreground" />
          <input
            type="text"
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            placeholder={`Buscar por nombre, familia (${activeTab === 'generative' ? 'qwen, claude...' : 'laya, drex...'})`}
            className="w-full pl-9 pr-3 py-1.5 text-xs font-mono rounded bg-background border border-border focus:outline-none focus:ring-1 focus:ring-amber-400"
            aria-label="Buscar modelo"
          />
        </div>

        {/* Chips de Capacidad */}
        {activeTab === 'generative' && (
          <div className="flex flex-wrap items-center gap-1.5 pt-1">
            <span className="text-[11px] font-mono text-muted-foreground flex items-center gap-1 mr-1">
              <SlidersHorizontal className="w-3 h-3" />
              Filtrar capacidad:
            </span>
            {(Object.keys(CAPABILITY_LABELS) as ModelCapability[]).map(cap => {
              const active = selectedCapabilities.includes(cap);
              return (
                <button
                  key={cap}
                  type="button"
                  onClick={() => toggleCapability(cap)}
                  className={`px-2 py-0.5 rounded text-[10px] font-mono transition-colors border ${
                    active
                      ? 'bg-amber-500/20 text-amber-300 border-amber-500/40 font-semibold'
                      : 'bg-muted/40 text-muted-foreground border-border hover:bg-muted'
                  }`}
                >
                  {CAPABILITY_LABELS[cap].label}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Modelo Seleccionado Actualmente */}
      {currentModel && (
        <div className="rounded border border-amber-500/30 bg-amber-500/5 p-3 text-xs font-mono flex items-center justify-between gap-3">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground text-[10px] uppercase tracking-wider">Activo para agente:</span>
              <strong className="text-amber-300 font-semibold">{currentModel.name}</strong>
              <span className="px-1.5 py-0.2 rounded bg-muted/80 text-[10px] text-muted-foreground border border-border">
                {currentModel.provider}
              </span>
              {currentModel.origin === 'tylluan' ? (
                <span className="px-1.5 py-0.2 rounded bg-amber-500/15 text-amber-300 text-[10px] border border-amber-500/30 font-semibold">
                  Suelo Tylluan
                </span>
              ) : (
                <span className="px-1.5 py-0.2 rounded bg-sky-500/15 text-sky-300 text-[10px] border border-sky-500/30">
                  Techo Operador
                </span>
              )}
              {currentModel.pinned && (
                <span className="px-1.5 py-0.2 rounded bg-amber-500/20 text-amber-300 text-[10px] border border-amber-500/40">
                  Fijado
                </span>
              )}
            </div>
            <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
              <span>Contexto: {currentModel.context_window.toLocaleString()} tok</span>
              {currentModel.decision_metadata && (
                <span>Interfaz: {currentModel.decision_metadata.interface} (máx {currentModel.decision_metadata.max_options || 2})</span>
              )}
              {currentModel.ram_private_mb && <span>RAM Privada: {currentModel.ram_private_mb} MB</span>}
            </div>
          </div>
          <div className="shrink-0 flex items-center gap-1 px-2 py-1 rounded bg-emerald-500/10 border border-emerald-500/30 text-emerald-400 text-[11px]">
            <Check className="w-3.5 h-3.5" />
            Listo
          </div>
        </div>
      )}

      {/* NIVEL 1: Punteros / Más Recientes */}
      <div className="space-y-2">
        <div className="flex items-center justify-between text-xs font-mono font-semibold text-muted-foreground px-1">
          <span>Nivel 1: Modelos Más Recientes (Recomendados)</span>
          <span className="text-[11px] text-muted-foreground">{level1Models.length} disponibles</span>
        </div>

        {level1Models.length === 0 ? (
          <div className="p-4 text-center text-xs font-mono text-muted-foreground border border-dashed border-border rounded">
            No hay modelos recientes que coincidan con los filtros.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-2" role="listbox" aria-label="Modelos Nivel 1">
            {level1Models.map(model => {
              const isSelected = model.id === currentSelectedId;
              return (
                <button
                  key={model.id}
                  type="button"
                  role="option"
                  aria-selected={isSelected}
                  disabled={!model.available && !showAdminView}
                  onClick={() => handleSelect(model)}
                  className={`text-left p-2.5 rounded border transition-all cursor-pointer flex items-start justify-between gap-2.5 ${
                    isSelected
                      ? 'bg-amber-500/15 border-amber-500/50 shadow-xs'
                      : model.available
                      ? 'bg-card hover:bg-muted/60 border-border'
                      : 'bg-muted/30 border-border/40 opacity-60 cursor-not-allowed'
                  }`}
                >
                  <div className="space-y-1 min-w-0">
                    <div className="flex items-center gap-1.5 flex-wrap">
                      <span className="text-xs font-semibold font-mono truncate">{model.name}</span>
                      {model.locality === 'local' ? (
                        <span className="px-1 py-0.2 rounded text-[9px] font-mono bg-blue-500/15 text-blue-300 border border-blue-500/30 flex items-center gap-0.5">
                          <Cpu className="w-2.5 h-2.5" /> Local
                        </span>
                      ) : (
                        <span className="px-1 py-0.2 rounded text-[9px] font-mono bg-purple-500/15 text-purple-300 border border-purple-500/30 flex items-center gap-0.5">
                          <Cloud className="w-2.5 h-2.5" /> Remoto
                        </span>
                      )}
                      {model.origin === 'tylluan' ? (
                        <span className="px-1 py-0.2 rounded text-[9px] font-mono bg-amber-500/15 text-amber-300 border border-amber-500/30 font-semibold">
                          Suelo
                        </span>
                      ) : (
                        <span className="px-1 py-0.2 rounded text-[9px] font-mono bg-sky-500/15 text-sky-300 border border-sky-500/30">
                          Techo
                        </span>
                      )}
                      {model.pinned && (
                        <span className="px-1 py-0.2 rounded text-[9px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/40">
                          Fijado
                        </span>
                      )}
                    </div>
                    <div className="flex items-center gap-2 text-[10px] font-mono text-muted-foreground flex-wrap">
                      <span>{model.provider}</span>
                      <span>·</span>
                      <span>{model.context_window.toLocaleString()} tok</span>
                      {model.ram_private_mb && (
                        <>
                          <span>·</span>
                          <span>{model.ram_private_mb} MB priv.</span>
                        </>
                      )}
                      {model.decision_metadata && (
                        <span>· ~{model.decision_metadata.typical_latency_ms}ms</span>
                      )}
                    </div>
                  </div>

                  <div className="shrink-0 flex items-center gap-1">
                    {isSelected && <Check className="w-4 h-4 text-amber-400" />}
                    {!model.available && (
                      <span className="px-1.5 py-0.5 rounded text-[9px] font-mono bg-rose-500/15 text-rose-300 border border-rose-500/30">
                        {model.unavailable_reason || 'No disp.'}
                      </span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* NIVEL 2: Menú 'Otros Modelos' (Agrupados por Proveedor) */}
      <div className="border-t border-border/60 pt-3">
        <button
          type="button"
          onClick={() => setIsLevel2Open(prev => !prev)}
          className="flex items-center justify-between w-full text-xs font-mono font-semibold text-muted-foreground hover:text-foreground py-1.5 px-2 rounded hover:bg-muted/40 transition-colors"
          aria-expanded={isLevel2Open}
        >
          <span className="flex items-center gap-1.5">
            {isLevel2Open ? <ChevronDown className="w-4 h-4 text-amber-400" /> : <ChevronRight className="w-4 h-4" />}
            Nivel 2: Otros modelos del catálogo (versiones secundarias y proveedores)
          </span>
          <span className="text-[11px] text-muted-foreground">
            {Object.values(level2Grouped).reduce((acc, arr) => acc + arr.length, 0)} modelos
          </span>
        </button>

        {isLevel2Open && (
          <div className="mt-3 space-y-3 pl-2 border-l border-border/60">
            {Object.keys(level2Grouped).length === 0 ? (
              <p className="text-xs font-mono text-muted-foreground py-2 pl-2">
                No hay modelos secundarios disponibles con los filtros actuales.
              </p>
            ) : (
              Object.entries(level2Grouped).map(([provider, provModels]) => (
                <div key={provider} className="space-y-1.5">
                  <h4 className="text-[11px] font-mono font-semibold uppercase tracking-wider text-muted-foreground pl-1">
                    {provider} ({provModels.length})
                  </h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-1.5">
                    {provModels.map(model => {
                      const isSelected = model.id === currentSelectedId;
                      return (
                        <button
                          key={model.id}
                          type="button"
                          disabled={!model.available && !showAdminView}
                          onClick={() => handleSelect(model)}
                          className={`text-left p-2 rounded border text-xs font-mono transition-all flex items-center justify-between gap-2 ${
                            isSelected
                              ? 'bg-amber-500/15 border-amber-500/50'
                              : model.available
                              ? 'bg-card/70 hover:bg-muted/60 border-border'
                              : 'bg-muted/20 border-border/40 opacity-50 cursor-not-allowed'
                          }`}
                        >
                          <div className="truncate min-w-0">
                            <div className="flex items-center gap-1 flex-wrap">
                              <span className="font-semibold block truncate">{model.name}</span>
                              {model.origin === 'tylluan' ? (
                                <span className="px-1 py-0.2 rounded text-[8px] font-mono bg-amber-500/15 text-amber-300 border border-amber-500/30 font-semibold">
                                  Suelo
                                </span>
                              ) : (
                                <span className="px-1 py-0.2 rounded text-[8px] font-mono bg-sky-500/15 text-sky-300 border border-sky-500/30">
                                  Techo
                                </span>
                              )}
                              {model.pinned && (
                                <span className="px-1 py-0.2 rounded text-[8px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/40">
                                  Fijado
                                </span>
                              )}
                            </div>
                            <span className="text-[10px] text-muted-foreground">
                              {model.context_window.toLocaleString()} tok · {model.locality}
                              {model.ram_private_mb ? ` · ${model.ram_private_mb} MB priv.` : ''}
                            </span>
                          </div>
                          {isSelected && <Check className="w-3.5 h-3.5 text-amber-400 shrink-0" />}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
};
