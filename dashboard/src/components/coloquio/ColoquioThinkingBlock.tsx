import { BrainCircuit, ChevronRight } from 'lucide-react';
import { useRef, useState } from 'react';
import { parseMarkdown } from '../../lib/coloquio-markdown';
import { cn } from '../../lib/utils';

interface ColoquioThinkingBlockProps {
  thoughtContent: string;
  isStreaming?: boolean;
}

export function ColoquioThinkingBlock({ thoughtContent, isStreaming = false }: ColoquioThinkingBlockProps) {
  const [open, setOpen] = useState(isStreaming);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const lineCount = thoughtContent.split(/\r?\n/).filter(Boolean).length;

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      setOpen(false);
      buttonRef.current?.focus();
    }
  };

  return (
    <div
      onKeyDown={handleKeyDown}
      className="my-2 rounded-md border border-border bg-muted/40 overflow-hidden text-[11px]"
    >
      <button
        ref={buttonRef}
        type="button"
        onClick={() => setOpen(prev => !prev)}
        aria-expanded={open}
        className="w-full flex items-center justify-between gap-2 px-2.5 py-1.5 text-left font-semibold text-foreground hover:bg-muted/70 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span className="flex items-center gap-1.5">
          <ChevronRight
            className={cn('w-3.5 h-3.5 text-muted-foreground transition-transform', open ? 'rotate-90' : '')}
          />
          <BrainCircuit
            className={cn(
              'w-3.5 h-3.5',
              isStreaming ? 'text-cyan-600 dark:text-cyan-400 animate-pulse' : 'text-primary'
            )}
          />
          <span>Razonamiento</span>
          <span className="text-[10.5px] font-mono font-normal text-muted-foreground">
            · {lineCount} {lineCount === 1 ? 'línea' : 'líneas'}
          </span>
        </span>
        {isStreaming && (
          <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 border border-cyan-500/30 text-cyan-700 dark:text-cyan-300">
            en curso
          </span>
        )}
      </button>
      {open && (
        <div
          className="px-3 py-2 border-t border-border bg-background/60 pl-3.5 border-l-2 border-l-primary/40 whitespace-pre-wrap leading-relaxed text-muted-foreground font-mono text-[11px] select-text cursor-text"
          dangerouslySetInnerHTML={{
            __html: parseMarkdown(thoughtContent),
          }}
        />
      )}
    </div>
  );
}
