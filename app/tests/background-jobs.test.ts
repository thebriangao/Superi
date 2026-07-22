import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  BackgroundJobJournal,
  MAX_DESKTOP_BACKGROUND_JOBS,
  type DesktopBackgroundJobsSnapshot,
} from "../src/background-jobs.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function clock(...values: string[]): () => string {
  let index = 0;
  return () => values[Math.min(index++, values.length - 1)];
}

test("real work publishes running and terminal receipts without replacing its result", async () => {
  const journal = new BackgroundJobJournal({
    now: clock("2026-07-22T04:00:00Z", "2026-07-22T04:00:03Z"),
  });
  let finish!: (value: number) => void;
  const work = new Promise<number>((resolve) => {
    finish = resolve;
  });
  const observed: number[] = [];
  const unsubscribe = journal.subscribe(() => {
    observed.push(journal.getSnapshot().revision);
  });

  const pending = journal.run(
    { kind: "cache", label: "Generate preview for river.mov" },
    () => work,
  );
  const running = journal.getSnapshot().jobs[0];
  assert.equal(running.status, "running");
  assert.equal(running.started_at, "2026-07-22T04:00:00Z");
  assert.equal(running.finished_at, null);
  assert.equal(running.failure, null);
  assert.equal(journal.hasRunningJobs(), true);
  assert.equal(journal.canRetry(running.id), false);
  assert.equal(await journal.retry(running.id), false);

  finish(17);
  assert.equal(await pending, 17);
  const completed = journal.getSnapshot().jobs[0];
  assert.equal(completed.status, "completed");
  assert.equal(completed.finished_at, "2026-07-22T04:00:03Z");
  assert.equal(completed.failure, null);
  assert.equal(journal.hasRunningJobs(), false);
  assert.deepEqual(observed, [1, 2]);
  assert.ok(Object.isFrozen(journal.getSnapshot()));
  assert.ok(Object.isFrozen(journal.getSnapshot().jobs));
  assert.ok(Object.isFrozen(journal.getSnapshot().jobs[0]));
  unsubscribe();
});

test("classified failures remain visible and same-session retry creates fresh history", async () => {
  const journal = new BackgroundJobJournal({
    now: clock(
      "2026-07-22T04:01:00Z",
      "2026-07-22T04:01:01Z",
      "2026-07-22T04:01:02Z",
      "2026-07-22T04:01:03Z",
    ),
  });
  let attempts = 0;
  const operation = async () => {
    attempts += 1;
    if (attempts === 1) {
      throw {
        title: "Proxy source changed",
        action: "Refresh the media library and regenerate the proxy.",
      };
    }
    return "ready";
  };

  await assert.rejects(
    journal.run({ kind: "proxy", label: "Proxy interview.mov" }, operation),
  );
  const failed = journal.getSnapshot().jobs[0];
  assert.deepEqual(failed.failure, {
    title: "Proxy source changed",
    action: "Refresh the media library and regenerate the proxy.",
  });
  assert.equal(failed.status, "failed");
  assert.equal(journal.canRetry(failed.id), true);

  assert.equal(await journal.retry(failed.id), true);
  assert.equal(attempts, 2);
  assert.equal(journal.getSnapshot().jobs.length, 2);
  assert.equal(journal.getSnapshot().jobs[0].status, "failed");
  assert.equal(journal.getSnapshot().jobs[1].status, "completed");
  assert.equal(journal.canRetry(failed.id), false);
  assert.equal(journal.canRetry(journal.getSnapshot().jobs[1].id), false);
});

test("restart restores bounded evidence and marks unfinished work interrupted", () => {
  const restored: DesktopBackgroundJobsSnapshot = {
    schema_version: 1,
    revision: 8,
    next_sequence: 11,
    jobs: [
      {
        id: "desktop-job-9",
        sequence: 9,
        kind: "analysis",
        label: "Analyze interview.mov",
        status: "running",
        started_at: "2026-07-22T04:02:00Z",
        finished_at: null,
        failure: null,
      },
      {
        id: "desktop-job-10",
        sequence: 10,
        kind: "save",
        label: "Save alpha.superi",
        status: "completed",
        started_at: "2026-07-22T04:02:01Z",
        finished_at: "2026-07-22T04:02:02Z",
        failure: null,
      },
    ],
  };
  const journal = new BackgroundJobJournal({
    now: clock("2026-07-22T04:03:00Z"),
  });

  journal.restore(restored);
  const snapshot = journal.getSnapshot();
  assert.equal(snapshot.schema_version, 1);
  assert.equal(snapshot.revision, 9);
  assert.equal(snapshot.next_sequence, 11);
  assert.equal(snapshot.jobs[0].status, "interrupted");
  assert.equal(snapshot.jobs[0].finished_at, "2026-07-22T04:03:00Z");
  assert.deepEqual(snapshot.jobs[0].failure, {
    title: "Background operation was interrupted",
    action: "Review current project state, then run the operation again if it is still needed.",
  });
  assert.equal(journal.canRetry(snapshot.jobs[0].id), false);
  assert.equal(snapshot.jobs[1].status, "completed");

  assert.throws(
    () =>
      journal.restore({
        ...restored,
        jobs: [{ ...restored.jobs[0], started_at: "not-a-timestamp" }],
      }),
    /receipt is invalid/,
  );
  assert.throws(
    () =>
      journal.restore({
        ...restored,
        jobs: [
          {
            ...restored.jobs[0],
            started_at: "2026-02-31T04:02:00Z",
          },
        ],
      }),
    /receipt is invalid/,
  );
  assert.throws(
    () =>
      journal.restore({
        ...restored,
        unexpected: true,
      } as DesktopBackgroundJobsSnapshot),
    /snapshot is invalid/,
  );
});

test("observer failures cannot replace authoritative work results", async () => {
  const journal = new BackgroundJobJournal({
    now: clock("2026-07-22T04:03:10Z", "2026-07-22T04:03:11Z"),
  });
  journal.subscribe(() => {
    throw new Error("presentation observer failed");
  });

  assert.equal(
    await journal.run({ kind: "save", label: "Save alpha" }, async () => 23),
    23,
  );
  assert.equal(journal.getSnapshot().jobs[0].status, "completed");
});

test("journal bounds receipts and dismisses only finished evidence", async () => {
  const journal = new BackgroundJobJournal({
    capacity: 2,
    now: clock(
      "2026-07-22T04:04:00Z",
      "2026-07-22T04:04:01Z",
      "2026-07-22T04:04:02Z",
      "2026-07-22T04:04:03Z",
      "2026-07-22T04:04:04Z",
      "2026-07-22T04:04:05Z",
    ),
  });

  await journal.run({ kind: "save", label: "Save one" }, async () => 1);
  await journal.run({ kind: "save", label: "Save two" }, async () => 2);
  await journal.run({ kind: "save", label: "Save three" }, async () => 3);
  assert.deepEqual(
    journal.getSnapshot().jobs.map((job) => job.label),
    ["Save two", "Save three"],
  );
  assert.equal(journal.dismiss(journal.getSnapshot().jobs[0].id), true);
  assert.equal(journal.getSnapshot().jobs.length, 1);
  assert.equal(MAX_DESKTOP_BACKGROUND_JOBS, 64);
});

test("journal never evicts unfinished work when its bounded capacity is full", async () => {
  const journal = new BackgroundJobJournal({
    capacity: 1,
    now: clock("2026-07-22T04:05:00Z", "2026-07-22T04:05:01Z"),
  });
  let finish!: () => void;
  const pending = journal.run(
    { kind: "analysis", label: "Analyze the active clip" },
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );

  await assert.rejects(
    journal.run(
      { kind: "cache", label: "Generate a preview" },
      async () => undefined,
    ),
    /capacity is full/,
  );
  assert.equal(journal.getSnapshot().jobs[0].status, "running");
  finish();
  await pending;
  assert.equal(journal.getSnapshot().jobs[0].status, "completed");
});

test("real desktop project operations enter only the intended job categories", () => {
  const lifecycle = readFileSync(
    resolve(appRoot, "src/project-lifecycle.ts"),
    "utf8",
  );

  assert.match(lifecycle, /runDesktopBackgroundJob/);
  assert.match(
    lifecycle,
    /command\.kind === "save"[\s\S]{0,500}kind: "save"/,
  );
  assert.match(
    lifecycle,
    /command\.kind === "save_as"[\s\S]{0,500}kind: "save"/,
  );
  assert.match(
    lifecycle,
    /command\.kind === "restore_recovery"[\s\S]{0,500}kind: "save"/,
  );
  assert.match(
    lifecycle,
    /generateProjectMediaPreview[\s\S]{0,700}kind: "cache"/,
  );
  assert.match(
    lifecycle,
    /mutateProjectMediaContentAnalysis[\s\S]{0,700}kind: "analysis"/,
  );
  assert.match(
    lifecycle,
    /mutation\.kind === "create_or_replace"[\s\S]{0,700}kind: "proxy"/,
  );
  assert.match(
    lifecycle,
    /operation\.kind === "proxy" \|\| operation\.kind === "transcode"/,
  );
  assert.doesNotMatch(
    lifecycle,
    /setInterval|requestAnimationFrame|submit_job|poll_runtime/,
  );
});
