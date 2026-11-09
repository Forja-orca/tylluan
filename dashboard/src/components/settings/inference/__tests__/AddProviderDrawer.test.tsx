import React from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { AddProviderDrawer } from '../AddProviderDrawer';
import type { NexusBridge } from '../../../../lib/nexus-bridge';

describe('AddProviderDrawer (T-UI-2 / SPEC §4, §6, §7)', () => {
  let mockBridge: { fetchRaw: ReturnType<typeof vi.fn> };
  let mockNotify: (msg: string, type?: 'info' | 'error') => void;
  let mockOnClose: () => void;
  let mockOnProviderAdded: () => void;

  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
    mockBridge = {
      fetchRaw: vi.fn(),
    };
    mockNotify = vi.fn();
    mockOnClose = vi.fn();
    mockOnProviderAdded = vi.fn();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('no renderiza nada cuando isOpen es false', () => {
    render(
      <AddProviderDrawer
        isOpen={false}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
      />
    );
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('renderiza el drawer accesible con los 4 campos obligatorios y la clave oculta (type="password")', () => {
    render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        kernelSupportsPost={true}
      />
    );

    expect(screen.getByRole('dialog', { name: /añadir proveedor de inferencia/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/nombre del proveedor/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/url base/i)).toBeInTheDocument();

    const keyInput = screen.getByLabelText(/clave de api/i) as HTMLInputElement;
    expect(keyInput).toBeInTheDocument();
    expect(keyInput.type).toBe('password');
    expect(keyInput.getAttribute('autocomplete')).toBe('new-password');

    expect(screen.getByLabelText(/modelos disponibles/i)).toBeInTheDocument();
    expect(screen.getByText(/la clave se guarda en el almacén seguro del sistema/i)).toBeInTheDocument();
  });

  it('botón de guardado deshabilitado con "disponible cuando el kernel lo soporte" si el kernel aún no implementa D-157', () => {
    render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        kernelSupportsPost={false}
      />
    );

    const submitBtn = screen.getByRole('button', { name: /guardar.*disponible/i });
    expect(submitBtn).toBeDisabled();
  });

  it('gestiona error 409 (ETag rancio) o fallos de red mostrando el error exacto y sin emitir "añadido correctamente"', async () => {
    mockBridge.fetchRaw.mockRejectedValueOnce(new Error('HTTP Error: 409 Conflict (ETag mismatch)'));

    render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        onProviderAdded={mockOnProviderAdded}
        kernelSupportsPost={true}
      />
    );

    fireEvent.change(screen.getByLabelText(/nombre del proveedor/i), {
      target: { value: 'OpenRouter Cloud' },
    });
    fireEvent.change(screen.getByLabelText(/url base/i), {
      target: { value: 'https://openrouter.ai/api/v1' },
    });
    const keyInput = screen.getByLabelText(/clave de api/i) as HTMLInputElement;
    fireEvent.change(keyInput, {
      target: { value: 'sk-test-secret' },
    });

    const submitBtn = screen.getByRole('button', { name: /^guardar$/i });
    fireEvent.click(submitBtn);

    await waitFor(() => {
      // Debe notificar el error exacto y jamás "añadido correctamente"
      expect(mockNotify).toHaveBeenCalledWith(
        expect.stringContaining('Error al guardar proveedor: HTTP Error: 409 Conflict'),
        'error'
      );
      expect(mockNotify).not.toHaveBeenCalledWith(
        expect.stringContaining('añadido correctamente'),
        'info'
      );
      // No debe cerrar el drawer ni invocar onProviderAdded
      expect(mockOnProviderAdded).not.toHaveBeenCalled();
      expect(mockOnClose).not.toHaveBeenCalled();
    });

    // La clave debe haberse purgado del input por seguridad incluso tras fallo
    expect(keyInput.value).toBe('');
  });

  it('guarda con éxito cuando kernelSupportsPost es true y NUNCA persiste la clave en localStorage ni en estado tras enviar', async () => {
    mockBridge.fetchRaw.mockResolvedValueOnce({ ok: true, name: 'openrouter' });

    render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        onProviderAdded={mockOnProviderAdded}
        kernelSupportsPost={true}
      />
    );

    fireEvent.change(screen.getByLabelText(/nombre del proveedor/i), {
      target: { value: 'OpenRouter Cloud' },
    });
    fireEvent.change(screen.getByLabelText(/url base/i), {
      target: { value: 'https://openrouter.ai/api/v1' },
    });
    const keyInput = screen.getByLabelText(/clave de api/i) as HTMLInputElement;
    fireEvent.change(keyInput, {
      target: { value: 'sk-secret-key-to-wipe' },
    });
    fireEvent.change(screen.getByLabelText(/modelos disponibles/i), {
      target: { value: 'gpt-4o, claude-3-5-sonnet' },
    });

    const submitBtn = screen.getByRole('button', { name: /^guardar$/i });
    expect(submitBtn).not.toBeDisabled();
    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(mockBridge.fetchRaw).toHaveBeenCalledWith(
        '/api/v1/external-providers',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            name: 'openrouter-cloud',
            base_url: 'https://openrouter.ai/api/v1',
            api_key: 'sk-secret-key-to-wipe',
            models: ['gpt-4o', 'claude-3-5-sonnet'],
          }),
        })
      );
      expect(mockNotify).toHaveBeenCalledWith(
        expect.stringContaining('añadido correctamente'),
        'info'
      );
      expect(mockOnProviderAdded).toHaveBeenCalled();
      expect(mockOnClose).toHaveBeenCalled();
    });

    // Invariante de seguridad estricta (SPEC §6 / T3696):
    // 1. Cero rastro en localStorage
    expect(localStorage.getItem('api_key')).toBeNull();
    expect(localStorage.getItem('openrouter_key')).toBeNull();
    expect(localStorage.length).toBe(0);
    // 2. Cero rastro en sessionStorage
    expect(sessionStorage.length).toBe(0);
  });

  it('al pulsar Cancelar limpia inmediatamente la clave en memoria y cierra', () => {
    render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        kernelSupportsPost={true}
      />
    );

    const keyInput = screen.getByLabelText(/clave de api/i) as HTMLInputElement;
    fireEvent.change(keyInput, {
      target: { value: 'sk-temporary-key' },
    });
    expect(keyInput.value).toBe('sk-temporary-key');

    fireEvent.click(screen.getByRole('button', { name: /cancelar/i }));
    expect(mockOnClose).toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
  });

  it('accesibilidad WCAG AA: atrapa el foco cíclicamente con Tab/Shift+Tab y Escape cierra el panel', async () => {
    // Simulamos un botón externo que abrió el drawer
    const triggerBtn = document.createElement('button');
    triggerBtn.textContent = 'Añadir Proveedor Trigger';
    document.body.appendChild(triggerBtn);
    triggerBtn.focus();
    expect(document.activeElement).toBe(triggerBtn);

    const { unmount } = render(
      <AddProviderDrawer
        isOpen={true}
        onClose={mockOnClose}
        bridge={mockBridge as unknown as NexusBridge}
        notify={mockNotify}
        kernelSupportsPost={true}
      />
    );

    // Esperar a que el foco inicial se sitúe en el input de nombre
    await waitFor(() => {
      const nameInput = screen.getByLabelText(/nombre del proveedor/i);
      expect(document.activeElement).toBe(nameInput);
    });

    const closeBtn = screen.getByRole('button', { name: /cerrar formulario/i });
    const cancelBtn = screen.getByRole('button', { name: /cancelar/i });

    // Focus-trap hacia atrás (Shift+Tab en el primer elemento interactivo)
    closeBtn.focus();
    expect(document.activeElement).toBe(closeBtn);
    fireEvent.keyDown(window, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(cancelBtn);

    // Focus-trap hacia adelante (Tab en el último elemento interactivo)
    cancelBtn.focus();
    expect(document.activeElement).toBe(cancelBtn);
    fireEvent.keyDown(window, { key: 'Tab', shiftKey: false });
    expect(document.activeElement).toBe(closeBtn);

    // Escape cierra el drawer
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(mockOnClose).toHaveBeenCalled();

    // Desmontar el drawer y verificar que el foco vuelve al botón disparador
    unmount();
    expect(document.activeElement).toBe(triggerBtn);

    document.body.removeChild(triggerBtn);
  });
});
