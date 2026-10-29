import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { VisionTab } from '../VisionTab';
import type { NexusBridge } from '../../lib/api-client';

// Mock useNexus hook for VisionTab tests
vi.mock('../../hooks/useNexus', () => ({
  useNexus: vi.fn(() => ({
    guilds: [{ name: 'vision', status: 'running' }],
    refreshData: vi.fn(),
  })),
}));

describe('AG-13 — VisionTab Upload Bearer Wiring & 401 Flow', () => {
  const notifyMock = vi.fn();
  let uploadFileMock: ReturnType<typeof vi.fn>;
  let mockBridge: Partial<NexusBridge>;

  beforeEach(() => {
    vi.clearAllMocks();
    uploadFileMock = vi.fn();
    mockBridge = {
      uploadFile: uploadFileMock as unknown as NexusBridge['uploadFile'],
      fetchRaw: vi.fn().mockResolvedValue({}),
    };
    if (!globalThis.URL.createObjectURL) {
      globalThis.URL.createObjectURL = vi.fn(() => 'blob:mock-image-url');
    } else {
      vi.spyOn(globalThis.URL, 'createObjectURL').mockReturnValue('blob:mock-image-url');
    }
  });

  it('usa bridge.uploadFile pasando bearer y procesa respuesta 200 OK exitosa', async () => {
    uploadFileMock.mockResolvedValueOnce({
      status: 'ok',
      file: 'test_sample.png',
      original_name: 'test_sample.png',
      pipeline: 'ingest',
    });

    render(
      <VisionTab
        bridge={mockBridge as NexusBridge}
        notify={notifyMock}
      />
    );

    const file = new File(['dummy content'], 'test_sample.png', { type: 'image/png' });

    // Encontrar la zona de drop por el texto del placeholder
    const dropZone = screen.getByText(/Drag and drop an image here/i).closest('div');
    expect(dropZone).not.toBeNull();

    fireEvent.drop(dropZone!, {
      dataTransfer: {
        files: [file],
      },
    });

    await waitFor(() => {
      expect(uploadFileMock).toHaveBeenCalledTimes(1);
      expect(uploadFileMock).toHaveBeenCalledWith(file);
    });

    await waitFor(() => {
      expect(notifyMock).toHaveBeenCalledWith(
        expect.stringContaining('Uploaded: test_sample.png → data/ingest/test_sample.png'),
        'info'
      );
    });
  });

  it('propaga error 401 a través de bridge.uploadFile activando notificación de error', async () => {
    uploadFileMock.mockRejectedValueOnce(new Error('Unauthorized'));

    render(
      <VisionTab
        bridge={mockBridge as NexusBridge}
        notify={notifyMock}
      />
    );

    const file = new File(['dummy content'], 'private_scan.png', { type: 'image/png' });

    const dropZone = screen.getByText(/Drag and drop an image here/i).closest('div');
    expect(dropZone).not.toBeNull();

    fireEvent.drop(dropZone!, {
      dataTransfer: {
        files: [file],
      },
    });

    await waitFor(() => {
      expect(uploadFileMock).toHaveBeenCalledTimes(1);
      expect(uploadFileMock).toHaveBeenCalledWith(file);
    });

    await waitFor(() => {
      expect(notifyMock).toHaveBeenCalledWith(
        expect.stringContaining('Upload error: Unauthorized'),
        'error'
      );
    });
  });
});
