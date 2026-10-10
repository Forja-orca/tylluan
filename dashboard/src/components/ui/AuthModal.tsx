import React, { useState, useEffect } from 'react';
import { Key, X, Check, ShieldAlert, Eye, EyeOff, Clipboard } from 'lucide-react';

interface AuthModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSave: (token: string) => void;
  initialToken?: string;
}

export function AuthModal({
  isOpen,
  onClose,
  onSave,
  initialToken = '',
}: AuthModalProps) {
  const [tokenInput, setTokenInput] = useState('');
  const [showToken, setShowToken] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      const stored = initialToken || localStorage.getItem('tylluan_token') || '';
      setTokenInput(stored);
      setError(null);
    }
  }, [isOpen, initialToken]);

  if (!isOpen) return null;

  const handleSave = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = tokenInput.trim();
    if (!trimmed) {
      setError('Por favor, introduce un token válido.');
      return;
    }
    setError(null);
    onSave(trimmed);
  };

  const handlePasteClipboard = async () => {
    try {
      if (navigator.clipboard && navigator.clipboard.readText) {
        const text = await navigator.clipboard.readText();
        if (text) {
          setTokenInput(text.trim());
          setError(null);
        }
      }
    } catch {
      // Clipboard access denied or unsupported
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="auth-modal-title"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/85 backdrop-blur-sm animate-in fade-in duration-200"
    >
      <div className="relative w-full max-w-lg bg-slate-900 border border-slate-800 rounded-xl shadow-2xl overflow-hidden p-6 space-y-5">
        <button
          onClick={onClose}
          aria-label="Cerrar modal de autenticación"
          className="absolute top-4 right-4 text-slate-400 hover:text-slate-200 transition-colors p-1 rounded-lg hover:bg-slate-800"
        >
          <X className="w-4 h-4" />
        </button>

        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl border text-amber-400 bg-amber-500/10 border-amber-500/20 shrink-0">
            <Key className="w-6 h-6" />
          </div>
          <div className="space-y-1">
            <h2 id="auth-modal-title" className="text-lg font-semibold text-slate-100 flex items-center gap-2">
              Autenticación Requerida
            </h2>
            <p className="text-xs text-slate-400 leading-relaxed">
              El kernel de Tylluan Nexus está protegido por token Bearer en modo producción.
              Introduce el token generado en <code className="text-amber-300 font-mono text-[11px] bg-slate-800 px-1 py-0.5 rounded">.tylluan-token</code> para autorizar las solicitudes.
            </p>
          </div>
        </div>

        <form onSubmit={handleSave} className="space-y-4">
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <label htmlFor="bearer-token-input" className="text-xs font-medium text-slate-300">
                Bearer Token
              </label>
              <button
                type="button"
                onClick={handlePasteClipboard}
                className="text-[11px] text-teal-400 hover:text-teal-300 flex items-center gap-1 transition-colors"
              >
                <Clipboard className="w-3 h-3" />
                Pegar portapapeles
              </button>
            </div>

            <div className="relative">
              <input
                id="bearer-token-input"
                type={showToken ? 'text' : 'password'}
                value={tokenInput}
                onChange={(e) => {
                  setTokenInput(e.target.value);
                  if (error) setError(null);
                }}
                placeholder="Pega aquí el contenido de .tylluan-token..."
                className="w-full bg-slate-950 border border-slate-700 focus:border-amber-500 focus:ring-1 focus:ring-amber-500 rounded-lg px-3 py-2.5 pr-10 text-xs font-mono text-slate-200 placeholder-slate-600 outline-none transition-colors"
                autoFocus
                autoComplete="off"
                spellCheck={false}
              />
              <button
                type="button"
                onClick={() => setShowToken(!showToken)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 p-1"
                aria-label={showToken ? 'Ocultar token' : 'Mostrar token'}
              >
                {showToken ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
              </button>
            </div>

            {error && (
              <p className="text-xs text-rose-400 flex items-center gap-1.5 pt-1">
                <ShieldAlert className="w-3.5 h-3.5" />
                {error}
              </p>
            )}
          </div>

          <div className="bg-slate-950/60 border border-slate-800/80 rounded-lg p-3 text-[11px] text-slate-400 space-y-1">
            <p className="flex items-center gap-1 text-slate-300 font-medium">
              <span>📍</span> Ubicación del token en el sistema
            </p>
            <p className="font-mono text-slate-500 text-[10px]">
              En la raíz del proyecto Tylluan: <span className="text-slate-300">.tylluan-token</span>
            </p>
            <p className="text-[10px] text-slate-500">
              El token se conservará localmente en <span className="text-slate-400">localStorage</span> para mantener la sesión activa.
            </p>
          </div>

          <div className="flex items-center justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-xs font-medium text-slate-400 hover:text-slate-200 hover:bg-slate-800 rounded-lg transition-colors"
            >
              Cancelar
            </button>
            <button
              type="submit"
              className="px-4 py-2 text-xs font-medium bg-amber-700 hover:bg-amber-600 text-white rounded-lg flex items-center gap-1.5 transition-colors shadow-lg shadow-amber-900/20"
            >
              <Check className="w-3.5 h-3.5" />
              Guardar y Conectar
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
