import {
  restoreShellFocus,
  type ShellFocusResult,
  type ShellFocusTarget,
} from "./shell-input.ts";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "summary",
  "audio[controls]",
  "video[controls]",
  "[contenteditable]:not([contenteditable='false'])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

const KEYBOARD_LANDMARK_SELECTOR = "[data-keyboard-landmark]";

export type KeyboardLandmarkDirection = "forward" | "backward";

export interface KeyboardLandmarkEvent {
  readonly key: string;
  readonly shiftKey?: boolean;
  readonly altKey?: boolean;
  readonly ctrlKey?: boolean;
  readonly metaKey?: boolean;
  readonly defaultPrevented?: boolean;
  readonly isComposing?: boolean;
  readonly repeat?: boolean;
}

export function keyboardLandmarkDirection(
  event: KeyboardLandmarkEvent,
): KeyboardLandmarkDirection | null {
  if (
    event.key !== "F6" ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey ||
    event.defaultPrevented ||
    event.isComposing ||
    event.repeat
  ) {
    return null;
  }
  return event.shiftKey ? "backward" : "forward";
}

export function nextKeyboardLandmarkIndex(
  candidateCount: number,
  currentIndex: number,
  direction: KeyboardLandmarkDirection,
): number | null {
  return nextContainedFocusIndex(
    candidateCount,
    currentIndex,
    direction === "backward",
  );
}

export function focusAdjacentKeyboardLandmark(
  root: HTMLElement,
  activeElement: Element | null,
  direction: KeyboardLandmarkDirection,
): ShellFocusResult {
  const landmarks = Array.from(
    root.querySelectorAll<HTMLElement>(KEYBOARD_LANDMARK_SELECTOR),
  ).filter(isAvailableLandmark);
  if (landmarks.length === 0) return "unavailable";

  const current = activeElement?.closest<HTMLElement>(
    KEYBOARD_LANDMARK_SELECTOR,
  ) ?? null;
  let index = landmarks.findIndex((landmark) => landmark === current);
  for (let attempt = 0; attempt < landmarks.length; attempt += 1) {
    index = nextKeyboardLandmarkIndex(landmarks.length, index, direction) ?? -1;
    const candidate = landmarks[index];
    if (candidate !== undefined) {
      const result = focusFirstInScope(candidate);
      if (result !== "unavailable") return result;
    }
  }
  return "unavailable";
}

export function focusKeyboardLandmark(
  root: HTMLElement,
  landmarkId: string,
): ShellFocusResult {
  const landmark = Array.from(
    root.querySelectorAll<HTMLElement>(KEYBOARD_LANDMARK_SELECTOR),
  ).find(
    (candidate) =>
      candidate.dataset.keyboardLandmark === landmarkId &&
      isAvailableLandmark(candidate),
  );
  return restoreShellFocus(landmark ?? null);
}

export function nextContainedFocusIndex(
  candidateCount: number,
  currentIndex: number,
  reverse: boolean,
): number | null {
  const count = Math.max(0, Math.trunc(candidateCount));
  if (count === 0) return null;
  if (currentIndex < 0 || currentIndex >= count) {
    return reverse ? count - 1 : 0;
  }
  return (currentIndex + (reverse ? -1 : 1) + count) % count;
}

export function focusFirstInScope(
  scope: HTMLElement,
  preferred: HTMLElement | null = null,
): ShellFocusResult {
  return restoreShellFocus(
    preferred,
    focusableElements(scope)[0] ?? scope,
  );
}

export function containTabFocus(
  scope: HTMLElement,
  activeElement: Element | null,
  reverse: boolean,
): boolean {
  const candidates = focusableElements(scope);
  const currentIndex = candidates.findIndex(
    (candidate) => candidate === activeElement,
  );
  const nextIndex = nextContainedFocusIndex(
    candidates.length,
    currentIndex,
    reverse,
  );
  const nextTarget = nextIndex === null ? null : candidates[nextIndex] ?? null;
  return restoreShellFocus(nextTarget, scope) !== "unavailable";
}

function focusableElements(scope: HTMLElement): HTMLElement[] {
  return Array.from(scope.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    isFocusable,
  );
}

function isAvailableLandmark(target: HTMLElement): boolean {
  return (
    target.isConnected &&
    !target.hidden &&
    target.closest("[hidden], [inert], [aria-hidden='true']") === null
  );
}

function isFocusable(target: HTMLElement): target is HTMLElement & ShellFocusTarget {
  if (
    target.tabIndex < 0 ||
    target.hidden ||
    target.getClientRects().length === 0 ||
    target.closest("[hidden], [inert], [aria-hidden='true']") !== null
  ) {
    return false;
  }
  const view = target.ownerDocument.defaultView;
  if (view === null) return true;
  const style = view.getComputedStyle(target);
  return style.display !== "none" && style.visibility !== "hidden";
}
