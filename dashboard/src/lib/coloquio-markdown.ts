// eslint-disable-next-line no-control-regex
const ANSI_CSI = /\u001b\[[0-?]*[ -/]*[@-~]/g;
// eslint-disable-next-line no-control-regex
const ANSI_OSC = /\u001b\][^\u0007]*(?:\u0007|\u001b\\)/g;
const INVISIBLE_FORMAT = /[\u00ad\u061c\u180e\u200b-\u200f\u202a-\u202e\u2060-\u2064\u2066-\u2069\ufeff]/g;
const TAG_SEQUENCE = /[\u{e0000}-\u{e007f}]+/gu;

function decodeTagSequence(sequence: string): string {
  return Array.from(sequence)
    .map(ch => String.fromCodePoint((ch.codePointAt(0) ?? 0) - 0xe0000))
    .join('');
}

/**
 * D-80: remove terminal controls and invisible Unicode from rendered text.
 * Unicode tag characters are decoded only to decide whether they are one of
 * the three legitimate subdivision flags (England, Scotland, Wales); any other
 * tag payload is dropped instead of rendering as an invisible instruction.
 */
export function sanitizeDisplayText(text: string): string {
  const cleaned = text
    .replace(ANSI_CSI, '')
    .replace(ANSI_OSC, '')
    .replace(INVISIBLE_FORMAT, '');
  return cleaned.replace(TAG_SEQUENCE, (sequence, offset, whole) => {
    const decoded = decodeTagSequence(sequence).replace(/\u007f/g, '');
    const before = whole.slice(Math.max(0, offset - 2), offset);
    const legitimateFlag = before === '\u{1f3f4}' && ['gbeng', 'gbsct', 'gbwls'].includes(decoded);
    return legitimateFlag ? sequence : '';
  });
}

/**
 * Escapes characters with special meaning in regular expressions.
 */
function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Highlights matches of `search` within an HTML string safely without mutating
 * HTML tags, element attributes, or HTML character entities.
 *
 * Splitting on `(<[^>]+>|&[a-zA-Z0-9#]+;)` isolates text nodes from markup,
 * preventing corrupted attribute quotes or nested tag injection.
 */
export function highlightHtmlContent(html: string, search: string): string {
  const trimmed = search.trim();
  if (!trimmed || !html) return html;

  const escaped = escapeRegex(trimmed);
  const regex = new RegExp(`(${escaped})`, 'gi');

  // Split into tags/entities and raw text content
  const tokens = html.split(/(<[^>]+>|&[a-zA-Z0-9#]+;)/g);
  return tokens
    .map(token => {
      // Preserve HTML tags (<...>) and entities (&...;) without highlighting
      if ((token.startsWith('<') && token.endsWith('>')) || (token.startsWith('&') && token.endsWith(';'))) {
        return token;
      }
      return token.replace(regex, '<mark class="bg-amber-400/30 text-foreground rounded px-0.5">$1</mark>');
    })
    .join('');
}

/**
 * Validates and sanitizes URLs before use in `href` or `src` attributes.
 * Prevents javascript:, data:, and vbscript: scheme injection while allowing
 * relative paths and http/https protocols.
 */
export function sanitizeHref(rawUrl: string | undefined | null): string {
  if (!rawUrl) return '#';
  const trimmed = rawUrl.trim();
  if (!trimmed) return '#';

  // Strip control characters and whitespace when checking schemes
  // eslint-disable-next-line no-control-regex
  const normalized = trimmed.replace(/[\u0000-\u001f\s]/g, '').toLowerCase();
  if (
    normalized.startsWith('javascript:') ||
    normalized.startsWith('data:') ||
    normalized.startsWith('vbscript:')
  ) {
    return '#';
  }

  // Allow safe relative paths
  if (trimmed.startsWith('/') || trimmed.startsWith('./') || trimmed.startsWith('#')) {
    return trimmed;
  }

  // Validate absolute URL protocol
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol === 'http:' || parsed.protocol === 'https:') {
      return trimmed;
    }
    return '#';
  } catch {
    return '#';
  }
}

/**
 * Render a unified diff / .patch snippet with line-by-line syntax highlighting,
 * old/new line numbers, and addition/deletion statistics.
 * Zero external dependencies — keeps Coloquio bundle minimal.
 */
export function renderUnifiedDiffHtml(escapedCode: string, title = 'patch / diff'): string {
  const lines = escapedCode.split(/\r?\n/);
  let additions = 0;
  let deletions = 0;
  let oldLine = 0;
  let newLine = 0;

  const renderedRows: string[] = [];

  for (const line of lines) {
    if (line.startsWith('+++ ') || line.startsWith('--- ')) {
      renderedRows.push(
        `<div class="flex items-baseline px-2.5 py-0.5 bg-slate-900/90 text-slate-300 font-bold border-y border-slate-800/80">` +
        `<span class="w-16 shrink-0 select-none text-[10px] text-slate-500">FILE</span>` +
        `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
      );
      continue;
    }

    if (line.startsWith('diff --git ') || line.startsWith('index ') || line.startsWith('new file ') || line.startsWith('deleted file ')) {
      renderedRows.push(
        `<div class="flex items-baseline px-2.5 py-0.5 bg-slate-900/60 text-indigo-300 font-semibold">` +
        `<span class="w-16 shrink-0 select-none text-[10px] text-slate-500">META</span>` +
        `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
      );
      continue;
    }

    const hunkMatch = line.match(/^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@(.*)$/);
    if (hunkMatch) {
      oldLine = parseInt(hunkMatch[1], 10);
      newLine = parseInt(hunkMatch[2], 10);
      renderedRows.push(
        `<div class="flex items-baseline px-2.5 py-0.5 bg-cyan-950/50 text-cyan-300 border-y border-cyan-500/20 font-semibold">` +
        `<span class="w-16 shrink-0 select-none text-[10px] text-cyan-400/80">HUNK</span>` +
        `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
      );
      continue;
    }

    if (line.startsWith('+')) {
      additions += 1;
      const numStr = newLine > 0 ? String(newLine++) : '+';
      renderedRows.push(
        `<div class="flex items-baseline px-2.5 py-0.2 bg-emerald-950/45 text-emerald-200 border-l-2 border-emerald-500">` +
        `<span class="w-8 shrink-0 select-none text-[10px] text-slate-600 text-right pr-1.5"></span>` +
        `<span class="w-8 shrink-0 select-none text-[10px] text-emerald-400/80 text-right pr-2">${numStr}</span>` +
        `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
      );
      continue;
    }

    if (line.startsWith('-')) {
      deletions += 1;
      const numStr = oldLine > 0 ? String(oldLine++) : '-';
      renderedRows.push(
        `<div class="flex items-baseline px-2.5 py-0.2 bg-rose-950/45 text-rose-200 border-l-2 border-rose-500">` +
        `<span class="w-8 shrink-0 select-none text-[10px] text-rose-400/80 text-right pr-1.5">${numStr}</span>` +
        `<span class="w-8 shrink-0 select-none text-[10px] text-slate-600 text-right pr-2"></span>` +
        `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
      );
      continue;
    }

    const oldStr = oldLine > 0 ? String(oldLine++) : '';
    const newStr = newLine > 0 ? String(newLine++) : '';
    renderedRows.push(
      `<div class="flex items-baseline px-2.5 py-0.2 text-slate-300 hover:bg-slate-900/50">` +
      `<span class="w-8 shrink-0 select-none text-[10px] text-slate-600 text-right pr-1.5">${oldStr}</span>` +
      `<span class="w-8 shrink-0 select-none text-[10px] text-slate-600 text-right pr-2">${newStr}</span>` +
      `<span class="whitespace-pre overflow-x-auto">${line}</span></div>`
    );
  }

  return (
    `<div class="my-2 rounded-lg border border-slate-700/70 bg-slate-950/95 overflow-hidden font-mono text-[11px] shadow-sm" data-patch-viewer="true">` +
    `<div class="flex items-center justify-between px-3 py-1.5 bg-slate-900 border-b border-slate-800 text-[11px]">` +
    `<span class="font-bold text-slate-200">${title}</span>` +
    `<div class="flex items-center gap-2">` +
    `<span class="px-1.5 py-0.2 rounded bg-emerald-500/15 border border-emerald-500/30 text-emerald-300 font-semibold">+${additions}</span>` +
    `<span class="px-1.5 py-0.2 rounded bg-rose-500/15 border border-rose-500/30 text-rose-300 font-semibold">-${deletions}</span>` +
    `</div></div>` +
    `<div class="max-h-96 overflow-x-auto overflow-y-auto divide-y divide-slate-900/40">${renderedRows.join('')}</div>` +
    `</div>`
  );
}

function isUnifiedDiffBlock(lang: string, code: string): boolean {
  const normalizedLang = lang.toLowerCase();
  if (normalizedLang === 'diff' || normalizedLang === 'patch' || normalizedLang.endsWith('.patch')) {
    return true;
  }
  return (
    code.includes('diff --git ') ||
    (code.includes('--- ') && code.includes('+++ ') && code.includes('@@ '))
  );
}

/**
 * Markdown renderer for Coloquio messages.
 * Handles code blocks, unified diff/.patch syntax highlighting, inline code,
 * bold, italics, quotes, lists, turn jumps, and mentions.
 *
 * Word-boundary matching for underscore emphasis prevents false italics on
 * snake_case identifiers, file paths, and URLs (e.g. scripts/check_test_count.sh).
 */
export function parseMarkdown(text: string): string {
  let h = sanitizeDisplayText(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  const codeBlocks: string[] = [];
  h = h.replace(/```([^\r\n]*)\r?\n([\s\S]*?)\r?\n```/g, (_, lang, code) => {
    const idx = codeBlocks.length;
    const cleanLang = (lang || '').trim() || 'code';
    if (isUnifiedDiffBlock(cleanLang, code)) {
      codeBlocks.push(renderUnifiedDiffHtml(code, cleanLang === 'code' ? 'patch / diff' : cleanLang));
    } else {
      codeBlocks.push(
        `<pre class="bg-slate-950/90 border border-slate-700/50 rounded-lg p-3 my-2 font-mono text-[11px] overflow-x-auto text-slate-200">` +
        `<div class="text-[11px] text-slate-400 font-bold mb-1 border-b border-slate-800 pb-1">${cleanLang}</div>` +
        `<code>${code}</code></pre>`
      );
    }
    return `%%CODE_BLOCK_${idx}%%`;
  });

  const inlineCodes: string[] = [];
  h = h.replace(/`([^`]+)`/g, (_, code) => {
    const idx = inlineCodes.length;
    inlineCodes.push(
      `<code class="bg-slate-200/80 dark:bg-slate-950 border border-slate-300/80 dark:border-slate-700/50 px-1 py-0.5 rounded font-mono text-[11px] text-indigo-700 dark:text-indigo-300">${code}</code>`
    );
    return `%%INLINE_CODE_${idx}%%`;
  });

  h = h.replace(/\*\*([^*]+)\*\*/g, '<strong class="text-slate-100">$1</strong>');
  h = h.replace(/(^|[\s,;().!?])\*([^*\s](?:[^*\n]*?[^*\s])?)\*(?=[,\s;().!?]|$)/g, '$1<em>$2</em>');
  h = h.replace(/(^|[\s,;().!?])_([^_ \n](?:[^_\n]*?[^_ \n])?)_(?=[,\s;().!?]|$)/g, '$1<em>$2</em>');

  h = h.replace(/(?:^|\n)&gt;\s?(.*)/g, (_, q) =>
    `<blockquote class="border-l-2 border-indigo-500/40 pl-3 italic text-slate-600 dark:text-slate-400 my-1.5">${q}</blockquote>`
  );
  h = h.replace(/(?:^|\n)[-*]\s(.*)/g, (_, item) =>
    `<li class="list-disc ml-4 my-0.5 text-slate-700 dark:text-slate-300">${item}</li>`
  );
  // Wrap contiguous <li> sequences in <ul> to satisfy WCAG listitem rule
  h = h.replace(/((?:<li[^>]*>.*?<\/li>\s*)+)/g, '<ul class="my-1 list-none space-y-0.5">$1</ul>');
  h = h.replace(/(^|[\s,;().!?])#(\d{1,4})\b/g,
    '$1<span class="cursor-pointer text-amber-700 dark:text-amber-400 hover:text-amber-600 dark:hover:text-amber-300 underline font-mono font-semibold" data-jump-turn="$2">#$2</span>'
  );
  h = h.replace(/(^|[\s,;()>.!?])@([A-Za-z0-9_][A-Za-z0-9_-]*)/g,
    '$1<span class="text-amber-700 dark:text-amber-300 bg-amber-950 dark:bg-amber-950/50 border border-amber-400/50 dark:border-amber-800/40 rounded px-1 font-semibold">@$2</span>'
  );

  h = h.replace(/%%INLINE_CODE_(\d+)%%/g, (_, idx) => inlineCodes[parseInt(idx, 10)]);
  h = h.replace(/%%CODE_BLOCK_(\d+)%%/g, (_, idx) => codeBlocks[parseInt(idx, 10)]);

  return h;
}
