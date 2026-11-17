import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import React from 'react';
import { RepoImporterMockup } from '../RepoImporterMockup';

describe('RepoImporterMockup (ADR-033 R5 — maqueta simulada)', () => {
  it('valida URL antes de iniciar la importación simulada', () => {
    render(<RepoImporterMockup />);

    const input = screen.getByLabelText(/URL del repositorio a importar/i);
    fireEvent.change(input, { target: { value: 'no-es-url' } });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    expect(screen.getByRole('alert')).toHaveTextContent(/URL http:\/\/ o https:\/\/ válida/i);
    expect(screen.queryByRole('list', { name: /Estados del importador/i })).not.toBeInTheDocument();
  });

  it('recorre las fases ADR-033 hasta activo mostrando autoprueba de ruta sin llamar al backend', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    render(<RepoImporterMockup />);

    fireEvent.change(screen.getByLabelText(/URL del repositorio a importar/i), {
      target: { value: 'https://github.com/forja-orca/guild-demo' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    expect(screen.getByText(/Estado: cuarentena — clonado guild-demo/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Continuar a escaneo/i }));

    expect(screen.getByText(/Estado: escaneo de estructura/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Generar propuesta/i }));

    expect(screen.getByText(/Estado: propuesta de guild «imported-guild-demo»/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Pasar a compuertas/i }));

    expect(screen.getByText(/Estado: compuertas — hashes, CAPABILITIES/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Aprobar compuertas/i }));

    expect(screen.getByText(/autoprueba de ruta: precisión 92 %, roba 0 consultas/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Confirmar activo/i }));

    expect(screen.getByText(/Estado: activo — guild «imported-guild-demo»/i)).toBeInTheDocument();
    expect(fetchSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('permite pasar a estado rechazado al rechazar en compuertas', () => {
    render(<RepoImporterMockup />);

    fireEvent.change(screen.getByLabelText(/URL del repositorio a importar/i), {
      target: { value: 'https://github.com/forja-orca/guild-riesgo' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));
    fireEvent.click(screen.getByRole('button', { name: /Continuar a escaneo/i }));
    fireEvent.click(screen.getByRole('button', { name: /Generar propuesta/i }));
    fireEvent.click(screen.getByRole('button', { name: /Pasar a compuertas/i }));
    fireEvent.click(screen.getByRole('button', { name: /^Rechazar$/i }));

    expect(screen.getByText(/Estado: rechazado/i)).toBeInTheDocument();
  });

  it('muestra estado de carga con botones deshabilitados y avanza a cuarentena al resolver el mock', async () => {
    let resolveImport!: () => void;
    const onImportRepo = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveImport = resolve;
        }),
    );

    render(<RepoImporterMockup onImportRepo={onImportRepo} />);

    const input = screen.getByLabelText(/URL del repositorio a importar/i);
    fireEvent.change(input, { target: { value: 'https://github.com/forja-orca/guild-async' } });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    expect(screen.getByRole('button', { name: /Importando\.\.\./i })).toBeDisabled();
    expect(input).toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(/Clonando repositorio en cuarentena/i);

    resolveImport();
    await waitFor(() => {
      expect(screen.getByText(/Estado: cuarentena — clonado guild-async/i)).toBeInTheDocument();
    });
  });

  it('muestra alerta recuperable ante error de importación y permite reintentar', async () => {
    const onImportRepo = vi
      .fn()
      .mockRejectedValueOnce(new Error('Timeout clonando repositorio'))
      .mockResolvedValueOnce(undefined);

    render(<RepoImporterMockup onImportRepo={onImportRepo} />);

    fireEvent.change(screen.getByLabelText(/URL del repositorio a importar/i), {
      target: { value: 'https://github.com/forja-orca/guild-retry' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Importar repositorio/i }));

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(/Timeout clonando repositorio/i);

    fireEvent.click(screen.getByRole('button', { name: /Reintentar/i }));
    await waitFor(() => {
      expect(screen.getByText(/Estado: cuarentena — clonado guild-retry/i)).toBeInTheDocument();
    });
    expect(onImportRepo).toHaveBeenCalledTimes(2);
  });
});
