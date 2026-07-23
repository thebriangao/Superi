/**
 * Portable type projections consumed by the preserved editorial planners.
 *
 * Runtime ownership lives in superi-session and the public API. This file has no host calls.
 */

export interface MediaTimelineRelationship {
  readonly timeline_id: string;
  readonly clip_id: string | null;
}

export interface MediaTranscriptSegment {
  readonly segment_id: string;
  readonly text: string;
  readonly start_frame: number;
  readonly end_frame: number;
  readonly rate_numerator: number;
  readonly rate_denominator: number;
  readonly speaker: string | null;
  readonly timeline_relationships: readonly MediaTimelineRelationship[];
}

export interface SourceMonitorTime {
  readonly value: number;
  readonly timebase_numerator: number;
  readonly timebase_denominator: number;
}

export interface SourceMonitorMarks {
  readonly source_fingerprint: string | null;
  readonly in_mark: SourceMonitorTime | null;
  readonly out_mark: SourceMonitorTime | null;
}

export interface SourceMonitorStream {
  readonly stream_id: number;
  readonly kind: string;
  readonly codec: string;
  readonly timebase_numerator: number;
  readonly timebase_denominator: number;
}

export interface SourceMonitorSnapshot {
  readonly monitor_revision: number;
  readonly engine_state: "empty" | "ready" | "stale";
  readonly project_id: string | null;
  readonly project_revision: number | null;
  readonly library_revision: number | null;
  readonly media_id: string | null;
  readonly media_name: string | null;
  readonly source_fingerprint: string | null;
  readonly opened_fingerprint: string | null;
  readonly backend_id: string | null;
  readonly container_id: string | null;
  readonly stream: SourceMonitorStream | null;
  readonly current: SourceMonitorTime | null;
  readonly duration: SourceMonitorTime | null;
  readonly range_start: SourceMonitorTime | null;
  readonly range_end: SourceMonitorTime | null;
  readonly marks: SourceMonitorMarks;
  readonly marks_fresh: boolean;
  readonly presentation_note: string;
}
