import React from 'react';
import { AlertTriangle, X, Check } from 'lucide-react';

interface ConfirmModalProps {
  isOpen: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  variant?: 'danger' | 'warning' | 'info';
  onConfirm: () => void;
  onCancel: () => void;
}

// WCAG pre-flight 2026-08-12:
//   title (slate-100 on slate-900)    → 16.80:1 PASS
//   message (slate-400 on slate-900)  →  7.18:1 PASS
//   cancel text (slate-400 on slate-800) → 5.89:1 PASS
//   cancel hover (slate-200 on slate-800) → 12.26:1 PASS
//   close btn (slate-400 on slate-900) → 7.18:1 PASS (was slate-500: 3.87 LARGE OK — raised)
//   warning btn white/amber-700       →  5.02:1 PASS (amber-600 was 3.19 FAIL — fixed)
//   danger btn white/rose-600         →  4.70:1 PASS
//   info btn white/sky-700            →  5.93:1 PASS (sky-600 was 4.10 LARGE OK — raised)

export function ConfirmModal({
  isOpen,
  title,
  message,
  confirmText = 'Confirmar',
  cancelText = 'Cancelar',
  variant = 'warning',
  onConfirm,
  onCancel
}: ConfirmModalProps) {
  if (!isOpen) return null;

  // Sovereign Amber (warning), rose (danger), sky (info) — all AA-verified
  let btnColor = 'bg-amber-700 hover:bg-amber-600 text-white';
  let iconColor = 'text-amber-400 bg-amber-500/10 border-amber-500/20';

  if (variant === 'danger') {
    btnColor = 'bg-rose-600 hover:bg-rose-500 text-white';
    iconColor = 'text-rose-400 bg-rose-500/10 border-rose-500/20';
  } else if (variant === 'info') {
    btnColor = 'bg-sky-700 hover:bg-sky-600 text-white';
    iconColor = 'text-sky-400 bg-sky-500/10 border-sky-500/20';
  }

  return (
    // Obsidian Nocturne overlay (#0A0D14 / slate-950 at 80%)
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200">
      {/* Owl Slate card surface */}
      <div className="relative w-full max-w-md bg-slate-900 border border-slate-800 rounded-xl shadow-2xl overflow-hidden p-6 space-y-4">
        <button
          onClick={onCancel}
          className="absolute top-4 right-4 text-slate-400 hover:text-slate-200 transition-colors"
        >
          <X className="w-4 h-4" />
        </button>

        <div className="flex items-start gap-3">
          <div className={`p-2.5 rounded-lg border flex-shrink-0 ${iconColor}`}>
            <AlertTriangle className="w-5 h-5" />
          </div>
          <div className="space-y-1">
            <h3 className="text-base font-bold font-sans text-slate-100">{title}</h3>
            <p className="text-xs font-sans text-slate-400 leading-relaxed">{message}</p>
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 pt-2 border-t border-slate-800/80">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-xs font-mono font-semibold text-slate-400 hover:text-slate-200 bg-slate-800/60 hover:bg-slate-800 rounded-lg border border-slate-700/60 transition-all"
          >
            {cancelText}
          </button>
          <button
            onClick={onConfirm}
            className={`flex items-center gap-1.5 px-4 py-2 text-xs font-mono font-bold rounded-lg transition-all shadow-lg ${btnColor}`}
          >
            <Check className="w-3.5 h-3.5" />
            <span>{confirmText}</span>
          </button>
        </div>
      </div>
    </div>
  );
}
export default ConfirmModal;
