export const DESKTOP_BACKGROUND_JOBS_SCHEMA_VERSION = 1;
export const MAX_DESKTOP_BACKGROUND_JOBS = 64;

export type DesktopBackgroundJobKind =
  | "proxy"
  | "analysis"
  | "cache"
  | "save";

export type DesktopBackgroundJobStatus =
  | "running"
  | "completed"
  | "failed"
  | "interrupted";

export interface DesktopBackgroundJobFailure {
  readonly title: string;
  readonly action: string;
}

export interface DesktopBackgroundJobReceipt {
  readonly id: string;
  readonly sequence: number;
  readonly kind: DesktopBackgroundJobKind;
  readonly label: string;
  readonly status: DesktopBackgroundJobStatus;
  readonly started_at: string;
  readonly finished_at: string | null;
  readonly failure: DesktopBackgroundJobFailure | null;
}

export interface DesktopBackgroundJobsSnapshot {
  readonly schema_version: 1;
  readonly revision: number;
  readonly next_sequence: number;
  readonly jobs: readonly DesktopBackgroundJobReceipt[];
}

export interface DesktopBackgroundJobInput {
  readonly kind: DesktopBackgroundJobKind;
  readonly label: string;
}

interface RetryOwner {
  readonly input: DesktopBackgroundJobInput;
  readonly work: () => Promise<unknown>;
}

export interface BackgroundJobJournalOptions {
  readonly capacity?: number;
  readonly now?: () => string;
}

const INTERRUPTED_FAILURE = Object.freeze({
  title: "Background operation was interrupted",
  action:
    "Review current project state, then run the operation again if it is still needed.",
});
const TEXT_ENCODER = new TextEncoder();
const SNAPSHOT_KEYS = Object.freeze([
  "schema_version",
  "revision",
  "next_sequence",
  "jobs",
]);
const RECEIPT_KEYS = Object.freeze([
  "id",
  "sequence",
  "kind",
  "label",
  "status",
  "started_at",
  "finished_at",
  "failure",
]);
const FAILURE_KEYS = Object.freeze(["title", "action"]);

export class BackgroundJobJournal {
  readonly #capacity: number;
  readonly #now: () => string;
  readonly #listeners = new Set<() => void>();
  readonly #retryOwners = new Map<string, RetryOwner>();
  #snapshot: DesktopBackgroundJobsSnapshot;

  public constructor(options: BackgroundJobJournalOptions = {}) {
    const capacity = options.capacity ?? MAX_DESKTOP_BACKGROUND_JOBS;
    if (
      !Number.isSafeInteger(capacity) ||
      capacity < 1 ||
      capacity > MAX_DESKTOP_BACKGROUND_JOBS
    ) {
      throw new Error("Background job capacity is invalid");
    }
    this.#capacity = capacity;
    this.#now = options.now ?? (() => new Date().toISOString());
    this.#snapshot = freezeSnapshot({
      schema_version: DESKTOP_BACKGROUND_JOBS_SCHEMA_VERSION,
      revision: 0,
      next_sequence: 1,
      jobs: [],
    });
  }

  public readonly getSnapshot = (): DesktopBackgroundJobsSnapshot =>
    this.#snapshot;

  public readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  };

  public hasRunningJobs(): boolean {
    return this.#snapshot.jobs.some((job) => job.status === "running");
  }

  public canRetry(id: string): boolean {
    return (
      this.#snapshot.jobs.some(
        (job) => job.id === id && job.status === "failed",
      ) && this.#retryOwners.has(id)
    );
  }

  public async run<T>(
    input: DesktopBackgroundJobInput,
    work: () => Promise<T>,
  ): Promise<T> {
    const normalized = normalizeInput(input);
    const receipt = this.#begin(normalized, work);
    try {
      const result = await work();
      this.#finish(receipt.id, "completed", null);
      return result;
    } catch (error: unknown) {
      this.#finish(receipt.id, "failed", projectFailure(error));
      throw error;
    }
  }

  public async retry(id: string): Promise<boolean> {
    if (!this.canRetry(id)) return false;
    const owner = this.#retryOwners.get(id);
    if (owner === undefined) return false;
    this.#retryOwners.delete(id);
    await this.run(owner.input, owner.work);
    return true;
  }

  public dismiss(id: string): boolean {
    const job = this.#snapshot.jobs.find((candidate) => candidate.id === id);
    if (job === undefined || job.status === "running") return false;
    this.#ensureRevisionAvailable();
    this.#retryOwners.delete(id);
    this.#publish(
      this.#snapshot.jobs.filter((candidate) => candidate.id !== id),
      this.#snapshot.next_sequence,
    );
    return true;
  }

  public restore(candidate: DesktopBackgroundJobsSnapshot): void {
    const restored = validateRestoredSnapshot(candidate, this.#capacity);
    const hadRunning = restored.jobs.some((job) => job.status === "running");
    if (hadRunning && restored.revision >= Number.MAX_SAFE_INTEGER) {
      throw new Error("Background job revision is exhausted");
    }
    const interruptedAt = currentUtcTimestamp(this.#now());
    const jobs = restored.jobs.map((job) =>
      job.status === "running"
        ? freezeReceipt({
            ...job,
            status: "interrupted",
            finished_at: terminalTimestamp(interruptedAt, job.started_at),
            failure: INTERRUPTED_FAILURE,
          })
        : freezeReceipt(job),
    );
    this.#retryOwners.clear();
    this.#snapshot = freezeSnapshot({
      schema_version: DESKTOP_BACKGROUND_JOBS_SCHEMA_VERSION,
      revision: restored.revision + Number(hadRunning),
      next_sequence: restored.next_sequence,
      jobs,
    });
    this.#emit();
  }

  #begin<T>(
    input: DesktopBackgroundJobInput,
    work: () => Promise<T>,
  ): DesktopBackgroundJobReceipt {
    this.#ensureRevisionAvailable(2);
    const jobs = [...this.#snapshot.jobs];
    if (jobs.length >= this.#capacity) {
      const removableIndex = jobs.findIndex((job) => job.status !== "running");
      if (removableIndex < 0) {
        throw new Error("Background job capacity is full");
      }
      const [removed] = jobs.splice(removableIndex, 1);
      if (removed !== undefined) this.#retryOwners.delete(removed.id);
    }
    const sequence = this.#snapshot.next_sequence;
    if (
      !Number.isSafeInteger(sequence) ||
      sequence < 1 ||
      sequence >= Number.MAX_SAFE_INTEGER
    ) {
      throw new Error("Background job sequence is exhausted");
    }
    const receipt = freezeReceipt({
      id: `desktop-job-${sequence}`,
      sequence,
      kind: input.kind,
      label: input.label,
      status: "running",
      started_at: currentUtcTimestamp(this.#now()),
      finished_at: null,
      failure: null,
    });
    this.#retryOwners.set(receipt.id, { input, work });
    jobs.push(receipt);
    this.#publish(jobs, sequence + 1);
    return receipt;
  }

  #finish(
    id: string,
    status: "completed" | "failed",
    failure: DesktopBackgroundJobFailure | null,
  ): void {
    this.#ensureRevisionAvailable();
    const jobs = this.#snapshot.jobs.map((job) =>
      job.id === id && job.status === "running"
        ? freezeReceipt({
            ...job,
            status,
            finished_at: terminalTimestamp(
              currentUtcTimestamp(this.#now()),
              job.started_at,
            ),
            failure,
          })
        : job,
    );
    if (status === "completed") this.#retryOwners.delete(id);
    this.#publish(jobs, this.#snapshot.next_sequence);
  }

  #publish(
    jobs: readonly DesktopBackgroundJobReceipt[],
    nextSequence: number,
  ): void {
    this.#snapshot = freezeSnapshot({
      schema_version: DESKTOP_BACKGROUND_JOBS_SCHEMA_VERSION,
      revision: this.#snapshot.revision + 1,
      next_sequence: nextSequence,
      jobs,
    });
    this.#emit();
  }

  #ensureRevisionAvailable(required = 1): void {
    if (this.#snapshot.revision > Number.MAX_SAFE_INTEGER - required) {
      throw new Error("Background job revision is exhausted");
    }
  }

  #emit(): void {
    for (const listener of this.#listeners) {
      try {
        listener();
      } catch {
        // Presentation observers cannot replace the authoritative work result.
      }
    }
  }
}

function normalizeInput(
  input: DesktopBackgroundJobInput,
): DesktopBackgroundJobInput {
  if (!isJobKind(input.kind)) throw new Error("Background job kind is invalid");
  const label = boundedText(input.label);
  if (label === null) throw new Error("Background job label is invalid");
  return Object.freeze({ kind: input.kind, label });
}

function projectFailure(error: unknown): DesktopBackgroundJobFailure {
  if (isRecord(error)) {
    const title = boundedText(error.title);
    const action = boundedText(error.action);
    if (title !== null && action !== null) {
      return Object.freeze({ title, action });
    }
  }
  return Object.freeze({
    title: "Background operation failed",
    action: "Review current project state and try the operation again.",
  });
}

function validateRestoredSnapshot(
  candidate: DesktopBackgroundJobsSnapshot,
  capacity: number,
): DesktopBackgroundJobsSnapshot {
  if (
    !hasExactKeys(candidate, SNAPSHOT_KEYS) ||
    candidate.schema_version !== DESKTOP_BACKGROUND_JOBS_SCHEMA_VERSION ||
    !Number.isSafeInteger(candidate.revision) ||
    candidate.revision < 0 ||
    !Number.isSafeInteger(candidate.next_sequence) ||
    candidate.next_sequence < 1 ||
    !Array.isArray(candidate.jobs) ||
    candidate.jobs.length > capacity
  ) {
    throw new Error("Background job snapshot is invalid");
  }
  const identities = new Set<string>();
  let greatestSequence = 0;
  for (const job of candidate.jobs) {
    if (
      !hasExactKeys(job, RECEIPT_KEYS) ||
      typeof job.id !== "string" ||
      job.id !== `desktop-job-${job.sequence}` ||
      identities.has(job.id) ||
      !Number.isSafeInteger(job.sequence) ||
      job.sequence < 1 ||
      !isJobKind(job.kind) ||
      !isJobStatus(job.status) ||
      boundedText(job.label) !== job.label ||
      !isUtcTimestamp(job.started_at) ||
      (job.finished_at !== null &&
        (!isUtcTimestamp(job.finished_at) ||
          Date.parse(job.finished_at) < Date.parse(job.started_at))) ||
      (job.status === "running") !== (job.finished_at === null) ||
      (job.status === "failed" || job.status === "interrupted") !==
        (job.failure !== null) ||
      (job.failure !== null &&
        (!hasExactKeys(job.failure, FAILURE_KEYS) ||
          boundedText(job.failure.title) !== job.failure.title ||
          boundedText(job.failure.action) !== job.failure.action))
    ) {
      throw new Error("Background job receipt is invalid");
    }
    identities.add(job.id);
    greatestSequence = Math.max(greatestSequence, job.sequence);
  }
  if (candidate.next_sequence <= greatestSequence) {
    throw new Error("Background job sequence is stale");
  }
  return candidate;
}

function isJobKind(value: unknown): value is DesktopBackgroundJobKind {
  return ["proxy", "analysis", "cache", "save"].includes(String(value));
}

function isJobStatus(value: unknown): value is DesktopBackgroundJobStatus {
  return ["running", "completed", "failed", "interrupted"].includes(
    String(value),
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function hasExactKeys(value: unknown, keys: readonly string[]): boolean {
  if (!isRecord(value)) return false;
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function isUtcTimestamp(value: unknown): value is string {
  return typeof value === "string" && utcTimestampParts(value) !== null;
}

function currentUtcTimestamp(value: unknown): string {
  if (!isUtcTimestamp(value)) {
    throw new Error("Background job clock is invalid");
  }
  return value;
}

function terminalTimestamp(candidate: string, startedAt: string): string {
  const terminal = utcTimestampParts(candidate);
  const started = utcTimestampParts(startedAt);
  if (terminal === null || started === null) {
    throw new Error("Background job timestamp is invalid");
  }
  for (let index = 0; index < terminal.length; index += 1) {
    if (terminal[index] < started[index]) return startedAt;
    if (terminal[index] > started[index]) return candidate;
  }
  return candidate;
}

function utcTimestampParts(value: string): readonly number[] | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?Z$/u.exec(
    value,
  );
  if (match === null) return null;
  const [year, month, day, hour, minute, second] = match
    .slice(1, 7)
    .map(Number);
  const leapYear = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const maximumDay = [
    0,
    31,
    leapYear ? 29 : 28,
    31,
    30,
    31,
    30,
    31,
    31,
    30,
    31,
    30,
    31,
  ][month];
  if (
    maximumDay === undefined ||
    day < 1 ||
    day > maximumDay ||
    hour >= 24 ||
    minute >= 60 ||
    second >= 60
  ) {
    return null;
  }
  const nanoseconds = Number((match[7] ?? "").padEnd(9, "0"));
  return [year, month, day, hour, minute, second, nanoseconds];
}

function boundedText(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const text = value.trim();
  if (
    text.length === 0 ||
    TEXT_ENCODER.encode(text).length > 512 ||
    /\p{Cc}/u.test(text)
  ) {
    return null;
  }
  return text;
}

function freezeReceipt(
  receipt: DesktopBackgroundJobReceipt,
): DesktopBackgroundJobReceipt {
  return Object.freeze({
    ...receipt,
    failure:
      receipt.failure === null
        ? null
        : Object.freeze({ ...receipt.failure }),
  });
}

function freezeSnapshot(
  snapshot: DesktopBackgroundJobsSnapshot,
): DesktopBackgroundJobsSnapshot {
  return Object.freeze({
    ...snapshot,
    jobs: Object.freeze(snapshot.jobs.map(freezeReceipt)),
  });
}

export const desktopBackgroundJobJournal = new BackgroundJobJournal();

export function runDesktopBackgroundJob<T>(
  input: DesktopBackgroundJobInput,
  work: () => Promise<T>,
): Promise<T> {
  return desktopBackgroundJobJournal.run(input, work);
}
