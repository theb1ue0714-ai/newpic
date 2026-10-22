import { describe, it, expect } from 'vitest';
import {
  initialSelectionState,
  selectionReducer,
  logicalToPhysical,
  physicalToLogical,
  type Rect,
  type DisplayBounds,
} from './selection';

describe('selection reducer and coordinate helpers', () => {
  const defaultDisplay: DisplayBounds = {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
    scaleFactor: 1.0,
  };

  it('handles pointer down, move, and up for standard top-left to bottom-right drag', () => {
    let state = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
    });

    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 100, y: 150 },
    });
    expect(state.status).toBe('selecting');
    expect(state.startPoint).toEqual({ x: 100, y: 150 });
    expect(state.currentRect).toEqual({ x: 100, y: 150, width: 1, height: 1 });

    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 300, y: 250 },
    });
    expect(state.currentRect).toEqual({ x: 100, y: 150, width: 200, height: 100 });

    state = selectionReducer(state, {
      type: 'POINTER_UP',
      point: { x: 300, y: 250 },
    });
    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({ x: 100, y: 150, width: 200, height: 100 });
  });

  it('normalizes reverse-direction drags (bottom-right to top-left)', () => {
    let state = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
    });

    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 400, y: 300 },
    });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 150, y: 100 },
    });

    expect(state.currentRect).toEqual({
      x: 150,
      y: 100,
      width: 250,
      height: 200,
    });

    state = selectionReducer(state, {
      type: 'POINTER_UP',
    });
    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({
      x: 150,
      y: 100,
      width: 250,
      height: 200,
    });
  });

  it('enforces a minimum 1x1 selection even on single-click or 0-pixel delta', () => {
    let state = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
    });

    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 250, y: 250 },
    });
    state = selectionReducer(state, {
      type: 'POINTER_UP',
      point: { x: 250, y: 250 },
    });

    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({
      x: 250,
      y: 250,
      width: 1,
      height: 1,
    });
  });

  it('handles escape cancellation and preserves F8 last completed region', () => {
    const previousRegion: Rect = { x: 50, y: 50, width: 300, height: 200 };
    let state = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
      lastCompletedRegion: previousRegion,
    });

    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 500, y: 500 },
    });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 700, y: 800 },
    });
    expect(state.status).toBe('selecting');
    expect(state.currentRect).toEqual({ x: 500, y: 500, width: 200, height: 300 });

    state = selectionReducer(state, { type: 'CANCEL' });

    expect(state.status).toBe('cancelled');
    expect(state.currentRect).toBeNull();
    // Must preserve previous F8 region!
    expect(state.lastCompletedRegion).toEqual(previousRegion);
  });

  it('handles fixed 800x600 movement centered at pointer', () => {
    let state = initialSelectionState({
      mode: 'fixed_size',
      fixedSize: { width: 800, height: 600 },
      displayBounds: defaultDisplay,
    });

    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 960, y: 540 },
    });

    expect(state.currentRect).toEqual({
      x: 960 - 400, // 560
      y: 540 - 300, // 240
      width: 800,
      height: 600,
    });

    state = selectionReducer(state, {
      type: 'POINTER_UP',
    });
    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({
      x: 560,
      y: 240,
      width: 800,
      height: 600,
    });
  });

  it('clamps fixed selection to display edges', () => {
    let state = initialSelectionState({
      mode: 'fixed_size',
      fixedSize: { width: 800, height: 600 },
      displayBounds: defaultDisplay,
    });

    // Move to near top-left: should clamp to (0, 0)
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 100, y: 100 },
    });
    expect(state.currentRect).toEqual({
      x: 0,
      y: 0,
      width: 800,
      height: 600,
    });

    // Move to near bottom-right: should clamp to (1920 - 800, 1080 - 600) = (1120, 480)
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 1900, y: 1050 },
    });
    expect(state.currentRect).toEqual({
      x: 1120,
      y: 480,
      width: 800,
      height: 600,
    });
  });

  it('clamps fixed size if display is smaller than requested dimensions', () => {
    const smallDisplay: DisplayBounds = {
      x: 0,
      y: 0,
      width: 640,
      height: 480,
      scaleFactor: 1.0,
    };
    let state = initialSelectionState({
      mode: 'fixed_size',
      fixedSize: { width: 800, height: 600 },
      displayBounds: smallDisplay,
    });

    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 300, y: 200 },
    });
    expect(state.currentRect).toEqual({
      x: 0,
      y: 0,
      width: 640,
      height: 480,
    });
  });

  it('converts between logical and physical coordinates according to scaleFactor', () => {
    const logicalRect: Rect = { x: 100, y: 150, width: 400, height: 300 };
    const scaleFactor = 1.5;

    const physical = logicalToPhysical(logicalRect, scaleFactor);
    expect(physical).toEqual({
      x: 150,
      y: 225,
      width: 600,
      height: 450,
    });

    const restoredLogical = physicalToLogical(physical, scaleFactor);
    expect(restoredLogical).toEqual(logicalRect);
  });
});
