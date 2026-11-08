import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import React, { useState, useEffect } from 'react';
import { StandbyGracefulBanner } from '../ui/StandbyGracefulBanner';

describe('StandbyGracefulBanner & Real Probe Countdown', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders exact seconds to next probe honestly without invented numbers', () => {
    const handleRetry = vi.fn();
    render(
      <StandbyGracefulBanner
        countdownSecs={5}
        totalStepSecs={5}
        reconnectAttempts={1}
        retrying={false}
        onRetryNow={handleRetry}
        lastKnownDetail="v0.17.2 (45e0af94)"
      />
    );

    expect(screen.getByTestId('standby-graceful-banner')).toBeInTheDocument();
    expect(screen.getByText('Kernel en Espera (Standby Graceful)')).toBeInTheDocument();
    expect(screen.getByText('Próxima sonda: 5s')).toBeInTheDocument();
    expect(screen.getByText(/Backoff AG-28: 5s/)).toBeInTheDocument();
    expect(screen.getByText(/Estado local preservado/)).toBeInTheDocument();

    const retryBtn = screen.getByRole('button', { name: /Reconectar ahora/i });
    fireEvent.click(retryBtn);
    expect(handleRetry).toHaveBeenCalledTimes(1);
  });

  it('renders retrying state cleanly when probe is in-flight', () => {
    render(
      <StandbyGracefulBanner
        countdownSecs={0}
        totalStepSecs={10}
        reconnectAttempts={2}
        retrying={true}
        onRetryNow={vi.fn()}
      />
    );

    expect(screen.getByText('Sondeando /health...')).toBeInTheDocument();
    expect(screen.getByText(/Backoff AG-28: 10s/)).toBeInTheDocument();
    expect(screen.getByText('Sondeando kernel...')).toBeInTheDocument();
  });

  it('simulates honest wall-clock countdown ticks matching real probe schedule', () => {
    const onProbe = vi.fn();

    // Harness mimicking useNexusSSE real next-probe ticker
    const TestHarness: React.FC = () => {
      const [secondsLeft, setSecondsLeft] = useState(5);
      const [attempts, setAttempts] = useState(1);
      const [retrying, setRetrying] = useState(false);

      useEffect(() => {
        const interval = setInterval(() => {
          setSecondsLeft((prev) => {
            if (prev > 1) {
              return prev - 1;
            }
            // Trigger probe
            setRetrying(true);
            onProbe();
            // Next backoff step (e.g. 10s)
            setAttempts(2);
            setRetrying(false);
            return 10;
          });
        }, 1000);
        return () => clearInterval(interval);
      }, []);

      return (
        <StandbyGracefulBanner
          countdownSecs={secondsLeft}
          totalStepSecs={attempts === 1 ? 5 : 10}
          reconnectAttempts={attempts}
          retrying={retrying}
          onRetryNow={() => {
            setSecondsLeft(0);
            onProbe();
          }}
        />
      );
    };

    render(<TestHarness />);

    expect(screen.getByText('Próxima sonda: 5s')).toBeInTheDocument();

    // Advance 1s -> 4s
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText('Próxima sonda: 4s')).toBeInTheDocument();

    // Advance 3s more -> 1s
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByText('Próxima sonda: 1s')).toBeInTheDocument();

    // Advance 1s -> probe fires and resets to 10s
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(onProbe).toHaveBeenCalledTimes(1);
    expect(screen.getByText('Próxima sonda: 10s')).toBeInTheDocument();
    expect(screen.getByText(/Backoff AG-28: 10s/)).toBeInTheDocument();
  });
});
