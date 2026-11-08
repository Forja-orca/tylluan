import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { MissingTokenBanner } from '../ui/MissingTokenBanner';
import { StandbyGracefulBanner } from '../ui/StandbyGracefulBanner';

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

describe('StandbyGracefulBanner (UI-CIERRE)', () => {
  it('renderiza la cuenta atrás asistida de 60s, backoff AG-28 y botón de reconexión', () => {
    const handleRetry = vi.fn();
    render(
      <StandbyGracefulBanner
        countdownSecs={48}
        reconnectAttempts={2}
        retrying={false}
        onRetryNow={handleRetry}
        lastKnownDetail="v0.17.2 (886f8542)"
      />
    );

    expect(screen.getByTestId('standby-graceful-banner')).toBeInTheDocument();
    expect(screen.getByText(/Kernel en Espera \(Standby Graceful\)/i)).toBeInTheDocument();
    expect(screen.getByText(/Próxima sonda: 48s/i)).toBeInTheDocument();
    expect(screen.getByText(/Backoff AG-28: 10s/i)).toBeInTheDocument();

    const retryBtn = screen.getByRole('button', { name: /reconectar ahora/i });
    fireEvent.click(retryBtn);
    expect(handleRetry).toHaveBeenCalledTimes(1);
  });
});
