import { describe, it, expect } from 'vitest';
import { parseMarkdown, highlightHtmlContent, sanitizeHref } from '../../lib/coloquio-markdown';

describe('DASH-INNERHTML-AUDIT: Coloquio Markdown & Link Security', () => {
  describe('highlightHtmlContent — Mutation & Attribute Protection', () => {
    it('does not corrupt HTML tag attributes when search query matches attribute names or CSS classes', () => {
      const rawMarkdown = '**texto en negrita** y código `console.log("hola")`';
      const parsedHtml = parseMarkdown(rawMarkdown);

      // El HTML generado contiene clases como 'text-slate-100', 'bg-slate-200/80', etc.
      expect(parsedHtml).toContain('class="text-slate-100"');

      // Prueba con términos que coinciden con tags o clases internas
      const dangerousTerms = ['slate', 'text', 'class', 'strong', 'code', 'bg', 'span'];

      for (const term of dangerousTerms) {
        const highlighted = highlightHtmlContent(parsedHtml, term);

        // NUNCA debe inyectar <mark dentro de una etiqueta HTML
        expect(highlighted).not.toMatch(/<[^>]*<mark[^>]*>[^>]*>/);
        expect(highlighted).not.toContain('class="text-<mark');
        expect(highlighted).not.toContain('class="<mark');

        // Los tags de apertura y cierre deben permanecer perfectamente balanceados
        const openTags = (highlighted.match(/<strong\b/g) || []).length;
        const closeTags = (highlighted.match(/<\/strong>/g) || []).length;
        expect(openTags).toBe(closeTags);
      }
    });

    it('mutation test: demonstrates failure with legacy un-sanitized replace vs success with highlightHtmlContent', () => {
      const parsedHtml = '<strong class="text-slate-100">slate analysis</strong>';
      const search = 'slate';

      // 1. Simular la mutación defectuosa previa (replace directo sobre HTML completo)
      const escaped = search.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const flawedOutput = parsedHtml.replace(
        new RegExp(`(${escaped})`, 'gi'),
        '<mark class="bg-amber-400/30 text-foreground rounded px-0.5">$1</mark>'
      );

      // La lógica antigua corrompía el atributo de la clase:
      expect(flawedOutput).toContain('class="text-<mark class="bg-amber-400/30 text-foreground rounded px-0.5">slate</mark>-100"');

      // 2. La nueva función segura separa tags y texto plano:
      const safeOutput = highlightHtmlContent(parsedHtml, search);

      // La etiqueta strong y su clase se mantienen intactas
      expect(safeOutput).toContain('<strong class="text-slate-100">');
      // Y sólo el contenido visible dentro del nodo de texto se resalta
      expect(safeOutput).toContain('<mark class="bg-amber-400/30 text-foreground rounded px-0.5">slate</mark> analysis</strong>');
      expect(safeOutput).not.toContain('text-<mark');
    });

    it('correctly highlights matches in plain text without modifying surrounding tags', () => {
      const html = '<p>Este es un informe sobre el kernel y la federacion</p>';
      const result = highlightHtmlContent(html, 'kernel');

      expect(result).toBe(
        '<p>Este es un informe sobre el <mark class="bg-amber-400/30 text-foreground rounded px-0.5">kernel</mark> y la federacion</p>'
      );
    });

    it('does not corrupt HTML entities when search matches entity substrings', () => {
      const raw = 'x < 10 && y > 20';
      const parsed = parseMarkdown(raw);
      expect(parsed).toContain('&lt;');
      expect(parsed).toContain('&gt;');

      // Buscar 'lt' o 'gt' o 'amp' no debe romper la entidad
      const highlighted = highlightHtmlContent(parsed, 'lt');
      expect(highlighted).toContain('&lt;');
      expect(highlighted).not.toContain('&<mark');
    });

    it('handles special regex characters in search string safely', () => {
      const html = '<div>Resultado: [A-Z]+ (score: 99.5%) *test*</div>';
      const specialSearches = ['[A-Z]+', '(score:', '99.5%', '*test*'];

      for (const query of specialSearches) {
        expect(() => highlightHtmlContent(html, query)).not.toThrow();
        const highlighted = highlightHtmlContent(html, query);
        expect(highlighted).toContain('<mark class="bg-amber-400/30 text-foreground rounded px-0.5">');
      }
    });

    it('returns original HTML when search is empty or whitespace', () => {
      const html = '<div>No search active</div>';
      expect(highlightHtmlContent(html, '')).toBe(html);
      expect(highlightHtmlContent(html, '   ')).toBe(html);
      expect(highlightHtmlContent('', 'algo')).toBe('');
    });
  });

  describe('sanitizeHref — URL Scheme Validation & Link Hijacking Protection', () => {
    it('blocks dangerous javascript: schemes and variants', () => {
      const dangerous = [
        'javascript:alert(1)',
        'JAVASCRIPT:alert(document.cookie)',
        'javascript :alert("xss")',
        'javascript\n:alert(1)',
        'javascript\t:alert(1)',
        '\u0000javascript:alert(1)',
        'vbscript:msgbox(1)',
        'data:text/html,<script>alert(1)</script>',
        'data:application/javascript;base64,YWxlcnQoMSk=',
      ];

      for (const url of dangerous) {
        expect(sanitizeHref(url)).toBe('#');
      }
    });

    it('permits safe relative URLs and hash anchors', () => {
      expect(sanitizeHref('/api/v1/ingest/files/test.patch')).toBe('/api/v1/ingest/files/test.patch');
      expect(sanitizeHref('./download/patch.diff')).toBe('./download/patch.diff');
      expect(sanitizeHref('#turn-400')).toBe('#turn-400');
    });

    it('permits valid http and https URLs', () => {
      expect(sanitizeHref('http://127.0.0.1:47004/health')).toBe('http://127.0.0.1:47004/health');
      expect(sanitizeHref('https://github.com/forja-orca/tylluan')).toBe('https://github.com/forja-orca/tylluan');
    });

    it('returns # for invalid, malformed, empty or non-string inputs', () => {
      expect(sanitizeHref('')).toBe('#');
      expect(sanitizeHref('   ')).toBe('#');
      expect(sanitizeHref(null)).toBe('#');
      expect(sanitizeHref(undefined)).toBe('#');
      expect(sanitizeHref('not-a-valid-url-without-slash-or-scheme')).toBe('#');
    });
  });
});
