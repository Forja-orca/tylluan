import React, { useState } from 'react';
import { cn } from '../lib/utils';

interface TylluanLogoProps {
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl';
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
  const [imgLoaded, setImgLoaded] = useState(true);

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
    '2xl': 'p-3',
  };

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* CSS Keyframe Animations for Cybernetic Owl HUD */}
      <style>{`
        @keyframes owl-breathe {
          0%, 100% {
            transform: scale(1);
            filter: drop-shadow(0 0 10px rgba(0, 245, 212, 0.4)) drop-shadow(0 0 20px rgba(56, 189, 248, 0.2));
          }
          50% {
            transform: scale(1.04);
            filter: drop-shadow(0 0 22px rgba(0, 245, 212, 0.85)) drop-shadow(0 0 35px rgba(56, 189, 248, 0.5));
          }
        }

        @keyframes owl-eye-blink {
          0%, 90%, 100% {
            opacity: 0.95;
            transform: scale(1);
          }
          95% {
            opacity: 0.1;
            transform: scale(0.2, 0.05);
          }
        }

        @keyframes owl-scanline {
          0% {
            top: -20%;
            opacity: 0;
          }
          15% {
            opacity: 0.8;
          }
          85% {
            opacity: 0.8;
          }
          100% {
            top: 120%;
            opacity: 0;
          }
        }

        @keyframes hud-rotate-cw {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }

        @keyframes hud-rotate-ccw {
          from { transform: rotate(0deg); }
          to { transform: rotate(-360deg); }
        }

        .animate-owl-breathe {
          animation: owl-breathe 3.2s ease-in-out infinite;
        }

        .animate-owl-eye {
          animation: owl-eye-blink 4.5s ease-in-out infinite;
        }

        .animate-owl-scanline {
          animation: owl-scanline 2.8s cubic-bezier(0.4, 0, 0.6, 1) infinite;
        }

        .animate-hud-cw {
          animation: hud-rotate-cw 12s linear infinite;
        }

        .animate-hud-ccw {
          animation: hud-rotate-ccw 16s linear infinite;
        }
      `}</style>

      {/* Cybernetic Owl Avatar Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Outer Rotating HUD Ring 1 */}
        {animated && (
          <div className="absolute -inset-1.5 rounded-2xl border border-dashed border-[#00F5D4]/30 animate-hud-cw pointer-events-none" />
        )}

        {/* Outer Counter-Rotating HUD Ring 2 */}
        {animated && (
          <div className="absolute -inset-3 rounded-full border border-dotted border-sky-400/20 animate-hud-ccw pointer-events-none" />
        )}

        {/* Radial Energy Halo */}
        <div
          className={cn(
            'absolute inset-0 rounded-2xl bg-gradient-to-tr from-[#00F5D4]/20 via-sky-400/20 to-purple-500/10 transition-all duration-500',
            animated && 'animate-owl-breathe'
          )}
        />

        {/* Inner Card Container */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-[#00F5D4]/50 overflow-hidden flex items-center justify-center shadow-xl backdrop-blur-md',
            ringPadding[size]
          )}
        >
          {/* Laser Scanline Beam */}
          {animated && (
            <div className="absolute inset-x-0 h-1 bg-gradient-to-r from-transparent via-[#00F5D4] to-transparent shadow-[0_0_8px_#00F5D4] animate-owl-scanline z-20 pointer-events-none" />
          )}

          {/* Official Owl Logo Image or Vector Fallback */}
          {imgLoaded ? (
            <img
              src="/tylluan-logo.jpg"
              alt="Tylluan Cybernetic Owl Logo"
              className="w-full h-full object-cover rounded-xl transition-transform duration-700 group-hover:scale-110"
              onError={() => setImgLoaded(false)}
            />
          ) : (
            <svg
              viewBox="0 0 30 30"
              className="w-full h-full text-slate-100 transition-transform duration-700 group-hover:scale-110"
            >
              <path
                fill="#0B0F17"
                stroke="#00F5D4"
                strokeWidth="0.8"
                d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
              />
              <g className="animate-owl-breathe">
                <path fill="#00F5D4" d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill="#38BDF8" points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill="#00F5D4" d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </g>
            </svg>
          )}

          {/* Glowing Animated Cybernetic Owl Eyes Layer */}
          {animated && (
            <div className="absolute inset-0 pointer-events-none z-10 flex items-center justify-center">
              {/* Left Eye HUD Node */}
              <div className="absolute top-[38%] left-[34%] w-[12%] h-[12%] rounded-full bg-[#00F5D4] shadow-[0_0_10px_#00F5D4] animate-owl-eye" />
              {/* Right Eye HUD Node */}
              <div className="absolute top-[38%] right-[34%] w-[12%] h-[12%] rounded-full bg-[#00F5D4] shadow-[0_0_10px_#00F5D4] animate-owl-eye" />
            </div>
          )}

          {/* Vignette Overlay */}
          <div className="absolute inset-0 bg-gradient-to-b from-transparent via-slate-950/10 to-slate-950/40 pointer-events-none" />
        </div>
      </div>

      {/* Typography Label */}
      {showText && (
        <div className="font-mono leading-none">
          <div className="flex items-center gap-1.5 font-bold tracking-tight text-slate-100 text-sm">
            <span className="text-[#00F5D4] tracking-wider uppercase drop-shadow-[0_0_8px_rgba(0,245,212,0.4)]">
              TYLLUAN
            </span>
            <span className="text-[9px] px-1.5 py-0.5 rounded-md bg-[#00F5D4]/10 text-[#00F5D4] border border-[#00F5D4]/30 uppercase tracking-widest font-extrabold">
              o3
            </span>
          </div>
          <div className="text-[10px] text-slate-400 tracking-widest uppercase font-semibold mt-1 flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-[#00F5D4] animate-ping" />
            <span>Sovereign Substrate</span>
          </div>
        </div>
      )}
    </div>
  );
}
