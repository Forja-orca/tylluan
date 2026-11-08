import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { TwoLevelModelSelector } from '../TwoLevelModelSelector';
import { LlamaCppLauncherCard } from '../LlamaCppLauncherCard';
import { DecisionPlaneCard } from '../DecisionPlaneCard';
import { FIXTURE_MODEL_CATALOG, FIXTURE_LLAMA_SERVER_STATUS } from '../mock-catalog-fixture';

describe('TwoLevelModelSelector (ADR-020 v8 / Directiva José T3140 + T3144)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('renderiza por defecto los modelos más recientes (Nivel 1) y permite seleccionar', () => {
    const handleSelect = vi.fn();
    render(
      <TwoLevelModelSelector
        models={FIXTURE_MODEL_CATALOG.models}
        onSelectModel={handleSelect}
      />
    );

    // Debe mostrar la cabecera y modelos del Nivel 1
    expect(screen.getByText(/Selector de Modelos Soberano/i)).toBeInTheDocument();
    expect(screen.getByText(/Nivel 1: Modelos Más Recientes/i)).toBeInTheDocument();

    // Qwen 2.5 Coder 7B y Claude 4.6 Sonnet son punteros (is_latest_in_family: true)
    expect(screen.getAllByText(/Qwen 2.5 Coder 7B/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Claude 4.6 Sonnet/i)).toBeInTheDocument();

    // Seleccionar Qwen 2.5 Coder 7B
    const qwenBtn = screen.getByRole('option', { name: /Qwen 2.5 Coder 7B/i });
    fireEvent.click(qwenBtn);

    expect(handleSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'local::qwen2.5-coder-7b' })
    );
    expect(localStorage.getItem('tylluan_selected_model_id')).toBe('local::qwen2.5-coder-7b');
  });

  it('despliega el Nivel 2 ("Otros modelos") y los agrupa por proveedor', () => {
    render(
      <TwoLevelModelSelector
        models={FIXTURE_MODEL_CATALOG.models}
        onSelectModel={vi.fn()}
      />
    );

    const toggleBtn = screen.getByRole('button', { name: /Nivel 2: Otros modelos del catálogo/i });
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'false');

    // Desplegar Nivel 2
    fireEvent.click(toggleBtn);
    expect(toggleBtn).toHaveAttribute('aria-expanded', 'true');

    // Claude 3.5 Sonnet es secundario y debe aparecer agrupado bajo "anthropic"
    expect(screen.getByText(/Claude 3.5 Sonnet \(Legacy\)/i)).toBeInTheDocument();
  });

  it('cambia a la pestaña de Decisión y muestra modelos tipados con sus interfaces (T3144)', () => {
    render(
      <TwoLevelModelSelector
        models={FIXTURE_MODEL_CATALOG.models}
        onSelectModel={vi.fn()}
      />
    );

    const decisionTab = screen.getByRole('tab', { name: /Decisión \(T3144\)/i });
    fireEvent.click(decisionTab);

    // Modelos de decisión deben estar visibles
    expect(screen.getByText(/Laya 8B Decision Router/i)).toBeInTheDocument();
    expect(screen.getByText(/Jev Decision Engine 1.0/i)).toBeInTheDocument();
    expect(screen.getByText(/Arch-Router 1.5B Fast Intent/i)).toBeInTheDocument();
  });

  it('filtra modelos reactivamente según las capacidades seleccionadas (ej. Código)', () => {
    render(
      <TwoLevelModelSelector
        models={FIXTURE_MODEL_CATALOG.models}
        onSelectModel={vi.fn()}
      />
    );

    const codingChip = screen.getByRole('button', { name: /Código/i });
    fireEvent.click(codingChip);

    // Qwen tiene capacidad coding, debe seguir presente
    expect(screen.getAllByText(/Qwen 2.5 Coder 7B/i).length).toBeGreaterThanOrEqual(1);
  });
});

describe('LlamaCppLauncherCard (Control nativo del usuario)', () => {
  it('muestra estado activo, puerto :8080 y enlace a la Web UI nativa', () => {
    const handleToggle = vi.fn().mockResolvedValue(undefined);
    const handleUpdateDir = vi.fn().mockResolvedValue(undefined);
    const handleCheckUpdate = vi.fn().mockResolvedValue(undefined);

    render(
      <LlamaCppLauncherCard
        status={FIXTURE_LLAMA_SERVER_STATUS}
        onToggleServer={handleToggle}
        onUpdateDirectory={handleUpdateDir}
        onCheckUpdate={handleCheckUpdate}
      />
    );

    expect(screen.getByText(/Servidor llama.cpp del Usuario/i)).toBeInTheDocument();
    expect(screen.getByText(/Activo \(:8080\)/i)).toBeInTheDocument();
    expect(screen.getByText(/PID 41208/i)).toBeInTheDocument();

    const link = screen.getByRole('link', { name: /Abrir Web UI/i });
    expect(link).toHaveAttribute('href', 'http://127.0.0.1:8080');

    const stopBtn = screen.getByRole('button', { name: /Detener/i });
    fireEvent.click(stopBtn);
    expect(handleToggle).toHaveBeenCalledWith(false);
  });

  it('muestra badge "Detenido" cuando running es false y renderiza puerto dinámico (N1 / N2)', () => {
    const handleToggle = vi.fn().mockResolvedValue(undefined);
    render(
      <LlamaCppLauncherCard
        status={{
          ...FIXTURE_LLAMA_SERVER_STATUS,
          running: false,
          port: 8085,
          pid: null,
        }}
        onToggleServer={handleToggle}
        onUpdateDirectory={vi.fn()}
        onCheckUpdate={vi.fn()}
      />
    );

    expect(screen.getByText(/Detenido/i)).toBeInTheDocument();
    expect(screen.queryByText(/Activo \(:8085\)/i)).not.toBeInTheDocument();

    const startBtn = screen.getByRole('button', { name: /Lanzar/i });
    fireEvent.click(startBtn);
    expect(handleToggle).toHaveBeenCalledWith(true);
  });
});

describe('DecisionPlaneCard (Enrutamiento automático y Presupuesto de RAM)', () => {
  it('renderiza el toggle de auto-enrutamiento, selector de modelo y medidor de presupuesto', () => {
    const handleToggleAuto = vi.fn();
    const handleSelectDm = vi.fn();
    const handleUpdateThreshold = vi.fn();

    const decisionModels = FIXTURE_MODEL_CATALOG.models.filter(m => m.model_type === 'decision');

    render(
      <DecisionPlaneCard
        decisionModels={decisionModels}
        activeDecisionModelId="local::laya-8b"
        onSelectDecisionModel={handleSelectDm}
        autoRoutingEnabled={true}
        onToggleAutoRouting={handleToggleAuto}
        confidenceThreshold={0.75}
        onUpdateConfidenceThreshold={handleUpdateThreshold}
        ramBudget={FIXTURE_MODEL_CATALOG.tylluan_ram_budget}
        lastDecisionTrace={{
          timestamp: Date.now(),
          chosenModel: 'Qwen 2.5 Coder 7B',
          confidence: 0.88,
          latencyMs: 19,
          intentCategory: 'coding'
        }}
      />
    );

    expect(screen.getByText(/Plano de Decisión y Enrutamiento \(T3144\)/i)).toBeInTheDocument();
    expect(screen.getByText(/1420 MB \/ 3072 MB/i)).toBeInTheDocument();
    expect(screen.getByText(/46%/i)).toBeInTheDocument();
    expect(screen.getByText(/Último enrutado:/i)).toBeInTheDocument();
    expect(screen.getByText(/Qwen 2.5 Coder 7B/i)).toBeInTheDocument();
  });

  it('indica saturación visual cuando el presupuesto de RAM privada roza el límite (3072 - epsilon)', () => {
    const decisionModels = FIXTURE_MODEL_CATALOG.models.filter(m => m.model_type === 'decision');

    render(
      <DecisionPlaneCard
        decisionModels={decisionModels}
        activeDecisionModelId="local::laya-8b"
        onSelectDecisionModel={vi.fn()}
        autoRoutingEnabled={true}
        onToggleAutoRouting={vi.fn()}
        confidenceThreshold={0.80}
        onUpdateConfidenceThreshold={vi.fn()}
        ramBudget={{
          total_allowed_mb: 3072,
          current_used_private_mb: 3070, // 3072 - epsilon
          current_peak_private_mb: 3070,
        }}
      />
    );

    // Muestra 3070 MB / 3072 MB y 100% de ocupación
    expect(screen.getByText(/3070 MB \/ 3072 MB/i)).toBeInTheDocument();
    expect(screen.getByText(/100%/i)).toBeInTheDocument();
  });
});
