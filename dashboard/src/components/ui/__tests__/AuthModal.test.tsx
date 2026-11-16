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

  it('atrapa el foco con Tab y Shift+Tab dentro del modal', () => {
    render(<AuthModal isOpen={true} onClose={vi.fn()} onSave={vi.fn()} initialToken="" />);
    const closeBtn = screen.getByRole('button', { name: /Cerrar modal de autenticación/i });
    const submitBtn = screen.getByRole('button', { name: /Guardar y Conectar/i });

    submitBtn.focus();
    expect(document.activeElement).toBe(submitBtn);
    fireEvent.keyDown(window, { key: 'Tab' });
    expect(document.activeElement).toBe(closeBtn);

    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(submitBtn);
  });

  it('devuelve el foco al botón disparador al cerrarse', async () => {
    function Harness() {
      const [open, setOpen] = React.useState(false);
      return (
        <div>
          <button type="button" onClick={() => setOpen(true)}>
            Abrir Token
          </button>
          <AuthModal isOpen={open} onClose={() => setOpen(false)} onSave={vi.fn()} initialToken="" />
        </div>
      );
    }

    render(<Harness />);
    const trigger = screen.getByRole('button', { name: /Abrir Token/i });
    trigger.focus();
    expect(document.activeElement).toBe(trigger);

    fireEvent.click(trigger);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      expect(document.activeElement).toBe(trigger);
    });
  });
});
