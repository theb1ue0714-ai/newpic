import React, { useCallback, useEffect, useReducer, useRef } from 'react';
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
  active_display: boolean;
  initial_rect?: Rect;
}

const RESIZE_HANDLES: ReadonlyArray<ResizeHandle> = [
  'n',
  'ne',
  'e',
  'se',
  's',
  'sw',
  'w',
  'nw',
];

const RESIZE_HANDLE_LABELS: Record<ResizeHandle, string> = {
  n: '위쪽 크기 조절',
  ne: '오른쪽 위 크기 조절',
  e: '오른쪽 크기 조절',
  se: '오른쪽 아래 크기 조절',
  s: '아래쪽 크기 조절',
  sw: '왼쪽 아래 크기 조절',
  w: '왼쪽 크기 조절',
  nw: '왼쪽 위 크기 조절',
};

const DIMENSION_BADGE_WIDTH = 96;
const DIMENSION_BADGE_HEIGHT = 24;
const DIMENSION_BADGE_GAP = 6;

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(value, maximum));
}

function dimensionBadgePosition(rect: Rect, displayBounds: DisplayBounds) {
  const display = localLogicalDisplaySize(displayBounds);
  const left = clamp(
    rect.x + rect.width - DIMENSION_BADGE_WIDTH,
    0,
    Math.max(0, display.width - DIMENSION_BADGE_WIDTH)
  );
  const below = rect.y + rect.height + DIMENSION_BADGE_GAP;
  const top =
    below + DIMENSION_BADGE_HEIGHT <= display.height
      ? below
      : Math.max(0, rect.y - DIMENSION_BADGE_HEIGHT - DIMENSION_BADGE_GAP);
  return { left, top };
}

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
  const containerRef = useRef<HTMLDivElement>(null);
  const activePointerId = useRef<number | null>(null);
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
          activeDisplay:
            info.request.mode === 'free_region' || info.active_display === true,
          rect:
            info.request.mode === 'fixed_size' && info.active_display === true
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

  const releaseActivePointer = useCallback(() => {
    const pointerId = activePointerId.current;
    const container = containerRef.current;
    if (
      pointerId !== null &&
      container &&
      container.hasPointerCapture(pointerId)
    ) {
      container.releasePointerCapture(pointerId);
    }
    activePointerId.current = null;
  }, []);

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (
        !state.ready ||
        !state.activeDisplay ||
        event.button !== 0
      ) {
        return;
      }
      const target = state.mode === 'fixed_size' ? pointerTarget(event.target) : undefined;
      if (state.mode === 'fixed_size' && !target) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      activePointerId.current = event.pointerId;
      dispatch({
        type: 'POINTER_DOWN',
        point: { x: event.clientX, y: event.clientY },
        target,
      });
    },
    [state.activeDisplay, state.mode, state.ready]
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!state.ready || !state.activeDisplay) {
        return;
      }
      dispatch({
        type: 'POINTER_MOVE',
        point: { x: event.clientX, y: event.clientY },
      });
    },
    [state.activeDisplay, state.ready]
  );

  const handlePointerUp = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (!state.ready || !state.activeDisplay || event.button !== 0) {
        return;
      }
      dispatch({
        type: 'POINTER_UP',
        point: { x: event.clientX, y: event.clientY },
      });
      releaseActivePointer();
    },
    [releaseActivePointer, state.activeDisplay, state.ready]
  );

  const handlePointerCancel = useCallback(() => {
    if (!state.ready) {
      return;
    }
    dispatch({ type: 'POINTER_CANCEL' });
    releaseActivePointer();
  }, [releaseActivePointer, state.ready]);

  const handleLostPointerCapture = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (activePointerId.current === event.pointerId) {
        activePointerId.current = null;
      }
      if (state.ready) {
        dispatch({ type: 'POINTER_CANCEL' });
      }
    },
    [state.ready]
  );

  const handleControlKeyDown = useCallback(
    (
      event: React.KeyboardEvent<HTMLElement>,
      target: 'rect' | ResizeHandle
    ) => {
      const step = event.shiftKey ? 10 : 1;
      const delta = {
        ArrowLeft: { dx: -step, dy: 0 },
        ArrowRight: { dx: step, dy: 0 },
        ArrowUp: { dx: 0, dy: -step },
        ArrowDown: { dx: 0, dy: step },
      }[event.key];
      if (!delta) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      dispatch({ type: 'KEYBOARD_ADJUST', target, ...delta });
    },
    []
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        releaseActivePointer();
        dispatch({ type: 'CANCEL' });
        invoke('cancel_selection').catch(() => {});
        return;
      }
      if (!state.ready || !state.activeDisplay) {
        return;
      }
      if (
        event.key === 'Enter' &&
        state.mode === 'fixed_size' &&
        state.activeDisplay
      ) {
        event.preventDefault();
        releaseActivePointer();
        dispatch({ type: 'CONFIRM' });
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [releaseActivePointer, state.activeDisplay, state.mode, state.ready]);

  useEffect(() => {
    if (
      state.ready &&
      state.activeDisplay &&
      (state.status === 'completed' || state.status === 'confirming') &&
      state.lastCompletedRegion
    ) {
      const physicalRect = logicalToPhysical(
        state.lastCompletedRegion,
        state.displayBounds
      );
      invoke('complete_selection', { rect: physicalRect }).catch((error) => {
        console.error('Failed to complete selection:', error);
        dispatch({ type: 'CONFIRM_FAILED' });
      });
    }
  }, [
    state.activeDisplay,
    state.displayBounds,
    state.lastCompletedRegion,
    state.ready,
    state.status,
  ]);

  const currentRect = state.currentRect;
  const badgePosition = currentRect
    ? dimensionBadgePosition(currentRect, state.displayBounds)
    : null;

  return (
    <div
      ref={containerRef}
      className={`overlay-container overlay-container--${state.mode}`}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerCancel}
      onLostPointerCapture={handleLostPointerCapture}
      data-testid="overlay-container"
      data-ready={state.ready}
      data-active-display={state.activeDisplay}
    >
      {!state.ready ? (
        <div className="overlay-instruction">오버레이 준비 중</div>
      ) : state.activeDisplay ? (
        <div className="overlay-instruction">
          {state.mode === 'fixed_size'
            ? '드래그하여 이동 · 모서리/테두리로 크기 조절 · Enter 캡처 · Esc 취소'
            : '마우스를 드래그하여 영역을 선택하세요 · Esc 취소'}
        </div>
      ) : null}

      {currentRect && badgePosition && (
        <>
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
                  role="button"
                  tabIndex={0}
                  aria-label="캡처 영역 이동"
                  onKeyDown={(event) => handleControlKeyDown(event, 'rect')}
                />
                {RESIZE_HANDLES.map((handle) => (
                  <div
                    key={handle}
                    className={`resize-handle resize-handle--${handle}`}
                    data-handle={handle}
                    role="button"
                    tabIndex={0}
                    aria-label={RESIZE_HANDLE_LABELS[handle]}
                    onKeyDown={(event) => handleControlKeyDown(event, handle)}
                  />
                ))}
              </>
            )}
          </div>
          <div
            className="dimension-badge"
            data-testid="dimension-badge"
            style={{
              left: `${badgePosition.left}px`,
              top: `${badgePosition.top}px`,
            }}
          >
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
        </>
      )}
    </div>
  );
}
