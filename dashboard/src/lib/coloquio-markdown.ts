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
 * Markdown renderer for Coloquio messages.
 * Handles code blocks, inline code, bold, italics, quotes, lists, turn jumps, and mentions.
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
    codeBlocks.push(
      `<pre class="bg-slate-950/90 border border-slate-700/50 rounded-lg p-3 my-2 font-mono text-[11px] overflow-x-auto text-slate-200">` +
      `<div class="text-[11px] text-slate-400 font-bold mb-1 border-b border-slate-800 pb-1">${cleanLang}</div>` +
      `<code>${code}</code></pre>`
    );
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
