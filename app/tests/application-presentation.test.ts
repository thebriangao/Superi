import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_APPLICATION_NOTIFICATIONS,
  applicationFailureFromCrashDiagnostic,
  applicationFailureFromLifecycle,
  applicationFailureFromMessage,
  applicationFailureFromProject,
  applicationFailureFromTransport,
  applicationOperationalStatus,
  applicationProgressFromEditorJob,
  applicationRecoveryPolicy,
  createApplicationNotificationState,
  placeApplicationContextMenu,
  reduceApplicationNotificationState,
} from "../src/application-presentation.ts";

test("all four recovery classes keep distinct intent and actionable guidance", () => {
  assert.deepEqual(applicationRecoveryPolicy("retryable"), {
    intent: "retry",
    label: "Retry",
    summary: "The operation can be attempted again without discarding the last-valid state.",
  });
  assert.deepEqual(applicationRecoveryPolicy("degraded"), {
    intent: "continue_degraded",
    label: "Continue",
    summary: "Unrelated work can continue while the affected capability remains visible.",
  });
  assert.deepEqual(applicationRecoveryPolicy("user_correctable"), {
    intent: "correct",
    label: "Review",
    summary: "User input or project state must be corrected before retrying.",
  });
  assert.deepEqual(applicationRecoveryPolicy("terminal"), {
    intent: "restart",
    label: "Restart",
    summary: "Preserve available work and begin a fresh application lifetime.",
  });
});

test("classified failures retain safe source context and last-valid state", () => {
  const transport = applicationFailureFromTransport(
    "editor-state",
    "Public editor state",
    17,
    {
      condition: "user_correctable",
      category: "conflict",
      code: "project_revision_conflict",
      title: "The project changed before this command completed.",
      action: "Refresh the project and apply the intended change again.",
      contexts: [
        {
          component: "superi.project",
          operation: "apply",
          fields: {
            expected_revision: { kind: "u64", value: "16" },
          },
        },
      ],
      lastValidResource: {
        resource: "superi.editor.state",
        schema_version: "1.0.0",
        identity: "project-17",
        revision: 16,
      },
    },
  );
  assert.equal(transport.condition, "user_correctable");
  assert.equal(transport.category, "conflict");
  assert.equal(transport.primaryAction.intent, "correct");
  assert.deepEqual(transport.contexts, [
    { label: "Context", value: "superi.project.apply" },
    { label: "expected_revision", value: "16" },
  ]);
  assert.deepEqual(transport.lastValidResource, {
    resource: "superi.editor.state",
    schema_version: "1.0.0",
    identity: "project-17",
    revision: 16,
  });

  const project = applicationFailureFromProject(
    "project-shell",
    "Project continuity",
    41,
    {
      class: "retryable",
      code: "project_busy",
      title: "The project is busy.",
      action: "Wait for the current operation, then retry.",
      context: { project: "project-17", operation: "save" },
    },
  );
  assert.deepEqual(project.contexts, [
    { label: "operation", value: "save" },
    { label: "project", value: "project-17" },
  ]);
  assert.equal(project.primaryAction.intent, "retry");
});

test("retained crash and lifecycle evidence remains recovery-aware across sessions", () => {
  const crash = applicationFailureFromCrashDiagnostic({
    diagnostic_id: "diagnostic-4",
    captured_unix_millis: 1_750_000_000_000,
    failure_class: "degraded",
    code: "workspace_recovered_degraded",
    title: "The prior workspace recovered with reduced capability.",
    action: "Review the retained project and continue with unaffected work.",
    contexts: [
      { component: "superi.desktop", operation: "restore_workspace" },
    ],
    continuity: {
      workspace: {
        route_id: "editing",
        hidden_panel_ids: ["application.selection"],
        focused_panel_id: "workspace.editing",
        panel_layouts: [],
      },
      project: { path: "/Projects/cut.superi", project_revision: 73 },
      lifecycle: {
        revision: 9,
        engine_generation: 3,
        application_phase: "running",
      },
    },
    recovery_entry_point: "continue_degraded",
  });
  assert.equal(crash.id, "crash:diagnostic-4");
  assert.equal(crash.primaryAction.intent, "continue_degraded");
  assert.equal(crash.primaryAction.label, "Review recovery");
  assert.ok(
    crash.contexts.some(
      (context) =>
        context.label === "Recovery entry" &&
        context.value === "continue_degraded",
    ),
  );
  assert.ok(
    crash.contexts.some(
      (context) =>
        context.label === "Workspace" && context.value === "editing",
    ),
  );
  assert.ok(
    crash.contexts.some(
      (context) =>
        context.label === "Project" &&
        context.value === "/Projects/cut.superi at revision 73",
    ),
  );

  const lifecycle = applicationFailureFromLifecycle({
    revision: 12,
    engine_generation: 4,
    application_phase: "failed",
    engine_phase: "failed",
    intent: "recover",
    engine_acknowledgement_pending: false,
    failure: {
      category: "unavailable",
      recoverability: "terminal",
      summary: "The engine must begin a fresh lifetime.",
      contexts: [
        { component: "superi.engine", operation: "initialize" },
      ],
    },
    can_retry: false,
    can_restart: true,
    can_shutdown: true,
  });
  assert.equal(lifecycle?.condition, "terminal");
  assert.equal(lifecycle?.primaryAction.intent, "restart");
  assert.ok(
    lifecycle?.contexts.some(
      (context) =>
        context.label === "Engine generation" && context.value === "4",
    ),
  );
});

test("malformed recovery values fail closed to terminal presentation", () => {
  const failure = applicationFailureFromMessage({
    id: "unknown",
    source: "Unknown service",
    revision: 0,
    condition: "future_recovery_class",
    code: "unknown_failure",
    title: "A service returned an unknown recovery class.",
    action: "Review System before continuing.",
  });
  assert.equal(failure.condition, "terminal");
  assert.equal(failure.primaryAction.intent, "restart");
});

test("notification history is immutable, deduplicated, dismissible, and bounded", () => {
  let state = createApplicationNotificationState();
  for (let index = 0; index <= MAX_APPLICATION_NOTIFICATIONS; index += 1) {
    state = reduceApplicationNotificationState(state, {
      type: "publish",
      notification: {
        id: `notice-${index}`,
        title: `Notice ${index}`,
        message: `Message ${index}`,
        tone: "information",
      },
    });
  }
  assert.equal(state.notifications.length, MAX_APPLICATION_NOTIFICATIONS);
  assert.equal(state.notifications[0].id, "notice-1");
  assert.ok(Object.isFrozen(state));
  assert.ok(Object.isFrozen(state.notifications));

  const replaced = reduceApplicationNotificationState(state, {
    type: "publish",
    notification: {
      id: "notice-12",
      title: "Updated notice",
      message: "Updated message",
      tone: "success",
    },
  });
  assert.equal(
    replaced.notifications.filter((notice) => notice.id === "notice-12").length,
    1,
  );
  assert.equal(replaced.notifications.at(-1)?.title, "Updated notice");

  const dismissed = reduceApplicationNotificationState(replaced, {
    type: "dismiss",
    id: "notice-12",
  });
  assert.equal(
    dismissed.notifications.some((notice) => notice.id === "notice-12"),
    false,
  );
});

test("public export jobs produce truthful determinate and indeterminate progress", () => {
  const determinate = applicationProgressFromEditorJob({
    job_id: "job:export-17",
    status: "running",
    attempt: 2,
    progress_revision: 11,
    completed_units: 7,
    total_units: 10,
    elapsed: { seconds: 4, nanoseconds: 250_000_000 },
    dependencies: [],
    failure: null,
    dependency_failures: [],
    has_result: false,
    retry_allowed: false,
    is_final: false,
  });
  assert.deepEqual(determinate, {
    id: "export:job:export-17",
    label: "Export job:export-17",
    status: "running",
    detail: "Attempt 2, progress revision 11",
    completed: 7,
    total: 10,
    percent: 70,
    active: true,
    failureCondition: null,
  });

  const indeterminate = applicationProgressFromEditorJob({
    job_id: "job:analysis-2",
    status: "queued",
    attempt: 1,
    progress_revision: 0,
    completed_units: 0,
    total_units: null,
    elapsed: null,
    dependencies: ["job:source-1"],
    failure: null,
    dependency_failures: [],
    has_result: false,
    retry_allowed: false,
    is_final: false,
  });
  assert.equal(indeterminate.total, null);
  assert.equal(indeterminate.percent, null);
  assert.equal(indeterminate.active, true);

  const bounded = applicationProgressFromEditorJob({
    job_id: "job:bounded",
    status: "running",
    attempt: 1,
    progress_revision: 1,
    completed_units: 19,
    total_units: 10,
    elapsed: null,
    dependencies: [],
    failure: null,
    dependency_failures: [],
    has_result: false,
    retry_allowed: false,
    is_final: false,
  });
  assert.equal(bounded.completed, 10);
  assert.equal(bounded.percent, 100);
});

test("application status prioritizes attention, degradation, progress, and continuity", () => {
  const retryable = applicationFailureFromMessage({
    id: "retry",
    source: "Editor",
    revision: 1,
    condition: "retryable",
    code: "temporary",
    title: "Refresh failed.",
    action: "Retry the refresh.",
  });
  const terminal = applicationFailureFromMessage({
    id: "terminal",
    source: "Engine",
    revision: 2,
    condition: "terminal",
    code: "engine_stopped",
    title: "The engine stopped.",
    action: "Restart Superi.",
  });
  assert.equal(
    applicationOperationalStatus({
      failures: [retryable],
      progress: [],
      workspaceContinuity: "saved",
      engineLabel: "Ready",
    }).condition,
    "degraded",
  );
  assert.equal(
    applicationOperationalStatus({
      failures: [terminal],
      progress: [],
      workspaceContinuity: "saved",
      engineLabel: "Needs attention",
    }).condition,
    "attention",
  );
  assert.equal(
    applicationOperationalStatus({
      failures: [],
      progress: [
        {
          id: "progress",
          label: "Saving workspace",
          status: "running",
          detail: "Preserving panel intent",
          completed: 0,
          total: null,
          percent: null,
          active: true,
          failureCondition: null,
        },
      ],
      workspaceContinuity: "saving",
      engineLabel: "Ready",
    }).condition,
    "working",
  );
  assert.equal(
    applicationOperationalStatus({
      failures: [],
      progress: [],
      workspaceContinuity: "saved",
      engineLabel: "Ready",
    }).condition,
    "ready",
  );
});

test("context menus remain inside the visible viewport", () => {
  assert.deepEqual(
    placeApplicationContextMenu({
      x: 980,
      y: 740,
      menuWidth: 260,
      menuHeight: 320,
      viewportWidth: 1_024,
      viewportHeight: 768,
    }),
    { left: 756, top: 440 },
  );
  assert.deepEqual(
    placeApplicationContextMenu({
      x: -40,
      y: -10,
      menuWidth: 200,
      menuHeight: 100,
      viewportWidth: 1_024,
      viewportHeight: 768,
    }),
    { left: 8, top: 8 },
  );
});
