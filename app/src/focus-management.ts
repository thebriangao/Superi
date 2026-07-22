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
