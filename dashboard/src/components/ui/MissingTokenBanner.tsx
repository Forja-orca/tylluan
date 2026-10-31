import React from 'react';
import { ShieldAlert, KeyRound } from 'lucide-react';
import { TylluanLogo } from '../TylluanLogo';

interface MissingTokenBannerProps {
  onOpenAuthModal: () => void;
}

export function MissingTokenBanner({ onOpenAuthModal }: MissingTokenBannerProps) {
  return (
    <div
      role="region"
      aria-label="Aviso de autenticación requerida"
      className="m-3 md:m-6 rounded border border-salvia-500/30 bg-salvia-950/20 p-6 md:p-8 space-y-6 animate-in fade-in slide-in-from-top-2"
    >
      <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 border-b border-salvia-500/20 pb-5">
        <div className="flex items-center gap-3">
          <div className="w-12 h-12 rounded bg-salvia-500/10 border border-salvia-500/30 flex items-center justify-center shrink-0">
            <TylluanLogo size="md" showText={false} />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold bg-salvia-500/20 text-salvia-300 border border-salvia-500/30">
                <ShieldAlert className="w-3 h-3 text-salvia-400" />
                Seguridad Activa (dev_mode=false)
              </span>
            </div>
            <h3 className="text-lg font-bold text-slate-100 mt-1">
              Falta token de autenticación (dev_mode=false)
            </h3>
          </div>
        </div>

        <button
          type="button"
          onClick={onOpenAuthModal}
          className="inline-flex items-center gap-2 px-4 py-2.5 rounded bg-primary hover:bg-salvia-400 text-slate-950 text-xs font-bold transition-colors cursor-pointer focus-visible:ring-1 focus-visible:ring-primary focus-visible:outline-none"
        >
          <KeyRound className="w-4 h-4" />
          <span>Introducir Token de Acceso</span>
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs text-slate-300 leading-relaxed">
        <div className="space-y-2 bg-slate-900/60 border border-slate-800 rounded p-4">
          <h4 className="font-semibold text-slate-200 flex items-center gap-1.5">
            ¿Por qué veo este aviso?
          </h4>
          <p className="text-slate-400">
            El kernel de Tylluan está operando con la directiva estricta de seguridad (<code className="px-1.5 py-0.5 bg-slate-800 rounded text-salvia-300 font-mono">dev_mode = false</code>). Todas las rutas REST y suscripciones SSE exigen una cabecera <code className="px-1.5 py-0.5 bg-slate-800 rounded text-slate-200 font-mono">Authorization: Bearer</code> válida.
          </p>
        </div>

        <div className="space-y-2 bg-slate-900/60 border border-slate-800 rounded p-4">
          <h4 className="font-semibold text-slate-200 flex items-center gap-1.5">
            ¿Dónde encuentro mi token?
          </h4>
          <p className="text-slate-400">
            Tu token maestro generado por el kernel se encuentra en el archivo local <code className="px-1.5 py-0.5 bg-slate-800 rounded text-salvia-300 font-mono">.tylluan-token</code> en la raíz del espacio de trabajo. Cópialo y pégalo haciendo clic en el botón superior.
          </p>
        </div>
      </div>
    </div>
  );
}
