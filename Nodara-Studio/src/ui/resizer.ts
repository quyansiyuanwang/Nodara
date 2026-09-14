export type ResizeAxis = "x" | "y";

export interface ResizerOptions {
  axis: ResizeAxis;
  value: number;
  min: number;
  max: () => number;
  onChange: (value: number) => void;
  /** Drag up/left increases the value instead of decreasing it. */
  invert?: boolean;
}

/** Install a pointer/keyboard-accessible splitter for a CSS-sized pane. */
export function installResizer(handle: HTMLElement, options: ResizerOptions): void {
  let start = 0;
  let current = options.value;
  let startValue = current;

  const clamp = (value: number): number =>
    Math.round(Math.min(options.max(), Math.max(options.min, value)));

  const valueFromEvent = (event: PointerEvent): number => {
    const raw = options.axis === "x" ? event.clientX - start : event.clientY - start;
    return clamp(startValue + (options.invert ? -raw : raw));
  };

  const apply = (value: number): void => {
    current = value;
    options.onChange(value);
  };

  const finish = (event: PointerEvent): void => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    handle.releasePointerCapture(event.pointerId);
    handle.classList.remove("resizer--active");
    document.body.classList.remove("is-resizing-x", "is-resizing-y");
  };

  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    start = options.axis === "x" ? event.clientX : event.clientY;
    startValue = current;
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("resizer--active");
    document.body.classList.add(options.axis === "x" ? "is-resizing-x" : "is-resizing-y");
  });

  handle.addEventListener("pointermove", (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    apply(valueFromEvent(event));
  });

  handle.addEventListener("pointerup", finish);
  handle.addEventListener("pointercancel", finish);

  handle.addEventListener("dblclick", () => apply(options.value));
  handle.addEventListener("keydown", (event) => {
    const delta = event.key === "ArrowRight" || event.key === "ArrowDown" ? 16
      : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -16
      : 0;
    if (delta === 0) return;
    event.preventDefault();
    apply(clamp(current + (options.invert ? -delta : delta)));
  });
}
