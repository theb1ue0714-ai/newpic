import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
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
  active_display: true,
  initial_rect: {
    x: -2360,
    y: 480,
    width: 800,
    height: 600,
  },
};

function installPointerCapture(container: HTMLElement) {
  let capturedPointer: number | null = null;
  const setPointerCapture = vi.fn((pointerId: number) => {
    capturedPointer = pointerId;
  });
  const releasePointerCapture = vi.fn((pointerId: number) => {
    if (capturedPointer === pointerId) {
      capturedPointer = null;
    }
  });
  Object.assign(container, {
    setPointerCapture,
    releasePointerCapture,
    hasPointerCapture: (pointerId: number) => capturedPointer === pointerId,
  });
  return { setPointerCapture, releasePointerCapture };
}

describe('capture overlay', () => {
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

  it('exposes named keyboard-operable controls and targets a real east resize', async () => {
    render(<Overlay />);
    const container = screen.getByTestId('overlay-container');
    const pointerCapture = installPointerCapture(container);
    await screen.findByTestId('selection-cutout');

    const handles = container.querySelectorAll<HTMLElement>('[data-handle]');
    expect(handles).toHaveLength(8);
    expect(screen.getByRole('button', { name: '캡처 영역 이동' })).toBeVisible();
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

    const east = container.querySelector<HTMLElement>('[data-handle="e"]');
    expect(east).not.toBeNull();
    fireEvent.pointerDown(east!, {
      button: 0,
      pointerId: 7,
      clientX: 500,
      clientY: 200,
    });
    fireEvent.pointerMove(container, {
      pointerId: 7,
      clientX: 550,
      clientY: 200,
    });
    expect(pointerCapture.setPointerCapture).toHaveBeenCalledWith(7);
    expect(invokeMock).not.toHaveBeenCalledWith('complete_selection', expect.anything());

    fireEvent.keyDown(window, { key: 'Enter' });
    expect(pointerCapture.releasePointerCapture).toHaveBeenCalledWith(7);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('complete_selection', {
        rect: { x: -2360, y: 480, width: 900, height: 600 },
      });
    });
  });

  it('moves and resizes by Arrow keys with Shift acceleration, then confirms globally', async () => {
    render(<Overlay />);
    await screen.findByTestId('selection-cutout');
    const move = screen.getByRole('button', { name: '캡처 영역 이동' });
    const east = screen.getByRole('button', { name: '오른쪽 크기 조절' });

    move.focus();
    expect(move).toHaveFocus();
    fireEvent.keyDown(move, { key: 'ArrowRight' });
    fireEvent.keyDown(move, { key: 'ArrowDown', shiftKey: true });
    east.focus();
    expect(east).toHaveFocus();
    fireEvent.keyDown(east, { key: 'ArrowRight', shiftKey: true });
    fireEvent.keyDown(east, { key: 'Enter' });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('complete_selection', {
        rect: { x: -2358, y: 500, width: 820, height: 600 },
      });
    });
  });

  it('shows and completes F9 on exactly one active display overlay', async () => {
    const infos = [
      fixedOverlayInfo,
      {
        ...fixedOverlayInfo,
        display_bounds: { x: 0, y: 0, width: 1920, height: 1080 },
        scale_factor: 1,
        active_display: false,
        initial_rect: undefined,
      },
    ];
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_overlay_info') {
        return infos.shift();
      }
      return undefined;
    });

    render(
      <>
        <Overlay />
        <Overlay />
      </>
    );

    await waitFor(() => {
      expect(screen.getAllByText(/Enter 캡처/)).toHaveLength(1);
    });
    expect(screen.getAllByTestId('selection-cutout')).toHaveLength(1);

    fireEvent.keyDown(window, { key: 'Enter' });
    await waitFor(() => {
      expect(
        invokeMock.mock.calls.filter(([command]) => command === 'complete_selection')
      ).toHaveLength(1);
    });
  });

  it('does not default a missing fixed-mode active-display flag to true', async () => {
    const { active_display: _activeDisplay, ...withoutActiveDisplay } = fixedOverlayInfo;
    invokeMock.mockResolvedValue(withoutActiveDisplay);
    render(<Overlay />);

    await waitFor(() => {
      expect(screen.getByTestId('overlay-container')).toHaveAttribute(
        'data-ready',
        'true'
      );
    });
    expect(screen.getByTestId('overlay-container')).toHaveAttribute(
      'data-active-display',
      'false'
    );
    expect(screen.queryByTestId('selection-cutout')).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(invokeMock).not.toHaveBeenCalledWith('complete_selection', expect.anything());
    expect(invokeMock).toHaveBeenCalledWith('cancel_selection');
  });

  it('ignores pointer and keyboard input while overlay metadata is loading', async () => {
    let resolveInfo!: (value: typeof fixedOverlayInfo) => void;
    const deferred = new Promise<typeof fixedOverlayInfo>((resolve) => {
      resolveInfo = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_overlay_info') {
        return deferred;
      }
      return Promise.resolve(undefined);
    });
    render(<Overlay />);
    const container = screen.getByTestId('overlay-container');
    installPointerCapture(container);
    expect(container).toHaveAttribute('data-ready', 'false');

    fireEvent.pointerDown(container, {
      button: 0,
      pointerId: 4,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerMove(container, { pointerId: 4, clientX: 200, clientY: 200 });
    fireEvent.pointerUp(container, {
      button: 0,
      pointerId: 4,
      clientX: 200,
      clientY: 200,
    });
    fireEvent.keyDown(window, { key: 'Enter' });
    fireEvent.keyDown(window, { key: 'Escape' });

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenCalledWith('get_overlay_info');
    expect(invokeMock).toHaveBeenCalledWith('cancel_selection');
    expect(screen.queryByTestId('selection-cutout')).not.toBeInTheDocument();

    await act(async () => {
      resolveInfo(fixedOverlayInfo);
      await deferred;
    });
    expect(screen.queryByTestId('selection-cutout')).not.toBeInTheDocument();
  });

  it('still sends Escape cancellation after metadata loading fails', async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === 'get_overlay_info') {
        return Promise.reject(new Error('metadata unavailable'));
      }
      return Promise.resolve(undefined);
    });
    render(<Overlay />);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith('get_overlay_info'));

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(invokeMock).toHaveBeenCalledWith('cancel_selection');
    expect(invokeMock).not.toHaveBeenCalledWith('complete_selection', expect.anything());
  });

  it('keeps the dimension badge visible when the region touches every display edge', async () => {
    invokeMock.mockResolvedValue({
      ...fixedOverlayInfo,
      initial_rect: { x: -2560, y: 180, width: 2560, height: 1440 },
    });
    render(<Overlay />);

    const badge = await screen.findByTestId('dimension-badge');
    expect(badge).toHaveStyle({ left: '1184px', top: '0px' });
    expect(badge).toHaveTextContent('2560 × 1440');
  });

  it('releases pointer capture and cancels through Escape IPC', async () => {
    render(<Overlay />);
    const moveSurface = await screen.findByLabelText('캡처 영역 이동');
    const container = screen.getByTestId('overlay-container');
    const pointerCapture = installPointerCapture(container);
    fireEvent.pointerDown(moveSurface, {
      button: 0,
      pointerId: 9,
      clientX: 150,
      clientY: 200,
    });

    fireEvent.keyDown(window, { key: 'Escape' });

    expect(pointerCapture.releasePointerCapture).toHaveBeenCalledWith(9);
    expect(invokeMock).toHaveBeenCalledWith('cancel_selection');
    expect(screen.queryByTestId('selection-cutout')).not.toBeInTheDocument();
  });

  it.each(['pointerCancel', 'lostPointerCapture'] as const)(
    'clears dragging after %s',
    async (eventName) => {
      render(<Overlay />);
      const moveSurface = await screen.findByLabelText('캡처 영역 이동');
      const container = screen.getByTestId('overlay-container');
      const pointerCapture = installPointerCapture(container);
      fireEvent.pointerDown(moveSurface, {
        button: 0,
        pointerId: 11,
        clientX: 150,
        clientY: 200,
      });
      fireEvent[eventName](container, { pointerId: 11 });
      fireEvent.pointerMove(container, {
        pointerId: 11,
        clientX: 350,
        clientY: 400,
      });
      fireEvent.keyDown(window, { key: 'Enter' });

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('complete_selection', {
          rect: { x: -2360, y: 480, width: 800, height: 600 },
        });
      });
      if (eventName === 'pointerCancel') {
        expect(pointerCapture.releasePointerCapture).toHaveBeenCalledWith(11);
      }
    }
  );

  it('keeps F7 pointer-up completion after metadata loads', async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === 'get_overlay_info') {
        return {
          request: { action: 'region', mode: 'free_region' as const },
          display_bounds: fixedOverlayInfo.display_bounds,
          scale_factor: 2,
          active_display: false,
        };
      }
      return undefined;
    });
    render(<Overlay />);
    await screen.findByText(/영역을 선택하세요/);
    const container = screen.getByTestId('overlay-container');
    installPointerCapture(container);

    fireEvent.pointerDown(container, {
      button: 0,
      pointerId: 12,
      clientX: 10,
      clientY: 20,
    });
    fireEvent.pointerMove(container, {
      pointerId: 12,
      clientX: 110,
      clientY: 70,
    });
    fireEvent.pointerUp(container, {
      button: 0,
      pointerId: 12,
      clientX: 110,
      clientY: 70,
    });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('complete_selection', {
        rect: { x: -2540, y: 220, width: 200, height: 100 },
      });
    });
  });
});
