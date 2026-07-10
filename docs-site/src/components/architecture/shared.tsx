// Shared constants and utilities for architecture diagrams

export const COLORS = {
  // Backgrounds
  bg: '#0A0F1A',
  surface: '#111827',
  surfaceLight: '#1F2937',
  surfaceHover: '#374151',

  // Text
  text: '#F1F5F9',
  textMuted: '#94A3B8',
  textDim: '#64748B',

  // Accents
  teal: '#14B8A6',
  tealDark: '#0D9488',
  tealLight: '#2DD4BF',
  tealGlow: 'rgba(20, 184, 166, 0.15)',
  tealBorder: 'rgba(20, 184, 166, 0.4)',

  // Semantic
  success: '#10B981',
  warning: '#F59E0B',
  error: '#EF4444',
  info: '#3B82F6',

  // Neutrals
  border: '#1E293B',
  borderLight: '#334155',
  nodeFill: '#0F172A',
  nodeStroke: '#1E293B',
} as const;

export const NODE_STYLES = {
  core: { fill: '#0F172A', stroke: '#14B8A6', strokeWidth: 1.5 },
  subsystem: { fill: '#1E293B', stroke: '#334155', strokeWidth: 1 },
  external: { fill: '#1A1A2E', stroke: '#475569', strokeWidth: 1, strokeDasharray: '4 2' },
  data: { fill: '#0C1A1A', stroke: '#0D9488', strokeWidth: 1 },
  process: { fill: '#1A1520', stroke: '#A855F7', strokeWidth: 1 },
} as const;

export const ARROW_COLORS = {
  data: '#14B8A6',
  control: '#64748B',
  sync: '#F59E0B',
  error: '#EF4444',
} as const;

// SVG arrow marker definition
export function ArrowMarker({ id, color = ARROW_COLORS.data }: { id: string; color?: string }) {
  return (
    <defs>
      <marker
        id={id}
        viewBox="0 0 10 7"
        refX="10"
        refY="3.5"
        markerWidth="8"
        markerHeight="6"
        orient="auto-start-reverse"
      >
        <polygon points="0 0, 10 3.5, 0 7" fill={color} />
      </marker>
    </defs>
  );
}

// Glow filter for highlighted nodes
export function GlowFilter({ id, color = '#14B8A6' }: { id: string; color?: string }) {
  return (
    <defs>
      <filter id={id} x="-50%" y="-50%" width="200%" height="200%">
        <feGaussianBlur stdDeviation="4" result="blur" />
        <feFlood floodColor={color} floodOpacity="0.3" result="color" />
        <feComposite in="color" in2="blur" operator="in" result="glow" />
        <feMerge>
          <feMergeNode in="glow" />
          <feMergeNode in="SourceGraphic" />
        </feMerge>
      </filter>
    </defs>
  );
}

// Reusable SVG node component
interface NodeStyle {
  fill: string;
  stroke: string;
  strokeWidth: number;
  strokeDasharray?: string;
}

interface NodeProps {
  x: number;
  y: number;
  width: number;
  height: number;
  label: string;
  sublabel?: string;
  style?: NodeStyle;
  highlight?: boolean;
  onClick?: () => void;
}

export function SvgNode({ x, y, width, height, label, sublabel, style = NODE_STYLES.core, highlight, onClick }: NodeProps) {
  return (
    <g onClick={onClick} className={onClick ? 'cursor-pointer' : ''}>
      {highlight && (
        <rect
          x={x - 3}
          y={y - 3}
          width={width + 6}
          height={height + 6}
          rx={10}
          fill="none"
          stroke={style.stroke}
          strokeWidth={1}
          opacity={0.3}
        />
      )}
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        rx={6}
        fill={style.fill}
        stroke={style.stroke}
        strokeWidth={style.strokeWidth}
        strokeDasharray={style.strokeDasharray}
      />
      <text
        x={x + width / 2}
        y={y + height / 2 - (sublabel ? 7 : 0)}
        textAnchor="middle"
        dominantBaseline="central"
        fill="#F1F5F9"
        fontSize="13"
        fontFamily="system-ui, -apple-system, sans-serif"
        fontWeight={500}
      >
        {label}
      </text>
      {sublabel && (
        <text
          x={x + width / 2}
          y={y + height / 2 + 11}
          textAnchor="middle"
          dominantBaseline="central"
          fill="#94A3B8"
          fontSize="10"
          fontFamily="ui-monospace, monospace"
        >
          {sublabel}
        </text>
      )}
    </g>
  );
}

// Connection line with arrow
interface ConnectionProps {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color?: string;
  dashed?: boolean;
  label?: string;
  labelPosition?: 'start' | 'middle' | 'end';
  markerId?: string;
  path?: string; // Custom SVG path
  strokeWidth?: number;
}

export function Connection({
  x1, y1, x2, y2,
  color = ARROW_COLORS.data,
  dashed = false,
  label,
  labelPosition = 'middle',
  markerId = 'arrow-data',
  path,
  strokeWidth = 1.2,
}: ConnectionProps) {
  const midX = (x1 + x2) / 2;
  const midY = (y1 + y2) / 2;

  let labelX = midX;
  let labelY = midY;

  if (labelPosition === 'start') {
    labelX = x1 + (x2 - x1) * 0.25;
    labelY = y1 + (y2 - y1) * 0.25;
  } else if (labelPosition === 'end') {
    labelX = x1 + (x2 - x1) * 0.75;
    labelY = y1 + (y2 - y1) * 0.75;
  }

  return (
    <g>
      {path ? (
        <path
          d={path}
          fill="none"
          stroke={color}
          strokeWidth={strokeWidth}
          strokeDasharray={dashed ? '5 3' : undefined}
          markerEnd={`url(#${markerId})`}
          opacity={0.7}
        />
      ) : (
        <line
          x1={x1}
          y1={y1}
          x2={x2}
          y2={y2}
          stroke={color}
          strokeWidth={strokeWidth}
          strokeDasharray={dashed ? '5 3' : undefined}
          markerEnd={`url(#${markerId})`}
          opacity={0.7}
        />
      )}
      {label && (
        <text
          x={labelX}
          y={labelY - 8}
          textAnchor="middle"
          fill={color}
          fontSize="9"
          fontFamily="ui-monospace, monospace"
          opacity={0.9}
        >
          {label}
        </text>
      )}
    </g>
  );
}

// Section header for diagrams
interface SectionLabelProps {
  x: number;
  y: number;
  text: string;
  color?: string;
}

export function SectionLabel({ x, y, text, color = '#14B8A6' }: SectionLabelProps) {
  return (
    <g>
      <line x1={x} y1={y} x2={x + 4} y2={y} stroke={color} strokeWidth={2} />
      <text
        x={x + 10}
        y={y + 1}
        fill={color}
        fontSize="11"
        fontFamily="ui-monospace, monospace"
        fontWeight={600}
        style={{ textTransform: 'uppercase' }}
        letterSpacing="0.05em"
      >
        {text}
      </text>
    </g>
  );
}

// Phase box for flowcharts
interface PhaseBoxProps {
  x: number;
  y: number;
  width: number;
  height: number;
  title: string;
  color: string;
  children: React.ReactNode;
}

export function PhaseBox({ x, y, width, height, title, color, children }: PhaseBoxProps) {
  return (
    <g>
      <rect
        x={x}
        y={y}
        width={width}
        height={height}
        rx={8}
        fill={`${color}08`}
        stroke={`${color}30`}
        strokeWidth={1}
      />
      <rect
        x={x}
        y={y}
        width={width}
        height={28}
        rx={8}
        fill={`${color}15`}
      />
      <rect
        x={x}
        y={y + 20}
        width={width}
        height={8}
        fill={`${color}15`}
      />
      <text
        x={x + 12}
        y={y + 16}
        fill={color}
        fontSize="11"
        fontFamily="ui-monospace, monospace"
        fontWeight={600}
      >
        {title}
      </text>
      {children}
    </g>
  );
}

// Mini badge inside SVG
export function SvgBadge({ x, y, text, color = '#14B8A6' }: { x: number; y: number; text: string; color?: string }) {
  return (
    <g>
      <rect x={x} y={y} width={text.length * 7 + 12} height={18} rx={9} fill={`${color}20`} stroke={`${color}40`} strokeWidth={0.5} />
      <text x={x + (text.length * 7 + 12) / 2} y={y + 12} textAnchor="middle" fill={color} fontSize="9" fontFamily="ui-monospace, monospace">
        {text}
      </text>
    </g>
  );
}