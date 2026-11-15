import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { AuthModal } from '../AuthModal';

describe('AuthModal — Copiar token de conexión', () => {
  it('copia el token guardado al portapapeles en 1 clic', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText, readText: vi.fn() },
      configurable: true,
    });

    render(
      <AuthModal
        isOpen={true}
        onClose={vi.fn()}
        onSave={vi.fn()}
        initialToken="  tok-secreto-123  "
      />
    );

    const copyBtn = screen.getByRole('button', { name: /Copiar token al portapapeles/i });
    fireEvent.click(copyBtn);

    await waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('tok-secreto-123');
      expect(screen.getByText('Copiado')).toBeInTheDocument();
    });
  });

  it('no muestra el botón Copiar token si el campo está vacío y avisa al operador', () => {
    localStorage.removeItem('tylluan_token');
    render(<AuthModal isOpen={true} onClose={vi.fn()} onSave={vi.fn()} initialToken="" />);
    expect(screen.queryByRole('button', { name: /Copiar token al portapapeles/i })).not.toBeInTheDocument();
    expect(screen.getByText(/Sin token guardado en este navegador/i)).toBeInTheDocument();
  });
});
