import React from 'react';
import { cn } from '../lib/utils';

interface TylluanLogoProps {
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl';
  animated?: boolean;
  className?: string;
  showText?: boolean;
}

export function TylluanLogo({
  size = 'md',
  animated: _animated = false,
  className,
  showText = true,
}: TylluanLogoProps) {

  const containerSizes = {
    sm: 'w-8 h-8',
    md: 'w-10 h-10',
    lg: 'w-14 h-14',
    xl: 'w-20 h-20',
    '2xl': 'w-28 h-28',
  };

  const ringPadding = {
    sm: 'p-0.5',
    md: 'p-1',
    lg: 'p-1.5',
    xl: 'p-2',
    '2xl': 'p-2.5',
  };

  // Sovereign Color Tokens: Salvia Accent
  const ACCENT_COLOR = 'hsl(var(--primary))';

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* Cybernetic Owl Avatar Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Sovereign Card Base */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-card border border-primary/30 overflow-hidden flex items-center justify-center backdrop-blur-md transition-all duration-300 group-hover:border-primary/60',
            ringPadding[size]
          )}
        >
          {/* Base Layer: Official Owl SVG Clean Vector */}
          <div className="w-full h-full flex items-center justify-center relative z-10 p-1">
            <svg viewBox="0 0 30 30" className="w-full h-full text-foreground" aria-label="Tylluan Owl Official Logo">
              <path
                fill="currentColor"
                className="text-card"
                stroke={ACCENT_COLOR}
                strokeWidth="0.8"
                d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
              />
              <path fill={ACCENT_COLOR} d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
              <polygon fill={ACCENT_COLOR} points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
              <path fill={ACCENT_COLOR} d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
            </svg>
          </div>
        </div>
      </div>

      {/* Typography Label */}
      {showText && (
        <div className="font-mono leading-none">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-foreground text-sm">
            <span className="text-primary tracking-wider uppercase">
              TYLLUAN
            </span>
            <span className="text-[11px] px-1.5 py-0.5 rounded-md bg-primary/10 text-primary border border-primary/30 uppercase tracking-widest font-extrabold">
              o3
            </span>
          </div>
          <div className="text-[11px] text-muted-foreground tracking-widest uppercase font-semibold mt-1 flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-primary" />
            <span>Sustrato Soberano</span>
          </div>
        </div>
      )}
    </div>
  );
}
