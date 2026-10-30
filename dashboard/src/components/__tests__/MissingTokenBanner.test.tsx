import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MissingTokenBanner } from '../ui/MissingTokenBanner';

describe('MissingTokenBanner (AG-14)', () => {
  it('renderiza la información de seguridad activa y aviso dev_mode=false', () => {
    const handleOpen = vi.fn();
    render(<MissingTokenBanner onOpenAuthModal={handleOpen} />);

    expect(screen.getByRole('region', { name: /aviso de autenticación requerida/i })).toBeInTheDocument();
    expect(screen.getByText(/Seguridad Activa \(dev_mode=false\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Falta token de autenticación \(dev_mode=false\)/i)).toBeInTheDocument();
    expect(screen.getByText(/\.tylluan-token/i)).toBeInTheDocument();
  });

  it('llama a onOpenAuthModal al hacer clic en el botón de introducir token', () => {
    const handleOpen = vi.fn();
    render(<MissingTokenBanner onOpenAuthModal={handleOpen} />);

    const button = screen.getByRole('button', { name: /introducir token de acceso/i });
    expect(button).toBeInTheDocument();

    fireEvent.click(button);
    expect(handleOpen).toHaveBeenCalledTimes(1);
  });
});
