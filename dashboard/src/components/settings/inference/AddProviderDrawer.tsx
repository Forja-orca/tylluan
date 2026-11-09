import React, { useState, useEffect, useRef, useCallback } from 'react';
import { X, Key, Globe, Layers, Lock, RefreshCw } from 'lucide-react';
import type { NexusBridge } from '../../../lib/nexus-bridge';

export interface AddProviderDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
  onProviderAdded?: () => void;
  kernelSupportsPost?: boolean;
}

export function AddProviderDrawer({
  isOpen,
  onClose,
  bridge,
  notify,
  onProviderAdded,
  kernelSupportsPost = true,
}: AddProviderDrawerProps) {
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [modelsInput, setModelsInput] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const drawerRef = useRef<HTMLDivElement>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);
  const triggerRef = useRef<HTMLElement | null>(null);

  const handleClose = useCallback(() => {
    // Seguridad estricta (T3696 / SPEC §6): NUNCA retener la clave en estado tras cerrar
    setApiKey('');
    onClose();
  }, [onClose]);

  // Focus trap y escape para accesibilidad WCAG AA
  useEffect(() => {
    if (isOpen) {
      triggerRef.current = document.activeElement as HTMLElement | null;

      const focusTimer = setTimeout(() => {
        nameInputRef.current?.focus();
      }, 50);

      const handleKeyDown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          handleClose();
          return;
        }

        if (e.key === 'Tab') {
          if (!drawerRef.current) return;
          const focusable = drawerRef.current.querySelectorAll<HTMLElement>(
            'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
          );
          if (focusable.length === 0) return;

          const first = focusable[0];
          const last = focusable[focusable.length - 1];

          if (e.shiftKey) {
            if (document.activeElement === first) {
              e.preventDefault();
              last.focus();
            }
          } else {
            if (document.activeElement === last) {
              e.preventDefault();
              first.focus();
            }
          }
        }
      };

      window.addEventListener('keydown', handleKeyDown);
      return () => {
        clearTimeout(focusTimer);
        window.removeEventListener('keydown', handleKeyDown);
        triggerRef.current?.focus();
      };
    }
  }, [isOpen, handleClose]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!kernelSupportsPost) {
      notify('Función no disponible hasta que el kernel incorpore soporte de proveedores externos', 'error');
      return;
    }

    if (!name.trim() || !baseUrl.trim()) {
      notify('Por favor completa el nombre y la URL base del proveedor', 'error');
      return;
    }

    const modelsList = modelsInput
      .split(',')
      .map(m => m.trim())
      .filter(Boolean);

    setIsSubmitting(true);
    try {
      if (!bridge) throw new Error('Bridge no conectado');

      // Obtener ETag actual de /api/v1/config para cumplir R2 (D-101 / D-157)
      let etag: string | undefined;
      try {
        const token = bridge?.getToken();
        const configRes = await window.fetch('/api/v1/config', {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        });
        etag = configRes.headers.get('x-config-etag') || undefined;
      } catch {
        // Fallback si no se puede leer la cabecera
      }

      const payload = {
        name: name.trim().toLowerCase().replace(/\s+/g, '-'),
        base_url: baseUrl.trim(),
        api_key: apiKey.trim() || undefined,
        models: modelsList,
        etag: etag || undefined,
      };

      const res = await bridge.fetchRaw('/api/v1/external-providers', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      }) as { ok?: boolean; error?: string } | undefined;

      if (res && res.ok === false) {
        throw new Error(res.error || 'Fallo al guardar el proveedor');
      }

      notify(`Proveedor «${name}» añadido correctamente`, 'info');
      onProviderAdded?.();
      handleClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      notify(`Error al guardar proveedor: ${msg}`, 'error');
    } finally {
      // Regla no negociable de seguridad (T3696): NUNCA persistir la clave en estado
      setApiKey('');
      setIsSubmitting(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 overflow-hidden bg-black/60 backdrop-blur-sm flex justify-end transition-opacity duration-200"
      aria-modal="true"
      role="dialog"
      aria-labelledby="drawer-add-provider-title"
    >
      <div
        ref={drawerRef}
        className="w-full max-w-md bg-card border-l border-border h-full shadow-2xl flex flex-col justify-between overflow-y-auto animate-in slide-in-from-right duration-200"
      >
        {/* Cabecera del Drawer */}
        <div className="p-5 border-b border-border flex items-center justify-between">
          <div className="space-y-1">
            <h3 id="drawer-add-provider-title" className="text-base font-bold text-foreground flex items-center gap-2">
              <Globe className="w-5 h-5 text-primary" aria-hidden="true" />
              Añadir proveedor de inferencia
            </h3>
            <p className="text-xs text-muted-foreground">
              Conecta una API compatible con OpenAI (local o en la nube).
            </p>
          </div>
          <button
            type="button"
            onClick={handleClose}
            className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors cursor-pointer"
            aria-label="Cerrar formulario"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Formulario de 4 campos */}
        <form onSubmit={handleSubmit} className="p-5 space-y-4 flex-1">
          {/* Campo 1: Nombre */}
          <div className="space-y-1.5">
            <label htmlFor="provider-name-input" className="text-xs font-semibold text-foreground block">
              Nombre del proveedor
            </label>
            <input
              id="provider-name-input"
              ref={nameInputRef}
              type="text"
              required
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="ej: OpenRouter, Groq, Together"
              className="w-full px-3 py-2 text-xs rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          {/* Campo 2: URL base */}
          <div className="space-y-1.5">
            <label htmlFor="provider-url-input" className="text-xs font-semibold text-foreground block">
              URL base (OpenAI-compatible)
            </label>
            <input
              id="provider-url-input"
              type="url"
              required
              value={baseUrl}
              onChange={e => setBaseUrl(e.target.value)}
              placeholder="https://openrouter.ai/api/v1"
              className="w-full px-3 py-2 text-xs font-mono rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          {/* Campo 3: Clave de API (type="password", jamás en almacenamiento persistente del cliente) */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="provider-key-input" className="text-xs font-semibold text-foreground flex items-center gap-1.5">
                <Key className="w-3.5 h-3.5 text-muted-foreground" />
                <span>Clave de API</span>
              </label>
              <span className="text-[10px] text-muted-foreground flex items-center gap-1 font-mono">
                <Lock className="w-3 h-3 text-emerald-500" />
                <span>Almacenamiento seguro</span>
              </span>
            </div>
            <input
              id="provider-key-input"
              type="password"
              autoComplete="new-password"
              value={apiKey}
              onChange={e => setApiKey(e.target.value)}
              placeholder="sk-..."
              className="w-full px-3 py-2 text-xs font-mono rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
            <p className="text-[11px] text-muted-foreground">
              La clave se guarda en el almacén seguro del sistema; nunca en el navegador.
            </p>
          </div>

          {/* Campo 4: Modelos */}
          <div className="space-y-1.5 pt-1">
            <label htmlFor="provider-models-input" className="text-xs font-semibold text-foreground flex items-center gap-1.5">
              <Layers className="w-3.5 h-3.5 text-muted-foreground" />
              <span>Modelos disponibles (separados por coma)</span>
            </label>
            <input
              id="provider-models-input"
              type="text"
              value={modelsInput}
              onChange={e => setModelsInput(e.target.value)}
              placeholder="meta-llama/llama-3.3-70b-instruct, qwen/qwen-2.5-72b"
              className="w-full px-3 py-2 text-xs font-mono rounded-md border border-border bg-background text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>
        </form>

        {/* Pie del Drawer con botones de Guardar y Cancelar */}
        <div className="p-5 border-t border-border bg-muted/20 space-y-2">
          <button
            type="button"
            disabled={!kernelSupportsPost || isSubmitting}
            onClick={handleSubmit}
            className={`w-full py-2 px-4 text-xs font-bold rounded-md transition-all flex items-center justify-center gap-2 ${
              kernelSupportsPost
                ? 'bg-primary text-primary-foreground hover:bg-primary/90 cursor-pointer shadow-sm'
                : 'bg-muted text-muted-foreground border border-border opacity-70 cursor-not-allowed'
            }`}
            title={
              kernelSupportsPost
                ? 'Guardar proveedor'
                : 'disponible cuando el kernel lo soporte'
            }
          >
            {isSubmitting ? (
              <>
                <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                <span>Guardando...</span>
              </>
            ) : kernelSupportsPost ? (
              <span>Guardar</span>
            ) : (
              <span>Guardar (disponible cuando el kernel lo soporte)</span>
            )}
          </button>

          <button
            type="button"
            onClick={handleClose}
            className="w-full py-1.5 px-4 text-xs font-medium rounded-md border border-border bg-background hover:bg-muted text-foreground transition-colors cursor-pointer"
          >
            Cancelar
          </button>
        </div>
      </div>
    </div>
  );
}
