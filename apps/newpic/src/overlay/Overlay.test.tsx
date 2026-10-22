import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import { Overlay } from './Overlay';

const fixedOverlayInfo = {
  request: {
    action: 'fixed_region',
    mode: 'fixed_size' as const,
    fixed_size: { width: 800, height: 600 },
  },
  display_bounds: {
    x: -2560,
    y: 180,
    width: 2560,
    height: 1440,
  },
  scale_factor: 2,
  initial_rect: {
    x: -2360,
    y: 480,
    width: 800,
    height: 600,
  },
};

describe('F9 overlay', () => {
  beforeEach(() => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_overlay_info') {
        return fixedOverlayInfo;
      }
      return undefined;
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it('loads global physical geometry as display-local logical geometry', async () => {
    render(<Overlay />);

    const cutout = await screen.findByTestId('selection-cutout');
    expect(cutout).toHaveStyle({
      left: '100px',
      top: '150px',
      width: '400px',
      height: '300px',
    });
    expect(screen.getByTestId('dimension-badge')).toHaveTextContent('800 × 600');
  });

  it('renders a move surface, eight semantic handles, and the Enter instruction', async () => {
    render(<Overlay />);
    await screen.findByTestId('selection-cutout');

    expect(screen.getByLabelText('캡처 영역 이동')).toBeVisible();
    for (const label of [
      '위쪽 크기 조절',
      '오른쪽 위 크기 조절',
      '오른쪽 크기 조절',
      '오른쪽 아래 크기 조절',
      '아래쪽 크기 조절',
      '왼쪽 아래 크기 조절',
      '왼쪽 크기 조절',
      '왼쪽 위 크기 조절',
    ]) {
      expect(screen.getByRole('button', { name: label })).toBeVisible();
    }
    expect(screen.getByText(/드래그하여 이동/)).toHaveTextContent(
      '드래그하여 이동 · 모서리/테두리로 크기 조절 · Enter 캡처 · Esc 취소'
    );
  });

  it('captures the pointer while moving and completes only after Enter', async () => {
    render(<Overlay />);
    const moveSurface = await screen.findByLabelText('캡처 영역 이동');
    const container = screen.getByTestId('overlay-container');
    const setPointerCapture = vi.fn();
    const releasePointerCapture = vi.fn();
    Object.assign(container, {
      setPointerCapture,
      releasePointerCapture,
      hasPointerCapture: () => true,
    });

    fireEvent.pointerDown(moveSurface, {
      button: 0,
      pointerId: 7,
      clientX: 150,
      clientY: 200,
    });
    fireEvent.pointerMove(container, {
      pointerId: 7,
      clientX: 250,
      clientY: 300,
    });
    fireEvent.pointerUp(container, {
      button: 0,
      pointerId: 7,
      clientX: 250,
      clientY: 300,
    });

    expect(setPointerCapture).toHaveBeenCalledWith(7);
    expect(releasePointerCapture).toHaveBeenCalledWith(7);
    expect(invokeMock).not.toHaveBeenCalledWith('complete_selection', expect.anything());

    fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('complete_selection', {
        rect: { x: -2160, y: 680, width: 800, height: 600 },
      });
    });
  });
});
