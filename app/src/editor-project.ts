import type {
  EditorAudioTrackState,
  EditorStateSnapshot,
  GetEditorState,
} from "./api.ts";
import type { DesktopTransportFailure } from "./transport.ts";

export const EDITOR_WORKSPACE_IDS = Object.freeze([
  "editing",
  "compositing",
  "color",
  "audio",
  "delivery",
] as const);

export type EditorWorkspaceId = (typeof EDITOR_WORKSPACE_IDS)[number];

export type EditorProjectStatus =
  | "loading"
  | "refreshing"
  | "ready"
  | "degraded"
  | "failed"
  | "unavailable";

export interface EditorProjectPresentation {
  readonly status: EditorProjectStatus;
  readonly transactionId: string | null;
  readonly commandSequence: number | null;
  readonly snapshot: EditorStateSnapshot | null;
  readonly failure: DesktopTransportFailure | null;
}

export const INITIAL_EDITOR_PROJECT: EditorProjectPresentation = Object.freeze({
  status: "loading",
  transactionId: null,
  commandSequence: null,
  snapshot: null,
  failure: null,
});

export function createEditorStateRequest(
  transactionIdentity: string,
): GetEditorState {
  if (transactionIdentity.trim().length === 0) {
    throw new Error("editor transaction identity must not be empty");
  }
  return { transaction_id: transactionIdentity };
}

export function projectAudioTrack(
  track: EditorAudioTrackState,
): EditorAudioTrackState {
  return deepFreeze(structuredClone(track));
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) {
    return value;
  }
  for (const child of Object.values(value as Record<string, unknown>)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}
