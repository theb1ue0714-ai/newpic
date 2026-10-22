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
export type ResizeHandle = 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w' | 'nw';

/** Global physical display bounds plus the display's physical-to-logical scale. */
export interface DisplayBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
}

export type FixedInteraction =
  | { kind: 'move'; pointerStart: Point; rectStart: Rect }
  | {
      kind: 'resize';
      handle: ResizeHandle;
      pointerStart: Point;
      rectStart: Rect;
    };

export interface SelectionState {
  ready: boolean;
  activeDisplay: boolean;
  mode: SelectionMode;
  status: 'idle' | 'selecting' | 'completed' | 'cancelled';
  startPoint: Point | null;
  currentPoint: Point | null;
  fixedSize: Size;
  displayBounds: DisplayBounds;
  currentRect: Rect | null;
  lastCompletedRegion: Rect | null;
  interaction: FixedInteraction | null;
}

export interface InitialSelectionOptions {
  mode: SelectionMode;
  displayBounds: DisplayBounds;
  fixedSize?: Size;
  lastCompletedRegion?: Rect | null;
}

export type SelectionAction =
  | {
      type: 'LOAD_OVERLAY';
      mode: SelectionMode;
      displayBounds: DisplayBounds;
      activeDisplay: boolean;
      rect?: Rect;
    }
  | { type: 'INITIALIZE_FIXED'; rect: Rect }
  | {
      type: 'POINTER_DOWN';
      point: Point;
      target?: 'rect' | ResizeHandle;
    }
  | { type: 'POINTER_MOVE'; point: Point }
  | { type: 'POINTER_UP'; point?: Point }
  | { type: 'POINTER_CANCEL' }
  | { type: 'CONFIRM' }
  | { type: 'CANCEL' }
  | { type: 'RESET' };

const DEFAULT_FIXED_PHYSICAL_SIZE: Size = { width: 800, height: 600 };

export function initialSelectionState(options: InitialSelectionOptions): SelectionState {
  const fixedSize = options.fixedSize ?? DEFAULT_FIXED_PHYSICAL_SIZE;
  return {
    ready: false,
    activeDisplay: false,
    mode: options.mode,
    status: 'idle',
    startPoint: null,
    currentPoint: null,
    fixedSize,
    displayBounds: options.displayBounds,
    currentRect: null,
    lastCompletedRegion: options.lastCompletedRegion ?? null,
    interaction: null,
  };
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(value, maximum));
}

function localDisplaySize(displayBounds: DisplayBounds): Size {
  const scaleFactor = displayBounds.scaleFactor || 1;
  return {
    width: displayBounds.width / scaleFactor,
    height: displayBounds.height / scaleFactor,
  };
}

function physicalSizeToLogical(size: Size, scaleFactor: number): Size {
  const scale = scaleFactor || 1;
  return {
    width: size.width / scale,
    height: size.height / scale,
  };
}

function clampFixedRect(rect: Rect, displayBounds: DisplayBounds): Rect {
  const bounds = localDisplaySize(displayBounds);
  const width = clamp(rect.width, 1, bounds.width);
  const height = clamp(rect.height, 1, bounds.height);
  return {
    x: clamp(rect.x, 0, bounds.width - width),
    y: clamp(rect.y, 0, bounds.height - height),
    width,
    height,
  };
}

/** Centers a local-logical rectangle on a display and clamps it to that display. */
export function calculateFixedRect(
  pointer: Point,
  fixedSize: Size,
  displayBounds: DisplayBounds
): Rect {
  return clampFixedRect(
    {
      x: Math.round(pointer.x - fixedSize.width / 2),
      y: Math.round(pointer.y - fixedSize.height / 2),
      width: fixedSize.width,
      height: fixedSize.height,
    },
    displayBounds
  );
}

function centeredDefaultFixedRect(displayBounds: DisplayBounds): Rect {
  const bounds = localDisplaySize(displayBounds);
  const fixedSize = physicalSizeToLogical(
    DEFAULT_FIXED_PHYSICAL_SIZE,
    displayBounds.scaleFactor
  );
  return calculateFixedRect(
    { x: bounds.width / 2, y: bounds.height / 2 },
    fixedSize,
    displayBounds
  );
}

function moveFixedRect(
  interaction: Extract<FixedInteraction, { kind: 'move' }>,
  point: Point,
  displayBounds: DisplayBounds
): Rect {
  const dx = point.x - interaction.pointerStart.x;
  const dy = point.y - interaction.pointerStart.y;
  return clampFixedRect(
    {
      ...interaction.rectStart,
      x: interaction.rectStart.x + dx,
      y: interaction.rectStart.y + dy,
    },
    displayBounds
  );
}

function resizeFixedRect(
  interaction: Extract<FixedInteraction, { kind: 'resize' }>,
  point: Point,
  displayBounds: DisplayBounds
): Rect {
  const { handle, pointerStart, rectStart } = interaction;
  const bounds = localDisplaySize(displayBounds);
  const dx = point.x - pointerStart.x;
  const dy = point.y - pointerStart.y;

  let left = rectStart.x;
  let top = rectStart.y;
  let right = rectStart.x + rectStart.width;
  let bottom = rectStart.y + rectStart.height;

  if (handle.includes('w')) {
    left = clamp(rectStart.x + dx, 0, right - 1);
  }
  if (handle.includes('e')) {
    right = clamp(rectStart.x + rectStart.width + dx, left + 1, bounds.width);
  }
  if (handle.includes('n')) {
    top = clamp(rectStart.y + dy, 0, bottom - 1);
  }
  if (handle.includes('s')) {
    bottom = clamp(rectStart.y + rectStart.height + dy, top + 1, bounds.height);
  }

  return {
    x: left,
    y: top,
    width: right - left,
    height: bottom - top,
  };
}

function updateFixedInteraction(
  state: SelectionState,
  point: Point
): SelectionState {
  if (!state.interaction) {
    return state;
  }

  const currentRect =
    state.interaction.kind === 'move'
      ? moveFixedRect(state.interaction, point, state.displayBounds)
      : resizeFixedRect(state.interaction, point, state.displayBounds);

  return {
    ...state,
    status: 'selecting',
    currentPoint: point,
    currentRect,
  };
}

export function selectionReducer(
  state: SelectionState,
  action: SelectionAction
): SelectionState {
  if (action.type !== 'LOAD_OVERLAY' && action.type !== 'RESET' && !state.ready) {
    return state;
  }

  switch (action.type) {
    case 'LOAD_OVERLAY': {
      const currentRect =
        action.mode === 'fixed_size' && action.activeDisplay
          ? action.rect
            ? clampFixedRect(action.rect, action.displayBounds)
            : centeredDefaultFixedRect(action.displayBounds)
          : null;
      return {
        ...state,
        ready: true,
        activeDisplay: action.mode === 'free_region' || action.activeDisplay,
        mode: action.mode,
        status: 'idle',
        startPoint: null,
        currentPoint: null,
        displayBounds: action.displayBounds,
        currentRect,
        interaction: null,
      };
    }

    case 'INITIALIZE_FIXED': {
      if (state.mode !== 'fixed_size' || !state.activeDisplay) {
        return state;
      }
      return {
        ...state,
        status: 'idle',
        currentRect: clampFixedRect(action.rect, state.displayBounds),
        interaction: null,
      };
    }

    case 'POINTER_DOWN': {
      if (state.mode === 'fixed_size') {
        if (!state.activeDisplay || !state.currentRect || !action.target) {
          return state;
        }
        const interaction: FixedInteraction =
          action.target === 'rect'
            ? {
                kind: 'move',
                pointerStart: action.point,
                rectStart: state.currentRect,
              }
            : {
                kind: 'resize',
                handle: action.target,
                pointerStart: action.point,
                rectStart: state.currentRect,
              };
        return {
          ...state,
          status: 'selecting',
          currentPoint: action.point,
          interaction,
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
        return updateFixedInteraction(state, action.point);
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
        if (!state.interaction) {
          return state;
        }
        const movedState = action.point
          ? updateFixedInteraction(state, action.point)
          : state;
        return {
          ...movedState,
          status: 'idle',
          interaction: null,
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

    case 'POINTER_CANCEL': {
      if (!state.interaction) {
        return state;
      }
      return {
        ...state,
        status: 'idle',
        interaction: null,
        currentPoint: null,
      };
    }

    case 'CONFIRM': {
      if (
        state.mode !== 'fixed_size' ||
        !state.activeDisplay ||
        !state.currentRect
      ) {
        return state;
      }
      return {
        ...state,
        status: 'completed',
        interaction: null,
        lastCompletedRegion: state.currentRect,
      };
    }

    case 'CANCEL': {
      return {
        ...state,
        status: 'cancelled',
        currentRect: null,
        startPoint: null,
        currentPoint: null,
        interaction: null,
      };
    }

    case 'RESET': {
      return {
        ...state,
        ready: false,
        activeDisplay: false,
        status: 'idle',
        currentRect: null,
        startPoint: null,
        currentPoint: null,
        interaction: null,
      };
    }

    default:
      return state;
  }
}

/** Converts display-local logical geometry to global physical geometry. */
export function logicalToPhysical(rect: Rect, displayBounds: DisplayBounds): Rect {
  const scaleFactor = displayBounds.scaleFactor || 1;
  return {
    x: displayBounds.x + Math.round(rect.x * scaleFactor),
    y: displayBounds.y + Math.round(rect.y * scaleFactor),
    width: Math.max(1, Math.round(rect.width * scaleFactor)),
    height: Math.max(1, Math.round(rect.height * scaleFactor)),
  };
}

/** Converts global physical geometry to coordinates local to an overlay window. */
export function physicalToLogical(rect: Rect, displayBounds: DisplayBounds): Rect {
  const scaleFactor = displayBounds.scaleFactor || 1;
  return {
    x: (rect.x - displayBounds.x) / scaleFactor,
    y: (rect.y - displayBounds.y) / scaleFactor,
    width: Math.max(1 / scaleFactor, rect.width / scaleFactor),
    height: Math.max(1 / scaleFactor, rect.height / scaleFactor),
  };
}

export function physicalSizeForDisplay(size: Size, displayBounds: DisplayBounds): Size {
  return physicalSizeToLogical(size, displayBounds.scaleFactor);
}

export function localLogicalDisplaySize(displayBounds: DisplayBounds): Size {
  return localDisplaySize(displayBounds);
}
