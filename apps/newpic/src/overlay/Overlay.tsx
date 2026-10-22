import React, { useEffect, useReducer, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  initialSelectionState,
  selectionReducer,
  logicalToPhysical,
  type Point,
  type DisplayBounds,
  type Rect,
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
        scaleFactor: typeof window !== 'undefined' ? window.devicePixelRatio : 1.0,
      },
    },
    initialSelectionState
  );

  useEffect(() => {
    let mounted = true;
    async function initOverlay() {
      try {
        const info = await invoke<OverlayInfo>('get_overlay_info');
        if (info && mounted) {
          const displayBounds: DisplayBounds = {
            x: info.display_bounds.x,
            y: info.display_bounds.y,
            width: info.display_bounds.width,
            height: info.display_bounds.height,
            scaleFactor: info.scale_factor || 1.0,
          };
          // Set initial mode & fixed dimensions
          if (info.request.mode === 'fixed_size') {
            const fixedSize = info.request.fixed_size ?? { width: 800, height: 600 };
            dispatch({
              type: 'POINTER_MOVE',
              point: {
                x: Math.round(displayBounds.width / 2),
                y: Math.round(displayBounds.height / 2),
              },
            });
          }
        }
      } catch {
        // Ignored when running outside Tauri or in headless tests
      }
    }
    initOverlay();
    return () => {
      mounted = false;
    };
  }, []);

  const handlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    dispatch({
      type: 'POINTER_DOWN',
      point: { x: e.clientX, y: e.clientY },
    });
  }, []);

  const handlePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    dispatch({
      type: 'POINTER_MOVE',
      point: { x: e.clientX, y: e.clientY },
    });
  }, []);

  const handlePointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    dispatch({
      type: 'POINTER_UP',
      point: { x: e.clientX, y: e.clientY },
    });
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        dispatch({ type: 'CANCEL' });
        invoke('cancel_selection').catch(() => {});
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  useEffect(() => {
    if (state.status === 'completed' && state.lastCompletedRegion) {
      const physicalRect = logicalToPhysical(
        state.lastCompletedRegion,
        state.displayBounds.scaleFactor
      );
      invoke('complete_selection', { rect: physicalRect }).catch((err) => {
        console.error('Failed to complete selection:', err);
      });
    }
  }, [state.status, state.lastCompletedRegion, state.displayBounds.scaleFactor]);

  const currentRect = state.currentRect;

  return (
    <div
      className="overlay-container"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      data-testid="overlay-container"
    >
      <div className="overlay-instruction">
        {state.mode === 'fixed_size' ? (
          <>마우스를 이동하여 위치를 잡고 클릭하세요 · <kbd>Esc</kbd> 취소</>
        ) : (
          <>마우스를 드래그하여 영역을 선택하세요 · <kbd>Esc</kbd> 취소</>
        )}
      </div>

      {currentRect && (
        <div
          className="selection-cutout"
          style={{
            left: `${currentRect.x}px`,
            top: `${currentRect.y}px`,
            width: `${currentRect.width}px`,
            height: `${currentRect.height}px`,
          }}
          data-testid="selection-cutout"
        >
          <div className="dimension-badge" data-testid="dimension-badge">
            {currentRect.width} × {currentRect.height}
          </div>
        </div>
      )}
    </div>
  );
}
