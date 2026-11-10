import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { PatchAttachmentViewer } from '../coloquio/PatchAttachmentViewer';

describe('PatchAttachmentViewer', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('escapa el nombre del adjunto: no inyecta HTML en el visor', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, text: async () => '--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b' }));
    const evil = '<img src=x onerror="window.__xss=1">.patch';
    const { container } = render(<PatchAttachmentViewer url="http://127.0.0.1/p" name={evil} />);
    fireEvent.click(screen.getAllByRole('button')[0]);
    await waitFor(() => expect(container.querySelector('[data-patch-viewer]')).not.toBeNull());
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('[data-patch-viewer]')!.textContent).toContain('<img src=x');
  });

  it('sanitiza la URL del adjunto: neutraliza enlaces maliciosos javascript:', () => {
    const { container } = render(
      <PatchAttachmentViewer url="javascript:alert('pwned')" name="test.patch" />
    );
    const link = container.querySelector('a[title="Abrir archivo original"]');
    expect(link).not.toBeNull();
    expect(link?.getAttribute('href')).toBe('#');
  });
});
