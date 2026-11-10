import React, { useState } from 'react';
import { FileCode, ChevronDown, ChevronUp, ExternalLink, Loader2 } from 'lucide-react';
import { renderUnifiedDiffHtml, sanitizeDisplayText, sanitizeHref } from '../../lib/coloquio-markdown';

interface PatchAttachmentViewerProps {
  url: string;
  name: string;
  token?: string;
}

const MAX_VISIBLE_PATCH_LINES = 1200;

export const PatchAttachmentViewer: React.FC<PatchAttachmentViewerProps> = ({
  url,
  name,
  token,
}) => {
  const [expanded, setExpanded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [rawPatch, setRawPatch] = useState<string | null>(null);
  const [isTruncated, setIsTruncated] = useState(false);
  const [totalLines, setTotalLines] = useState(0);
  const [err, setErr] = useState<string | null>(null);

  const toggleExpand = async () => {
    const next = !expanded;
    setExpanded(next);
    if (next && rawPatch === null && !loading) {
      setLoading(true);
      setErr(null);
      try {
        const headers: Record<string, string> = {};
        if (token) headers['Authorization'] = `Bearer ${token}`;
        const res = await fetch(url, { headers });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const text = await res.text();
        const lines = text.split(/\r?\n/);
        setTotalLines(lines.length);
        if (lines.length > MAX_VISIBLE_PATCH_LINES) {
          setIsTruncated(true);
          setRawPatch(lines.slice(0, MAX_VISIBLE_PATCH_LINES).join('\n'));
        } else {
          setIsTruncated(false);
          setRawPatch(text);
        }
      } catch (e) {
        setErr(e instanceof Error ? e.message : 'No se pudo cargar el parche');
      } finally {
        setLoading(false);
      }
    }
  };

  const escapeHtml = (value: string): string =>
    sanitizeDisplayText(value)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');

  const escapedPatch = rawPatch ? escapeHtml(rawPatch) : '';
  // El nombre del adjunto lo escribe quien envia el mensaje: se escapa antes de ir a innerHTML.
  const escapedName = escapeHtml(name);

  return (
    <div className="w-full space-y-1.5">
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={toggleExpand}
          className="flex items-center gap-1.5 px-2.5 py-1.5 bg-muted hover:bg-muted/80 border border-border rounded text-xs font-mono text-foreground hover:text-primary transition-colors cursor-pointer"
        >
          <FileCode className="w-3.5 h-3.5 text-primary shrink-0" />
          <span className="truncate max-w-[200px] font-semibold">{name}</span>
          {expanded ? <ChevronUp className="w-3.5 h-3.5 ml-1" /> : <ChevronDown className="w-3.5 h-3.5 ml-1" />}
        </button>
        <a
          href={sanitizeHref(url)}
          target="_blank"
          rel="noopener noreferrer"
          title="Abrir archivo original"
          className="p-1.5 rounded bg-card border border-border text-muted-foreground hover:text-foreground transition-colors"
        >
          <ExternalLink className="w-3 h-3" />
        </a>
      </div>

      {expanded && (
        <div className="w-full">
          {loading ? (
            <div className="flex items-center gap-2 p-3 rounded bg-slate-950 border border-slate-800 text-xs font-mono text-slate-400">
              <Loader2 className="w-3.5 h-3.5 animate-spin text-primary" />
              <span>Cargando parche {name}...</span>
            </div>
          ) : err ? (
            <div className="p-2.5 rounded bg-rose-950/30 border border-rose-500/30 text-xs font-mono text-rose-300">
              Error al previsualizar {name}: {err}
            </div>
          ) : rawPatch !== null ? (
            <div className="space-y-1.5">
              <div
                dangerouslySetInnerHTML={{
                  __html: renderUnifiedDiffHtml(escapedPatch, escapedName),
                }}
              />
              {isTruncated && (
                <div className="p-2 rounded bg-amber-500/10 border border-amber-500/30 text-[11px] font-mono text-amber-300 flex items-center justify-between">
                  <span>
                    ⚠️ Parche extenso truncado a los primeros {MAX_VISIBLE_PATCH_LINES.toLocaleString()} renglones (de {totalLines.toLocaleString()}) para proteger el rendimiento.
                  </span>
                  <a
                    href={sanitizeHref(url)}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="underline hover:text-amber-200 ml-2 shrink-0 font-semibold"
                  >
                    Abrir archivo completo
                  </a>
                </div>
              )}
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
};
