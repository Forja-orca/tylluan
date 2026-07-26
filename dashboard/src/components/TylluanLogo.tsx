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

  // Sovereign Color Tokens (Immutable)
  const SOVEREIGN_BG = '#0B0F17';
  const CYAN_NEON = '#00F5D4';
  const MAGENTA_FLASH = '#FF2E93';

  // 12 Nodes Coordinates (Home vs Constellation Offsets)
  const nodesData = [
    // Left Wing (Nodes 0-3)
    { id: 0, type: 'wing_left', hx: 18, hy: 30, cx: 4, cy: 15, delay: 0 },
    { id: 1, type: 'wing_left', hx: 28, hy: 45, cx: 12, cy: 48, delay: 120 },
    { id: 2, type: 'wing_left', hx: 32, hy: 65, cx: 16, cy: 78, delay: 240 },
    { id: 3, type: 'wing_left', hx: 40, hy: 32, cx: 28, cy: 14, delay: 360 },

    // Right Wing (Nodes 4-7)
    { id: 4, type: 'wing_right', hx: 82, hy: 30, cx: 96, cy: 15, delay: 480 },
    { id: 5, type: 'wing_right', hx: 72, hy: 45, cx: 88, cy: 48, delay: 600 },
    { id: 6, type: 'wing_right', hx: 68, hy: 65, cx: 84, cy: 78, delay: 720 },
    { id: 7, type: 'wing_right', hx: 60, hy: 32, cx: 72, cy: 14, delay: 840 },

    // Eyes (Nodes 8-9, Magenta Memory Index)
    { id: 8, type: 'eye', hx: 42, hy: 42, cx: 38, cy: 36, delay: 960, isEye: true },
    { id: 9, type: 'eye', hx: 58, hy: 42, cx: 62, cy: 36, delay: 1080, isEye: true },

    // Chest (Nodes 10-11)
    { id: 10, type: 'chest', hx: 50, hy: 55, cx: 50, cy: 68, delay: 1200 },
    { id: 11, type: 'chest', hx: 50, hy: 74, cx: 50, cy: 88, delay: 1320 },
  ];

  // Edges connecting constellation nodes
  const constellationEdges = [
    [0, 3], [3, 8], [8, 10], [10, 9], [9, 7], [7, 4],
    [0, 1], [1, 2], [2, 11], [11, 6], [6, 5], [5, 4],
    [8, 9], [3, 7], [10, 11]
  ];

  return (
    <div className={cn('flex items-center gap-3 select-none', className)}>
      {/* Dynamic 60FPS Organic Keyframe Engine */}
      <style>{`
        /* Phase 1: Base State Ambient Aura Breathe */
        @keyframes sovereign-aura-breathe {
          0%, 100% {
            box-shadow: 0 0 12px rgba(0, 245, 212, 0.4), inset 0 0 8px rgba(0, 245, 212, 0.2);
          }
          50% {
            box-shadow: 0 0 24px rgba(0, 245, 212, 0.75), inset 0 0 16px rgba(0, 245, 212, 0.4);
          }
        }

        /* Phase 2 to 4: Owl Body Progressive Fade (1.0 -> 0.15 -> 1.0) */
        @keyframes owl-body-progressive {
          0%, 15.8% {
            opacity: 1;
            filter: drop-shadow(0 0 8px rgba(0, 245, 212, 0.5));
          }
          47.4%, 68.4% {
            opacity: 0.15;
            filter: drop-shadow(0 0 2px rgba(0, 245, 212, 0.2));
          }
          94.7% {
            opacity: 1;
            filter: drop-shadow(0 0 12px rgba(0, 245, 212, 0.8));
          }
          97.8% {
            /* Micro-flash seal at 200ms end of loop */
            opacity: 1;
            transform: scale(1.05);
            filter: drop-shadow(0 0 25px rgba(255, 46, 147, 0.95));
          }
          100% {
            opacity: 1;
            transform: scale(1);
            filter: drop-shadow(0 0 8px rgba(0, 245, 212, 0.5));
          }
        }

        /* Laser Connection Lines Opacity Pulse */
        @keyframes constellation-lines-fade {
          0%, 15.8% {
            opacity: 0.05;
          }
          47.4%, 68.4% {
            opacity: 0.45;
            stroke-dashoffset: 0;
          }
          94.7%, 100% {
            opacity: 0.05;
          }
        }

        /* Rotating Outer HUD Rings */
        @keyframes hud-spin-cw {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
        @keyframes hud-spin-ccw {
          from { transform: rotate(0deg); }
          to { transform: rotate(-360deg); }
        }

        .animate-sovereign-aura {
          animation: sovereign-aura-breathe 3.5s ease-in-out infinite;
        }

        .animate-owl-body-loop {
          animation: owl-body-progressive 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
          transform-origin: center center;
        }

        .animate-constellation-lines {
          animation: constellation-lines-fade 9.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
        }

        .animate-hud-ring-cw {
          animation: hud-spin-cw 16s linear infinite;
        }

        .animate-hud-ring-ccw {
          animation: hud-spin-ccw 22s linear infinite;
        }
      `}</style>

      {/* Cybernetic Owl Avatar Canvas Container */}
      <div className={cn('relative flex items-center justify-center group', containerSizes[size])}>
        {/* Outer Rotating HUD Ring 1 (Cian Neón) */}
        {animated && (
          <div className="absolute -inset-1.5 rounded-2xl border border-dashed border-[#00F5D4]/35 animate-hud-ring-cw pointer-events-none" />
        )}

        {/* Outer Counter-Rotating HUD Ring 2 (Magenta Flash) */}
        {animated && (
          <div className="absolute -inset-3 rounded-full border border-dotted border-[#FF2E93]/25 animate-hud-ring-ccw pointer-events-none" />
        )}

        {/* Sovereign Card Base */}
        <div
          className={cn(
            'relative w-full h-full rounded-2xl bg-[#0B0F17] border border-[#00F5D4]/40 overflow-hidden flex items-center justify-center backdrop-blur-md transition-all duration-300',
            animated && 'animate-sovereign-aura',
            ringPadding[size]
          )}
        >
          {/* Base Layer: Official Owl Image or SVG Base (Fades 1.0 -> 0.15 -> 1.0) */}
          <div className={cn('w-full h-full flex items-center justify-center relative z-10', animated && 'animate-owl-body-loop')}>
            {imgLoaded ? (
              <img
                src="/tylluan-logo.jpg"
                alt="Tylluan Owl Official Logo"
                className="w-full h-full object-cover rounded-xl"
                onError={() => setImgLoaded(false)}
              />
            ) : (
              <svg viewBox="0 0 30 30" className="w-full h-full text-slate-100">
                <path
                  fill="#0B0F17"
                  stroke={CYAN_NEON}
                  strokeWidth="0.8"
                  d="M24.51,28.51H5.49c-2.21,0-4-1.79-4-4V5.49c0-2.21,1.79-4,4-4h19.03c2.21,0,4,1.79,4,4v19.03C28.51,26.72,26.72,28.51,24.51,28.51z"
                />
                <path fill={CYAN_NEON} d="M15.47,7.1l-1.3,1.85c-0.2,0.29-0.54,0.47-0.9,0.47h-7.1V7.09C6.16,7.1,15.47,7.1,15.47,7.1z" />
                <polygon fill="#38BDF8" points="24.3,7.1 13.14,22.91 5.7,22.91 16.86,7.1" />
                <path fill={CYAN_NEON} d="M14.53,22.91l1.31-1.86c0.2-0.29,0.54-0.47,0.9-0.47h7.09v2.33H14.53z" />
              </svg>
            )}
          </div>

          {/* SVG Overlay Layer: 12 Granular Nodes + Bezier Flight Trajectories + Constellation Lines */}
          <svg viewBox="0 0 100 100" className="absolute inset-0 w-full h-full z-20 pointer-events-none overflow-visible">
            {/* Constellation Connecting Lines (Soft White rgba(255,255,255,0.3)) */}
            <g className={cn(animated && 'animate-constellation-lines')}>
              {constellationEdges.map(([fromId, toId], idx) => {
                const nFrom = nodesData[fromId];
                const nTo = nodesData[toId];
                return (
                  <line
                    key={idx}
                    x1={nFrom.cx}
                    y1={nFrom.cy}
                    x2={nTo.cx}
                    y2={nTo.cy}
                    stroke="rgba(255,255,255,0.3)"
                    strokeWidth="1.2"
                    strokeDasharray="3 2"
                  />
                );
              })}
            </g>

            {/* 12 Individual Nodes with Custom Staggered Bezier CSS Trajectories */}
            {nodesData.map((node) => {
              // Custom CSS animation keyframe for this specific node
              const nodeAnimName = `node-bezier-flight-${node.id}`;
              const totalLoopSecs = 9.5;

              // Calculate timing percentages for Phase 2 (Fly Out) and Phase 4 (Fly In) with 120ms Stagger
              const flyOutStartMs = 1500 + node.delay;
              const flyOutEndMs = flyOutStartMs + 1200; // 1.2s flight duration

              const reverseDelay = 1320 - node.delay; // Reverse order for return
              const flyInStartMs = 6500 + reverseDelay;
              const flyInEndMs = flyInStartMs + 1200; // 1.2s return flight duration

              const pFlyOutStart = ((flyOutStartMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyOutEnd = ((flyOutEndMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyInStart = ((flyInStartMs / (totalLoopSecs * 1000)) * 100).toFixed(1);
              const pFlyInEnd = ((flyInEndMs / (totalLoopSecs * 1000)) * 100).toFixed(1);

              return (
                <g key={node.id}>
                  {/* Inline keyframe for node flight */}
                  <style>{`
                    @keyframes ${nodeAnimName} {
                      0%, ${pFlyOutStart}% {
                        transform: translate(0px, 0px);
                        opacity: 0.2;
                      }
                      ${pFlyOutEnd}%, ${pFlyInStart}% {
                        /* Constellation Position (Phase 2 & 3) */
                        transform: translate(${node.cx - node.hx}px, ${node.cy - node.hy}px);
                        opacity: 1;
                      }
                      ${pFlyInEnd}%, 100% {
                        /* Overshoot Elastic Return to Home (Phase 4) */
                        transform: translate(0px, 0px);
                        opacity: 0.2;
                      }
                    }

                    .node-anim-${node.id} {
                      animation: ${nodeAnimName} ${totalLoopSecs}s cubic-bezier(0.34, 1.56, 0.64, 1) infinite;
                      transform-origin: ${node.hx}px ${node.hy}px;
                    }
                  `}</style>

                  {/* SVG Node Circle */}
                  <g className={cn(animated && `node-anim-${node.id}`)}>
                    {/* Node Core */}
                    <circle
                      cx={node.hx}
                      cy={node.hy}
                      r={node.isEye ? 3.5 : 2.5}
                      fill={node.isEye ? MAGENTA_FLASH : CYAN_NEON}
                    />

                    {/* Node Pulse Outer Glow Ring */}
                    <circle
                      cx={node.hx}
                      cy={node.hy}
                      r={node.isEye ? 6 : 4.5}
                      fill="none"
                      stroke={node.isEye ? MAGENTA_FLASH : CYAN_NEON}
                      strokeWidth="0.8"
                      opacity="0.6"
                    />
                  </g>
                </g>
              );
            })}
          </svg>

          {/* Vignette Overlay */}
          <div className="absolute inset-0 bg-gradient-to-b from-transparent via-transparent to-slate-950/20 pointer-events-none z-30" />
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
