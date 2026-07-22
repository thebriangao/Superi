export type KeyboardInputDisposition =
  | "route"
  | "handled"
  | "composing"
  | "repeated"
  | "editable";

export interface ShellKeyboardEvent {
  readonly defaultPrevented?: boolean;
  readonly isComposing?: boolean;
  readonly repeat?: boolean;
}

export function keyboardInputDisposition(
  event: ShellKeyboardEvent,
  editableTarget: boolean,
  allowInEditableContext: boolean,
): KeyboardInputDisposition {
  if (event.defaultPrevented) return "handled";
  if (event.isComposing) return "composing";
  if (event.repeat) return "repeated";
  if (editableTarget && !allowInEditableContext) return "editable";
  return "route";
}

export interface ShellFocusTarget {
  readonly isConnected: boolean;
  readonly hidden?: boolean;
  getAttribute(name: string): string | null;
  focus(options?: FocusOptions): void;
}

export type ShellFocusResult = "preferred" | "fallback" | "unavailable";

export function restoreShellFocus(
  preferred: ShellFocusTarget | null,
  fallback: ShellFocusTarget | null = null,
): ShellFocusResult {
  if (focusTarget(preferred)) return "preferred";
  if (fallback !== preferred && focusTarget(fallback)) return "fallback";
  return "unavailable";
}

function focusTarget(target: ShellFocusTarget | null): boolean {
  if (
    target === null ||
    !target.isConnected ||
    target.hidden === true ||
    target.getAttribute("aria-hidden") === "true" ||
    target.getAttribute("disabled") !== null ||
    target.getAttribute("inert") !== null
  ) {
    return false;
  }
  try {
    target.focus({ preventScroll: true });
    return true;
  } catch {
    return false;
  }
}

export interface PointerEventSampleSource {
  readonly pointerId: number;
  readonly pointerType?: string;
  readonly isPrimary?: boolean;
  readonly button?: number;
  readonly buttons?: number;
  readonly clientX: number;
  readonly clientY: number;
  readonly pressure?: number;
  readonly tangentialPressure?: number;
  readonly tiltX?: number;
  readonly tiltY?: number;
  readonly twist?: number;
  readonly width?: number;
  readonly height?: number;
  readonly timeStamp?: number;
  getCoalescedEvents?(): readonly PointerEventSampleSource[];
}

export interface ShellPointerSample {
  readonly pointerId: number;
  readonly pointerType: "mouse" | "pen" | "touch" | "unknown";
  readonly isPrimary: boolean;
  readonly button: number;
  readonly buttons: number;
  readonly clientX: number;
  readonly clientY: number;
  readonly pressure: number;
  readonly tangentialPressure: number;
  readonly tiltX: number;
  readonly tiltY: number;
  readonly twist: number;
  readonly width: number;
  readonly height: number;
  readonly timeStamp: number;
}

export function normalizePointerSamples(
  event: PointerEventSampleSource,
): readonly ShellPointerSample[] {
  let coalesced: readonly PointerEventSampleSource[] = [];
  try {
    coalesced = event.getCoalescedEvents?.() ?? [];
  } catch {
    coalesced = [];
  }
  const sources = coalesced.filter(
    (sample) => sample.pointerId === event.pointerId,
  );
  const last = sources.at(-1);
  const includeCurrent =
    last === undefined ||
    last.clientX !== event.clientX ||
    last.clientY !== event.clientY ||
    last.timeStamp !== event.timeStamp;
  const samples = (includeCurrent ? [...sources, event] : sources).map(
    normalizePointerSample,
  );
  if (samples.length === 0) samples.push(normalizePointerSample(event));
  return Object.freeze(samples);
}

export function latestPointerSample(
  event: PointerEventSampleSource,
): ShellPointerSample {
  return normalizePointerSamples(event).at(-1)!;
}

function normalizePointerSample(
  sample: PointerEventSampleSource,
): ShellPointerSample {
  const pointerType = ["mouse", "pen", "touch"].includes(
    sample.pointerType ?? "",
  )
    ? (sample.pointerType as "mouse" | "pen" | "touch")
    : "unknown";
  return Object.freeze({
    pointerId: integer(sample.pointerId, 0),
    pointerType,
    isPrimary: sample.isPrimary ?? true,
    button: integer(sample.button ?? 0, 0),
    buttons: Math.max(0, integer(sample.buttons ?? 0, 0)),
    clientX: finite(sample.clientX, 0),
    clientY: finite(sample.clientY, 0),
    pressure: clamp(finite(sample.pressure ?? 0, 0), 0, 1),
    tangentialPressure: clamp(
      finite(sample.tangentialPressure ?? 0, 0),
      -1,
      1,
    ),
    tiltX: clamp(finite(sample.tiltX ?? 0, 0), -90, 90),
    tiltY: clamp(finite(sample.tiltY ?? 0, 0), -90, 90),
    twist: clamp(finite(sample.twist ?? 0, 0), 0, 359),
    width: Math.max(0, finite(sample.width ?? 1, 1)),
    height: Math.max(0, finite(sample.height ?? 1, 1)),
    timeStamp: Math.max(0, finite(sample.timeStamp ?? 0, 0)),
  });
}

export interface PointerCaptureTarget {
  setPointerCapture(pointerId: number): void;
  hasPointerCapture(pointerId: number): boolean;
  releasePointerCapture(pointerId: number): void;
}

export function beginPointerCapture(
  target: PointerCaptureTarget,
  pointerId: number,
): boolean {
  try {
    target.setPointerCapture(pointerId);
    return target.hasPointerCapture(pointerId);
  } catch {
    return false;
  }
}

export function releasePointerCapture(
  target: PointerCaptureTarget,
  pointerId: number,
): boolean {
  try {
    if (!target.hasPointerCapture(pointerId)) return false;
    target.releasePointerCapture(pointerId);
    return true;
  } catch {
    return false;
  }
}

export interface ShellWheelEvent {
  readonly deltaX: number;
  readonly deltaY: number;
  readonly deltaMode: number;
  readonly clientX: number;
  readonly shiftKey: boolean;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
}

export interface ShellWheelMetrics {
  readonly lineHeightPixels: number;
  readonly pageWidthPixels: number;
  readonly pageHeightPixels: number;
}

export interface NormalizedWheelInput {
  readonly intent: "scroll" | "pan" | "zoom";
  readonly deltaXPixel: number;
  readonly deltaYPixel: number;
  readonly horizontalPixel: number;
  readonly zoomFactor: number;
  readonly anchorClientX: number;
}

export function normalizeWheelInput(
  event: ShellWheelEvent,
  metrics: ShellWheelMetrics,
): NormalizedWheelInput {
  const lineHeight = positive(metrics.lineHeightPixels, 16);
  const pageWidth = positive(metrics.pageWidthPixels, 1);
  const pageHeight = positive(metrics.pageHeightPixels, 1);
  const xScale = event.deltaMode === 1
    ? lineHeight
    : event.deltaMode === 2
      ? pageWidth
      : 1;
  const yScale = event.deltaMode === 1
    ? lineHeight
    : event.deltaMode === 2
      ? pageHeight
      : 1;
  const deltaXPixel = finite(event.deltaX, 0) * xScale;
  const deltaYPixel = finite(event.deltaY, 0) * yScale;
  const zoom = event.metaKey || event.ctrlKey;
  const shiftedPan =
    event.shiftKey && Math.abs(deltaYPixel) > Math.abs(deltaXPixel);
  return Object.freeze({
    intent: zoom ? "zoom" : shiftedPan ? "pan" : "scroll",
    deltaXPixel,
    deltaYPixel,
    horizontalPixel: shiftedPan
      ? deltaXPixel + deltaYPixel
      : deltaXPixel,
    zoomFactor: zoom
      ? clamp(Math.exp(-deltaYPixel * 0.002), 0.05, 20)
      : 1,
    anchorClientX: finite(event.clientX, 0),
  });
}

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback;
}

function positive(value: number, fallback: number): number {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function integer(value: number, fallback: number): number {
  return Number.isSafeInteger(value) ? value : fallback;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}
