import { describe, expect, it } from 'vitest';
import {
  initialSelectionState,
  selectionReducer,
  logicalToPhysical,
  physicalToLogical,
  type DisplayBounds,
  type Rect,
  type ResizeHandle,
} from './selection';

describe('selection reducer and coordinate helpers', () => {
  const defaultDisplay: DisplayBounds = {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
    scaleFactor: 1,
  };

  function loadFixed(rect?: Rect, displayBounds = defaultDisplay) {
    return selectionReducer(
      initialSelectionState({ mode: 'free_region', displayBounds: defaultDisplay }),
      {
        type: 'LOAD_OVERLAY',
        mode: 'fixed_size',
        displayBounds,
        activeDisplay: true,
        rect,
      }
    );
  }

  function resize(handle: ResizeHandle, point: { x: number; y: number }) {
    const handleStarts: Record<ResizeHandle, { x: number; y: number }> = {
      n: { x: 200, y: 100 },
      ne: { x: 300, y: 100 },
      e: { x: 300, y: 150 },
      se: { x: 300, y: 200 },
      s: { x: 200, y: 200 },
      sw: { x: 100, y: 200 },
      w: { x: 100, y: 150 },
      nw: { x: 100, y: 100 },
    };
    let state = loadFixed({ x: 100, y: 100, width: 200, height: 100 });
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: handleStarts[handle],
      target: handle,
    });
    return selectionReducer(state, { type: 'POINTER_MOVE', point });
  }

  it('loads F9 from a supplied display-local rectangle', () => {
    const state = loadFixed({ x: 120, y: 80, width: 640, height: 480 });

    expect(state.mode).toBe('fixed_size');
    expect(state.status).toBe('idle');
    expect(state.currentRect).toEqual({ x: 120, y: 80, width: 640, height: 480 });
    expect(state.interaction).toBeNull();
    expect(state.ready).toBe(true);
    expect(state.activeDisplay).toBe(true);
  });

  it('ignores input until overlay metadata has loaded', () => {
    const initial = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
    });
    const pressed = selectionReducer(initial, {
      type: 'POINTER_DOWN',
      point: { x: 100, y: 100 },
    });
    const confirmed = selectionReducer(pressed, { type: 'CONFIRM' });

    expect(initial.ready).toBe(false);
    expect(pressed).toBe(initial);
    expect(confirmed).toBe(initial);
  });

  it('keeps pre-load Escape cancellation terminal when metadata arrives late', () => {
    const initial = initialSelectionState({
      mode: 'free_region',
      displayBounds: defaultDisplay,
    });
    const cancelled = selectionReducer(initial, { type: 'CANCEL' });
    const lateLoad = selectionReducer(cancelled, {
      type: 'LOAD_OVERLAY',
      mode: 'fixed_size',
      displayBounds: defaultDisplay,
      activeDisplay: true,
      rect: { x: 100, y: 100, width: 800, height: 600 },
    });

    expect(cancelled.status).toBe('cancelled');
    expect(lateLoad).toBe(cancelled);
    expect(lateLoad.currentRect).toBeNull();
  });

  it('does not initialize or confirm F9 on an inactive display', () => {
    let state = selectionReducer(
      initialSelectionState({ mode: 'free_region', displayBounds: defaultDisplay }),
      {
        type: 'LOAD_OVERLAY',
        mode: 'fixed_size',
        displayBounds: defaultDisplay,
        activeDisplay: false,
        rect: { x: 100, y: 100, width: 800, height: 600 },
      }
    );

    expect(state.ready).toBe(true);
    expect(state.activeDisplay).toBe(false);
    expect(state.currentRect).toBeNull();
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 200, y: 200 },
      target: 'rect',
    });
    state = selectionReducer(state, { type: 'CONFIRM' });
    expect(state.status).toBe('idle');
    expect(state.lastCompletedRegion).toBeNull();
  });

  it('centers and clamps the default 800x600 F9 rectangle when none is supplied', () => {
    const state = loadFixed();

    expect(state.currentRect).toEqual({ x: 560, y: 240, width: 800, height: 600 });

    const smallDisplay: DisplayBounds = {
      x: -1280,
      y: 0,
      width: 640,
      height: 480,
      scaleFactor: 1,
    };
    expect(loadFixed(undefined, smallDisplay).currentRect).toEqual({
      x: 0,
      y: 0,
      width: 640,
      height: 480,
    });
  });

  it('does not move F9 for hover movement or background clicks', () => {
    const initial = loadFixed({ x: 100, y: 120, width: 800, height: 600 });
    const hovered = selectionReducer(initial, {
      type: 'POINTER_MOVE',
      point: { x: 1500, y: 800 },
    });
    const backgroundPressed = selectionReducer(hovered, {
      type: 'POINTER_DOWN',
      point: { x: 20, y: 20 },
    });

    expect(hovered.currentRect).toEqual(initial.currentRect);
    expect(backgroundPressed.currentRect).toEqual(initial.currentRect);
    expect(backgroundPressed.interaction).toBeNull();
  });

  it('moves F9 while preserving the initial pointer offset', () => {
    let state = loadFixed({ x: 100, y: 120, width: 800, height: 600 });
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 175, y: 200 },
      target: 'rect',
    });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 475, y: 400 },
    });

    expect(state.currentRect).toEqual({ x: 400, y: 320, width: 800, height: 600 });
    expect(state.status).toBe('selecting');
  });

  it('moves and resizes F9 from explicit keyboard adjustments', () => {
    let state = loadFixed({ x: 100, y: 100, width: 200, height: 100 });
    state = selectionReducer(state, {
      type: 'KEYBOARD_ADJUST',
      target: 'rect',
      dx: -150,
      dy: 10,
    });
    expect(state.currentRect).toEqual({ x: 0, y: 110, width: 200, height: 100 });

    state = selectionReducer(state, {
      type: 'KEYBOARD_ADJUST',
      target: 'e',
      dx: 10,
      dy: 0,
    });
    expect(state.currentRect).toEqual({ x: 0, y: 110, width: 210, height: 100 });

    state = selectionReducer(state, {
      type: 'KEYBOARD_ADJUST',
      target: 'nw',
      dx: 10,
      dy: 10,
    });
    expect(state.currentRect).toEqual({ x: 10, y: 120, width: 200, height: 90 });
  });

  it.each<[ResizeHandle, { x: number; y: number }, Rect]>([
    ['n', { x: 100, y: 80 }, { x: 100, y: 80, width: 200, height: 120 }],
    ['ne', { x: 330, y: 80 }, { x: 100, y: 80, width: 230, height: 120 }],
    ['e', { x: 330, y: 100 }, { x: 100, y: 100, width: 230, height: 100 }],
    ['se', { x: 330, y: 230 }, { x: 100, y: 100, width: 230, height: 130 }],
    ['s', { x: 100, y: 230 }, { x: 100, y: 100, width: 200, height: 130 }],
    ['sw', { x: 80, y: 230 }, { x: 80, y: 100, width: 220, height: 130 }],
    ['w', { x: 80, y: 100 }, { x: 80, y: 100, width: 220, height: 100 }],
    ['nw', { x: 80, y: 80 }, { x: 80, y: 80, width: 220, height: 120 }],
  ])('resizes F9 from the %s handle', (handle, point, expected) => {
    expect(resize(handle, point).currentRect).toEqual(expected);
  });

  it('stops reverse-direction resizing at 1x1 instead of flipping handles', () => {
    expect(resize('nw', { x: 500, y: 500 }).currentRect).toEqual({
      x: 299,
      y: 199,
      width: 1,
      height: 1,
    });
    expect(resize('se', { x: -500, y: -500 }).currentRect).toEqual({
      x: 100,
      y: 100,
      width: 1,
      height: 1,
    });
  });

  it('clamps moving and resizing to display-local logical bounds', () => {
    let moving = loadFixed({ x: 100, y: 100, width: 800, height: 600 });
    moving = selectionReducer(moving, {
      type: 'POINTER_DOWN',
      point: { x: 200, y: 200 },
      target: 'rect',
    });
    moving = selectionReducer(moving, {
      type: 'POINTER_MOVE',
      point: { x: 3000, y: 2000 },
    });
    expect(moving.currentRect).toEqual({ x: 1120, y: 480, width: 800, height: 600 });

    let resizing = loadFixed({ x: 100, y: 100, width: 200, height: 100 });
    resizing = selectionReducer(resizing, {
      type: 'POINTER_DOWN',
      point: { x: 100, y: 100 },
      target: 'nw',
    });
    resizing = selectionReducer(resizing, {
      type: 'POINTER_MOVE',
      point: { x: -500, y: -500 },
    });
    expect(resizing.currentRect).toEqual({ x: 0, y: 0, width: 300, height: 200 });
  });

  it('ends F9 interaction on pointer-up without completing capture', () => {
    let state = loadFixed({ x: 100, y: 100, width: 800, height: 600 });
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 150, y: 150 },
      target: 'rect',
    });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 250, y: 250 },
    });
    state = selectionReducer(state, { type: 'POINTER_UP' });

    expect(state.status).toBe('idle');
    expect(state.interaction).toBeNull();
    expect(state.lastCompletedRegion).toBeNull();
    expect(state.currentRect).toEqual({ x: 200, y: 200, width: 800, height: 600 });
  });

  it('ends F9 interaction on pointer cancellation without changing the rectangle', () => {
    let state = loadFixed({ x: 100, y: 100, width: 800, height: 600 });
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 150, y: 150 },
      target: 'rect',
    });
    state = selectionReducer(state, { type: 'POINTER_CANCEL' });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 500, y: 500 },
    });

    expect(state.status).toBe('idle');
    expect(state.interaction).toBeNull();
    expect(state.currentRect).toEqual({ x: 100, y: 100, width: 800, height: 600 });
  });

  it('completes F9 only through CONFIRM when a valid rectangle exists', () => {
    const state = selectionReducer(
      loadFixed({ x: 100, y: 100, width: 800, height: 600 }),
      { type: 'CONFIRM' }
    );

    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({ x: 100, y: 100, width: 800, height: 600 });

    const missing = initialSelectionState({
      mode: 'fixed_size',
      displayBounds: defaultDisplay,
    });
    expect(selectionReducer(missing, { type: 'CONFIRM' }).status).toBe('idle');
  });

  it('cancels F9 without replacing an externally persisted rectangle', () => {
    const previouslyPersisted: Rect = { x: 50, y: 50, width: 300, height: 200 };
    let state = initialSelectionState({
      mode: 'fixed_size',
      displayBounds: defaultDisplay,
      lastCompletedRegion: previouslyPersisted,
    });
    state = selectionReducer(state, {
      type: 'LOAD_OVERLAY',
      mode: 'fixed_size',
      displayBounds: defaultDisplay,
      activeDisplay: true,
      rect: { x: 500, y: 400, width: 600, height: 500 },
    });
    state = selectionReducer(state, { type: 'CANCEL' });

    expect(state.status).toBe('cancelled');
    expect(state.currentRect).toBeNull();
    expect(state.lastCompletedRegion).toEqual(previouslyPersisted);
  });

  it('keeps F7 drag completion on pointer-up unchanged', () => {
    let state = selectionReducer(
      initialSelectionState({
        mode: 'free_region',
        displayBounds: defaultDisplay,
      }),
      {
        type: 'LOAD_OVERLAY',
        mode: 'free_region',
        displayBounds: defaultDisplay,
        activeDisplay: true,
      }
    );
    state = selectionReducer(state, {
      type: 'POINTER_DOWN',
      point: { x: 400, y: 300 },
    });
    state = selectionReducer(state, {
      type: 'POINTER_MOVE',
      point: { x: 150, y: 100 },
    });
    state = selectionReducer(state, {
      type: 'POINTER_UP',
      point: { x: 150, y: 100 },
    });

    expect(state.status).toBe('completed');
    expect(state.lastCompletedRegion).toEqual({ x: 150, y: 100, width: 250, height: 200 });
  });

  it('converts local logical rectangles through scale and global physical display origin', () => {
    const leftDisplay: DisplayBounds = {
      x: -2560,
      y: 180,
      width: 2560,
      height: 1440,
      scaleFactor: 2,
    };
    const localLogical: Rect = { x: 100, y: 150, width: 400, height: 300 };

    const physical = logicalToPhysical(localLogical, leftDisplay);
    expect(physical).toEqual({
      x: -2360,
      y: 480,
      width: 800,
      height: 600,
    });
    expect(physicalToLogical(physical, leftDisplay)).toEqual(localLogical);

    const fractionalDisplay: DisplayBounds = {
      x: -1920,
      y: -1080,
      width: 2880,
      height: 1620,
      scaleFactor: 1.5,
    };
    const nonDivisiblePhysical: Rect = {
      x: -1819,
      y: -979,
      width: 801,
      height: 601,
    };
    expect(
      logicalToPhysical(
        physicalToLogical(nonDivisiblePhysical, fractionalDisplay),
        fractionalDisplay
      )
    ).toEqual(nonDivisiblePhysical);
  });
});
