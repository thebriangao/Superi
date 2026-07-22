import assert from "node:assert/strict";
import test from "node:test";

import {
  beginPointerCapture,
  keyboardInputDisposition,
  latestPointerSample,
  normalizePointerSamples,
  normalizeWheelInput,
  releasePointerCapture,
  restoreShellFocus,
} from "../src/shell-input.ts";

test("keyboard routing respects handled, composing, repeat, and editable input", () => {
  const base = {
    key: "k",
    metaKey: false,
    ctrlKey: true,
    altKey: false,
    shiftKey: false,
  };
  assert.equal(keyboardInputDisposition(base, false, false), "route");
  assert.equal(
    keyboardInputDisposition({ ...base, defaultPrevented: true }, false, false),
    "handled",
  );
  assert.equal(
    keyboardInputDisposition({ ...base, isComposing: true }, false, false),
    "composing",
  );
  assert.equal(
    keyboardInputDisposition({ ...base, repeat: true }, false, false),
    "repeated",
  );
  assert.equal(keyboardInputDisposition(base, true, false), "editable");
  assert.equal(keyboardInputDisposition(base, true, true), "route");
});

test("focus restoration uses a connected operable target and an exact fallback", () => {
  const calls: string[] = [];
  const disconnected = {
    isConnected: false,
    hidden: false,
    getAttribute: () => null,
    focus: () => calls.push("disconnected"),
  };
  const fallback = {
    isConnected: true,
    hidden: false,
    getAttribute: () => null,
    focus: (options?: FocusOptions) =>
      calls.push(options?.preventScroll ? "fallback:stable" : "fallback"),
  };
  assert.equal(restoreShellFocus(disconnected, fallback), "fallback");
  assert.deepEqual(calls, ["fallback:stable"]);
  assert.equal(
    restoreShellFocus(
      { ...fallback, getAttribute: (name) => (name === "aria-hidden" ? "true" : null) },
      null,
    ),
    "unavailable",
  );
});

test("coalesced pointer samples preserve pen precision and stable identity", () => {
  const event = {
    pointerId: 17,
    pointerType: "pen",
    isPrimary: true,
    button: 0,
    buttons: 1,
    clientX: 12.75,
    clientY: 8.5,
    pressure: 0.625,
    tangentialPressure: -0.25,
    tiltX: 21,
    tiltY: -14,
    twist: 270,
    width: 2.5,
    height: 3.25,
    timeStamp: 102,
    getCoalescedEvents: () => [
      {
        pointerId: 17,
        pointerType: "pen",
        isPrimary: true,
        button: 0,
        buttons: 1,
        clientX: 11.25,
        clientY: 8,
        pressure: 0.5,
        tangentialPressure: -0.2,
        tiltX: 20,
        tiltY: -13,
        twist: 269,
        width: 2.25,
        height: 3,
        timeStamp: 101,
      },
    ],
  };
  const samples = normalizePointerSamples(event);
  assert.equal(samples.length, 2);
  assert.deepEqual(samples.map((sample) => sample.clientX), [11.25, 12.75]);
  assert.equal(samples[1].pointerType, "pen");
  assert.equal(samples[1].pressure, 0.625);
  assert.equal(samples[1].tangentialPressure, -0.25);
  assert.equal(samples[1].twist, 270);
  assert.deepEqual(latestPointerSample(event), samples[1]);
  assert.ok(Object.isFrozen(samples));
  assert.ok(Object.isFrozen(samples[0]));
});

test("pointer capture starts and releases without leaking cancellation failures", () => {
  const captures = new Set<number>();
  const target = {
    setPointerCapture(pointerId: number) {
      captures.add(pointerId);
    },
    hasPointerCapture(pointerId: number) {
      return captures.has(pointerId);
    },
    releasePointerCapture(pointerId: number) {
      captures.delete(pointerId);
    },
  };
  assert.equal(beginPointerCapture(target, 4), true);
  assert.equal(captures.has(4), true);
  assert.equal(releasePointerCapture(target, 4), true);
  assert.equal(releasePointerCapture(target, 4), false);
  assert.equal(
    beginPointerCapture(
      { ...target, setPointerCapture: () => { throw new Error("detached"); } },
      5,
    ),
    false,
  );
});

test("wheel normalization preserves fractional precision across pixel, line, and page modes", () => {
  const metrics = {
    lineHeightPixels: 18,
    pageWidthPixels: 900,
    pageHeightPixels: 600,
  };
  assert.deepEqual(
    normalizeWheelInput(
      {
        deltaX: 0.125,
        deltaY: 1.375,
        deltaMode: 0,
        clientX: 240.5,
        shiftKey: false,
        ctrlKey: false,
        metaKey: false,
      },
      metrics,
    ),
    {
      intent: "scroll",
      deltaXPixel: 0.125,
      deltaYPixel: 1.375,
      horizontalPixel: 0.125,
      zoomFactor: 1,
      anchorClientX: 240.5,
    },
  );
  assert.equal(
    normalizeWheelInput(
      {
        deltaX: 0,
        deltaY: 2.5,
        deltaMode: 1,
        clientX: 0,
        shiftKey: true,
        ctrlKey: false,
        metaKey: false,
      },
      metrics,
    ).horizontalPixel,
    45,
  );
  assert.equal(
    normalizeWheelInput(
      {
        deltaX: 0,
        deltaY: 0.5,
        deltaMode: 2,
        clientX: 0,
        shiftKey: false,
        ctrlKey: true,
        metaKey: false,
      },
      metrics,
    ).deltaYPixel,
    300,
  );
  assert.equal(
    normalizeWheelInput(
      {
        deltaX: 0,
        deltaY: 0.5,
        deltaMode: 2,
        clientX: 0,
        shiftKey: false,
        ctrlKey: true,
        metaKey: false,
      },
      metrics,
    ).intent,
    "zoom",
  );
});
