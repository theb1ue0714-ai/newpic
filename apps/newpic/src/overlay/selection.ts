export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type SelectionMode = 'free_region' | 'fixed_size';

export interface DisplayBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
}

export interface SelectionState {
  mode: SelectionMode;
  status: 'idle' | 'selecting' | 'completed' | 'cancelled';
  startPoint: Point | null;
  currentPoint: Point | null;
  fixedSize: Size;
  displayBounds: DisplayBounds;
  currentRect: Rect | null;
  lastCompletedRegion: Rect | null;
}

export interface InitialSelectionOptions {
  mode: SelectionMode;
  displayBounds: DisplayBounds;
  fixedSize?: Size;
  lastCompletedRegion?: Rect | null;
}

export type SelectionAction =
  | { type: 'POINTER_DOWN'; point: Point }
  | { type: 'POINTER_MOVE'; point: Point }
  | { type: 'POINTER_UP'; point?: Point }
  | { type: 'CANCEL' }
  | { type: 'RESET' };

export function initialSelectionState(options: InitialSelectionOptions): SelectionState {
  const fixedSize = options.fixedSize ?? { width: 800, height: 600 };
  return {
    mode: options.mode,
    status: 'idle',
    startPoint: null,
    currentPoint: null,
    fixedSize,
    displayBounds: options.displayBounds,
    currentRect: null,
    lastCompletedRegion: options.lastCompletedRegion ?? null,
  };
}

export function calculateFixedRect(
  pointer: Point,
  fixedSize: Size,
  displayBounds: DisplayBounds
): Rect {
  const width = Math.min(fixedSize.width, displayBounds.width);
  const height = Math.min(fixedSize.height, displayBounds.height);

  const centeredX = Math.round(pointer.x - width / 2);
  const centeredY = Math.round(pointer.y - height / 2);

  const minX = displayBounds.x;
  const maxX = displayBounds.x + displayBounds.width - width;
  const minY = displayBounds.y;
  const maxY = displayBounds.y + displayBounds.height - height;

  const x = Math.max(minX, Math.min(centeredX, maxX));
  const y = Math.max(minY, Math.min(centeredY, maxY));

  return { x, y, width, height };
}

export function selectionReducer(
  state: SelectionState,
  action: SelectionAction
): SelectionState {
  switch (action.type) {
    case 'POINTER_DOWN': {
      if (state.mode === 'fixed_size') {
        const rect =
          state.currentRect ??
          calculateFixedRect(action.point, state.fixedSize, state.displayBounds);
        return {
          ...state,
          status: 'completed',
          currentPoint: action.point,
          currentRect: rect,
          lastCompletedRegion: rect,
        };
      }

      const rect: Rect = {
        x: action.point.x,
        y: action.point.y,
        width: 1,
        height: 1,
      };

      return {
        ...state,
        status: 'selecting',
        startPoint: action.point,
        currentPoint: action.point,
        currentRect: rect,
      };
    }

    case 'POINTER_MOVE': {
      if (state.mode === 'fixed_size') {
        const rect = calculateFixedRect(
          action.point,
          state.fixedSize,
          state.displayBounds
        );
        return {
          ...state,
          currentPoint: action.point,
          currentRect: rect,
        };
      }

      if (state.status !== 'selecting' || !state.startPoint) {
        return state;
      }

      const minX = Math.min(state.startPoint.x, action.point.x);
      const minY = Math.min(state.startPoint.y, action.point.y);
      const width = Math.max(1, Math.abs(action.point.x - state.startPoint.x));
      const height = Math.max(1, Math.abs(action.point.y - state.startPoint.y));

      return {
        ...state,
        currentPoint: action.point,
        currentRect: { x: minX, y: minY, width, height },
      };
    }

    case 'POINTER_UP': {
      if (state.mode === 'fixed_size') {
        if (!state.currentRect) {
          return state;
        }
        return {
          ...state,
          status: 'completed',
          lastCompletedRegion: state.currentRect,
        };
      }

      if (state.status !== 'selecting') {
        return state;
      }

      let finalRect = state.currentRect;
      if (action.point && state.startPoint) {
        const minX = Math.min(state.startPoint.x, action.point.x);
        const minY = Math.min(state.startPoint.y, action.point.y);
        const width = Math.max(1, Math.abs(action.point.x - state.startPoint.x));
        const height = Math.max(1, Math.abs(action.point.y - state.startPoint.y));
        finalRect = { x: minX, y: minY, width, height };
      }

      if (!finalRect) {
        return state;
      }

      return {
        ...state,
        status: 'completed',
        currentRect: finalRect,
        lastCompletedRegion: finalRect,
      };
    }

    case 'CANCEL': {
      return {
        ...state,
        status: 'cancelled',
        currentRect: null,
        startPoint: null,
        currentPoint: null,
      };
    }

    case 'RESET': {
      return {
        ...state,
        status: 'idle',
        currentRect: null,
        startPoint: null,
        currentPoint: null,
      };
    }

    default:
      return state;
  }
}

export function logicalToPhysical(rect: Rect, scaleFactor: number): Rect {
  return {
    x: Math.round(rect.x * scaleFactor),
    y: Math.round(rect.y * scaleFactor),
    width: Math.max(1, Math.round(rect.width * scaleFactor)),
    height: Math.max(1, Math.round(rect.height * scaleFactor)),
  };
}

export function physicalToLogical(rect: Rect, scaleFactor: number): Rect {
  return {
    x: Math.round(rect.x / scaleFactor),
    y: Math.round(rect.y / scaleFactor),
    width: Math.max(1, Math.round(rect.width / scaleFactor)),
    height: Math.max(1, Math.round(rect.height / scaleFactor)),
  };
}
