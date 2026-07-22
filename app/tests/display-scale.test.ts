import assert from "node:assert/strict";
import test from "node:test";

import {
  formatDisplayScale,
  observeDisplayScale,
  type DisplayScaleHost,
  type DisplayScaleObservation,
} from "../src/display-scale.ts";

class FakeScaleHost implements DisplayScaleHost {
  scale = 1;
  windowListener: (() => void) | null = null;
  viewportListener: (() => void) | null = null;
  resolutionListener: (() => void) | null = null;
  watchedScales: number[] = [];
  resolutionDisposals = 0;

  readScale() {
    return this.scale;
  }

  subscribeWindowResize(listener: () => void) {
    this.windowListener = listener;
    return () => { this.windowListener = null; };
  }

  subscribeViewportResize(listener: () => void) {
    this.viewportListener = listener;
    return () => { this.viewportListener = null; };
  }

  subscribeResolution(scaleFactor: number, listener: () => void) {
    this.watchedScales.push(scaleFactor);
    this.resolutionListener = listener;
    return () => {
      this.resolutionDisposals += 1;
      if (this.resolutionListener === listener) this.resolutionListener = null;
    };
  }
}

test("display scale observer publishes immutable changes and suppresses duplicate signals", () => {
  const host = new FakeScaleHost();
  const observations: DisplayScaleObservation[] = [];
  const stop = observeDisplayScale(host, (observation) => observations.push(observation));

  assert.deepEqual(observations, [
    { revision: 0, scaleFactor: 1, source: "initial" },
  ]);
  assert.ok(Object.isFrozen(observations[0]));
  host.windowListener?.();
  assert.equal(observations.length, 1);

  host.scale = 1.25;
  host.resolutionListener?.();
  assert.deepEqual(observations[1], {
    revision: 1,
    scaleFactor: 1.25,
    source: "resolution",
  });
  assert.deepEqual(host.watchedScales, [1, 1.25]);
  assert.equal(host.resolutionDisposals, 1);

  host.scale = 2;
  host.viewportListener?.();
  assert.deepEqual(observations[2], {
    revision: 2,
    scaleFactor: 2,
    source: "viewport",
  });
  assert.equal(formatDisplayScale(observations[2]), "2x display scale");

  stop();
  assert.equal(host.windowListener, null);
  assert.equal(host.viewportListener, null);
  assert.equal(host.resolutionListener, null);
});

test("invalid dynamic display scale is ignored without replacing the last valid value", () => {
  const host = new FakeScaleHost();
  host.scale = 1.5;
  const observations: DisplayScaleObservation[] = [];
  const stop = observeDisplayScale(host, (observation) => observations.push(observation));
  host.scale = Number.NaN;
  host.windowListener?.();
  host.scale = 0;
  host.viewportListener?.();
  assert.equal(observations.length, 1);
  assert.equal(observations[0].scaleFactor, 1.5);
  stop();
});

test("invalid initial display scale fails before installing listeners", () => {
  const host = new FakeScaleHost();
  host.scale = Number.POSITIVE_INFINITY;
  assert.throws(() => observeDisplayScale(host, () => {}), /display scale/i);
  assert.equal(host.windowListener, null);
  assert.equal(host.viewportListener, null);
  assert.equal(host.resolutionListener, null);
});
