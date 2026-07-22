export type DisplayScaleSource =
  | "initial"
  | "window"
  | "viewport"
  | "resolution";

export interface DisplayScaleObservation {
  readonly revision: number;
  readonly scaleFactor: number;
  readonly source: DisplayScaleSource;
}

export interface DisplayScaleHost {
  readScale(): number;
  subscribeWindowResize(listener: () => void): () => void;
  subscribeViewportResize(listener: () => void): () => void;
  subscribeResolution(
    scaleFactor: number,
    listener: () => void,
  ): () => void;
}

export function observeDisplayScale(
  host: DisplayScaleHost,
  listener: (observation: DisplayScaleObservation) => void,
): () => void {
  let stopped = false;
  let observation = createObservation(0, requireScale(host.readScale()), "initial");
  let stopResolution = () => {};

  const publish = (source: Exclude<DisplayScaleSource, "initial">) => {
    if (stopped) return;
    const nextScale = optionalScale(host.readScale());
    if (nextScale === null || nextScale === observation.scaleFactor) return;
    observation = createObservation(
      observation.revision + 1,
      nextScale,
      source,
    );
    listener(observation);
    stopResolution();
    stopResolution = host.subscribeResolution(observation.scaleFactor, () =>
      publish("resolution"),
    );
  };

  listener(observation);
  const stopWindow = host.subscribeWindowResize(() => publish("window"));
  const stopViewport = host.subscribeViewportResize(() => publish("viewport"));
  stopResolution = host.subscribeResolution(observation.scaleFactor, () =>
    publish("resolution"),
  );

  return () => {
    if (stopped) return;
    stopped = true;
    stopWindow();
    stopViewport();
    stopResolution();
  };
}

export function observeBrowserDisplayScale(
  listener: (observation: DisplayScaleObservation) => void,
): () => void {
  return observeDisplayScale(createBrowserDisplayScaleHost(), listener);
}

export function formatDisplayScale(
  observation: DisplayScaleObservation,
): string {
  const value = Number.isInteger(observation.scaleFactor)
    ? observation.scaleFactor.toString()
    : observation.scaleFactor.toFixed(3).replace(/0+$/u, "").replace(/\.$/u, "");
  return `${value}x display scale`;
}

function createBrowserDisplayScaleHost(): DisplayScaleHost {
  return {
    readScale: () => window.devicePixelRatio,
    subscribeWindowResize(listener) {
      window.addEventListener("resize", listener);
      return () => window.removeEventListener("resize", listener);
    },
    subscribeViewportResize(listener) {
      const viewport = window.visualViewport;
      if (viewport === null) return () => {};
      viewport.addEventListener("resize", listener);
      return () => viewport.removeEventListener("resize", listener);
    },
    subscribeResolution(scaleFactor, listener) {
      const query = window.matchMedia(`(resolution: ${scaleFactor}dppx)`);
      query.addEventListener("change", listener);
      return () => query.removeEventListener("change", listener);
    },
  };
}

function requireScale(value: number): number {
  const scale = optionalScale(value);
  if (scale === null) {
    throw new Error("Display scale must be finite and between 0.25x and 16x.");
  }
  return scale;
}

function optionalScale(value: number): number | null {
  return Number.isFinite(value) && value >= 0.25 && value <= 16
    ? value
    : null;
}

function createObservation(
  revision: number,
  scaleFactor: number,
  source: DisplayScaleSource,
): DisplayScaleObservation {
  return Object.freeze({ revision, scaleFactor, source });
}
