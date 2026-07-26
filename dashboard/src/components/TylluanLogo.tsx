import React from 'react';
import { cn } from '../lib/utils';

interface TylluanLogoProps {
  size?: 'sm' | 'md' | 'lg' | 'xl';
  animated?: boolean;
  className?: string;
  showText?: boolean;
}

export function TylluanLogo({
  size = 'md',
  animated = true,
  className,
  showText = true,
}: TylluanLogoProps) {
  const sizeClasses = {
    sm: 'w-7 h-7',
    md: 'w-9 h-9',
    lg: 'w-12 h-12',
    xl: 'w-16 h-16',
  };

  const imageSizes = {
    sm: 'w-7 h-7',
    md: 'w-9 h-9',
    lg: 'w-12 h-12',
    xl: 'w-16 h-16',
  };

  return (
    <div className={cn('flex items-center gap-3', className)}>
      <div
        className={cn(
          'relative flex items-center justify-center rounded-xl overflow-hidden border border-cyan-400/40 bg-[#0B0F17] shadow-[0_0_16px_rgba(56,189,248,0.35)] transition-all duration-300 group',
          sizeClasses[size],
          animated && 'hover:shadow-[0_0_24px_rgba(56,189,248,0.6)]'
        )}
      >
        {/* Animated breathing glow ring */}
        {animated && (
          <div className="absolute inset-0 rounded-xl bg-gradient-to-tr from-cyan-500/20 via-sky-400/10 to-transparent opacity-75 animate-pulse" />
        )}

        <img
          src="/tylluan-logo.jpg"
          alt="Tylluan Owl Logo"
          className={cn(
            'object-cover rounded-xl transition-transform duration-500',
            imageSizes[size],
            animated && 'group-hover:scale-105'
          )}
          onError={(e) => {
            // Fallback to SVG if jpg loading fails
            (e.target as HTMLImageElement).src = '/tylluan-logo.svg';
          }}
        />
      </div>

      {showText && (
        <div className="font-mono leading-tight">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-slate-100">
            <span className="text-cyan-400">TYLLUAN</span>
            <span className="text-xs px-1.5 py-0.5 rounded bg-cyan-950/80 text-cyan-300 border border-cyan-500/30 text-[9px] uppercase tracking-wider">
              Sovereign
            </span>
          </div>
          <div className="text-[10px] text-slate-400 tracking-wider uppercase font-semibold">
            Cognitive Substrate
          </div>
        </div>
      )}
    </div>
  );
}
