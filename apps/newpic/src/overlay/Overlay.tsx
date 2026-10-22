import React, { useCallback, useEffect, useReducer } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  calculateFixedRect,
  initialSelectionState,
  localLogicalDisplaySize,
  logicalToPhysical,
  physicalSizeForDisplay,
  physicalToLogical,
  selectionReducer,
  type DisplayBounds,
  type Rect,
  type ResizeHandle,
} from './selection';
import './overlay.css';

interface OverlayInfo {
  request: {
    action: string;
    mode: 'free_region' | 'fixed_size';
    fixed_size?: { width: number; height: number };
    fallback_from?: string;
  };
  display_bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  scale_factor: number;
  initial_rect?: Rect;
}

const RESIZE_HANDLES: ReadonlyArray<{
  handle: ResizeHandle;
  label: string;
}> = [
  { handle: 'n', label: '위쪽 크기 조절' },
  { handle: 'ne', label: '오른쪽 위 크기 조절' },
  { handle: 'e', label: '오른쪽 크기 조절' },
  { handle: 'se', label: '오른쪽 아래 크기 조절' },
  { handle: 's', label: '아래쪽 크기 조절' },
  { handle: 'sw', label: '왼쪽 아래 크기 조절' },
  { handle: 'w', label: '왼쪽 크기 조절' },
  { handle: 'nw', label: '왼쪽 위 크기 조절' },
];

function fixedInitialRect(info: OverlayInfo, displayBounds: DisplayBounds): Rect {
  if (info.initial_rect) {
    return physicalToLogical(info.initial_rect, displayBounds);
  }

  const localDisplay = localLogicalDisplaySize(displayBounds);
  const localSize = physicalSizeForDisplay(
    info.request.fixed_size ?? { width: 800, height: 600 },
    displayBounds
  );
  return calculateFixedRect(
    { x: localDisplay.width / 2, y: localDisplay.height / 2 },
    localSize,
    displayBounds
  );
}

function pointerTarget(element: EventTarget | null): 'rect' | ResizeHandle | undefined {
  if (!(element instanceof Element)) {
    return undefined;
  }
  const target = element.closest<HTMLElement>('[data-handle], [data-selection-target="rect"]');
  if (!target) {
    return undefined;
  }
  return (target.dataset.handle as ResizeHandle | undefined) ?? 'rect';
}

export function Overlay() {
  const [state, dispatch] = useReducer(
    selectionReducer,
    {
      mode: 'free_region',
      displayBounds: {
        x: 0,
        y: 0,
        width: typeof window !== 'undefined' ? window.innerWidth : 1920,
        height: typeof window !== 'undefined' ? window.innerHeight : 1080,
        scaleFactor: typeof window !== 'undefined' ? window.devicePixelRatio : 1,
      },
    },
    initialSelectionState
  );

  useEffect(() => {
    let mounted = true;
    async function initOverlay() {
      try {
        const info = await invoke<OverlayInfo>('get_overlay_info');
        if (!info || !mounted) {
          return;
        }
        const displayBounds: DisplayBounds = {
          x: info.display_bounds.x,
          y: info.display_bounds.y,
          width: info.display_bounds.width,
          height: info.display_bounds.height,
          scaleFactor: info.scale_factor || 1,
        };
        dispatch({
          type: 'LOAD_OVERLAY',
          mode: info.request.mode,
          displayBounds,
          rect:
            info.request.mode === 'fixed_size'
              ? fixedInitialRect(info, displayBounds)
              : undefined,
        });
      } catch {
        // Ignored when running outside Tauri or in headless tests.
      }
    }
    void initOverlay();
    return () => {
      mounted = false;
    };
  }, []);

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) {
        return;
      }
      const target = state.mode === 'fixed_size' ? pointerTarget(event.target) : undefined;
      if (state.mode === 'fixed_size' && !target) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      dispatch({
        type: 'POINTER_DOWN',
        point: { x: event.clientX, y: event.clientY },
        target,
      });
    },
    [state.mode]
  );

  const handlePointerMove = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    dispatch({
      type: 'POINTER_MOVE',
      point: { x: event.clientX, y: event.clientY },
    });
  }, []);

  const handlePointerUp = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    dispatch({
      type: 'POINTER_UP',
      point: { x: event.clientX, y: event.clientY },
    });
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        dispatch({ type: 'CANCEL' });
        invoke('cancel_selection').catch(() => {});
      } else if (event.key === 'Enter' && state.mode === 'fixed_size') {
        event.preventDefault();
        dispatch({ type: 'CONFIRM' });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [state.mode]);

  useEffect(() => {
    if (state.status === 'completed' && state.lastCompletedRegion) {
      const physicalRect = logicalToPhysical(
        state.lastCompletedRegion,
        state.displayBounds
      );
      invoke('complete_selection', { rect: physicalRect }).catch((error) => {
        console.error('Failed to complete selection:', error);
      });
    }
  }, [state.status, state.lastCompletedRegion, state.displayBounds]);

  const currentRect = state.currentRect;

  return (
    <div
      className={`overlay-container overlay-container--${state.mode}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      data-testid="overlay-container"
    >
      <div className="overlay-instruction">
        {state.mode === 'fixed_size'
          ? '드래그하여 이동 · 모서리/테두리로 크기 조절 · Enter 캡처 · Esc 취소'
          : '마우스를 드래그하여 영역을 선택하세요 · Esc 취소'}
      </div>

      {currentRect && (
        <div
          className={`selection-cutout ${
            state.mode === 'fixed_size' ? 'selection-cutout--fixed' : ''
          }`}
          style={{
            left: `${currentRect.x}px`,
            top: `${currentRect.y}px`,
            width: `${currentRect.width}px`,
            height: `${currentRect.height}px`,
          }}
          data-testid="selection-cutout"
        >
          {state.mode === 'fixed_size' && (
            <>
              <div
                className="fixed-move-surface"
                data-selection-target="rect"
                aria-label="캡처 영역 이동"
              />
              {RESIZE_HANDLES.map(({ handle, label }) => (
                <button
                  key={handle}
                  type="button"
                  className={`resize-handle resize-handle--${handle}`}
                  data-handle={handle}
                  aria-label={label}
                />
              ))}
            </>
          )}
          <div className="dimension-badge" data-testid="dimension-badge">
            {Math.max(
              1,
              Math.round(currentRect.width * state.displayBounds.scaleFactor)
            )}{' '}
            ×{' '}
            {Math.max(
              1,
              Math.round(currentRect.height * state.displayBounds.scaleFactor)
            )}
          </div>
        </div>
      )}
    </div>
  );
}
