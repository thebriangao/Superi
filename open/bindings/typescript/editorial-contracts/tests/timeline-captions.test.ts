import assert from "node:assert/strict";
import test from "node:test";

import {
  buildCaptionImportActions,
  captionStyleFromFields,
  captionCuesFromTranscript,
  parseCaptionExchange,
  serializeCaptionExchange,
} from "../src/timeline-captions.ts";

const TIMELINE = "timeline:00000000000000000000000000000001";
const TRACK = "track:00000000000000000000000000000002";
const CLIP = "clip:00000000000000000000000000000003";

test("caption style fields publish the canonical strict API representation", () => {
  assert.deepEqual(
    captionStyleFromFields({
      fontFamily: " Inter ",
      fontSize: "42",
      foreground: "#FFFFFFFF",
      background: "#000000CC",
      bold: true,
      italic: false,
      alignment: "center",
      position: "bottom",
    }),
    {
      font_family: "Inter",
      font_size: 42,
      foreground: "#ffffffff",
      background: "#000000cc",
      bold: true,
      italic: false,
      alignment: "center",
      position: "bottom",
    },
  );
  assert.throws(
    () =>
      captionStyleFromFields({
        fontFamily: "",
        fontSize: "42.5",
        foreground: "",
        background: "",
        bold: false,
        italic: false,
        alignment: "center",
        position: "bottom",
      }),
    /whole number/i,
  );
});

test("SRT import builds one canonical caption track with explicit gaps", () => {
  const cues = parseCaptionExchange(
    [
      "1",
      "00:00:01,000 --> 00:00:02,250",
      "First line",
      "",
      "2",
      "00:00:03,000 --> 00:00:04,000",
      "Second line",
      "",
    ].join("\r\n"),
    "srt",
    "en-US",
  );
  assert.deepEqual(
    cues.map((cue) => [cue.startMilliseconds, cue.endMilliseconds, cue.text]),
    [
      [1_000, 2_250, "First line"],
      [3_000, 4_000, "Second line"],
    ],
  );

  let identity = 10;
  const result = buildCaptionImportActions({
    timelineId: TIMELINE,
    trackId: TRACK,
    trackName: "English subtitles",
    trackPosition: 2,
    language: "en-US",
    purpose: "subtitles",
    cues,
    createId(kind) {
      identity += 1;
      return `${kind}:${identity.toString(16).padStart(32, "0")}`;
    },
  });
  assert.equal(result.actions[0]?.action, "mutate_tracks");
  if (result.actions[0]?.action !== "mutate_tracks") return;
  assert.deepEqual(result.actions[0].mutations[1], {
    operation: "set_caption_semantics",
    timeline_id: TIMELINE,
    track_id: TRACK,
    language: "en-US",
    purpose: "subtitles",
  });
  assert.equal(result.actions[1]?.action, "edit_timeline");
  if (result.actions[1]?.action !== "edit_timeline") return;
  assert.deepEqual(
    result.actions[1].operations.map((operation) => {
      assert.equal(operation.operation, "append");
      return [
        operation.material.kind,
        operation.material.record_range.start.value,
        operation.material.record_range.duration.value,
      ];
    }),
    [
      ["gap", 0, 1_000],
      ["caption", 1_000, 1_250],
      ["gap", 2_250, 750],
      ["caption", 3_000, 1_000],
    ],
  );
  assert.equal(result.captionIds.length, 2);
});

test("WebVTT voice and supported presentation settings round trip", () => {
  const cues = parseCaptionExchange(
    [
      "WEBVTT",
      "",
      "cue-1",
      "00:00:00.500 --> 00:00:02.000 align:start line:0%",
      "<v Narrator>Hello &lt;world&gt; &amp; everyone</v>",
      "",
    ].join("\n"),
    "vtt",
    "en-US",
  );
  assert.equal(cues[0]?.speaker, "Narrator");
  assert.equal(cues[0]?.text, "Hello <world> & everyone");
  assert.equal(cues[0]?.style?.alignment, "start");
  assert.equal(cues[0]?.style?.position, "top");
  const encoded = serializeCaptionExchange(cues, "vtt");
  const decoded = parseCaptionExchange(encoded, "vtt", "en-US");
  assert.deepEqual(decoded, cues);
  assert.throws(
    () =>
      parseCaptionExchange(
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n<c.unsupported>text</c>\n",
        "vtt",
        "en-US",
      ),
    /unsupported cue text markup/i,
  );
});

test("fresh transcript analysis becomes ordinary editable caption cues", () => {
  const cues = captionCuesFromTranscript({
    expectedProjectRevision: 9,
    projectRevision: 9,
    currentSourceFingerprint: "source-a",
    analysisSourceFingerprint: "source-a",
    language: "en-US",
    segments: [
      {
        segment_id: "segment-1",
        text: "Language analysis result",
        start_frame: 24,
        end_frame: 72,
        rate_numerator: 24,
        rate_denominator: 1,
        speaker: "Speaker A",
        timeline_relationships: [
          { timeline_id: TIMELINE, clip_id: CLIP },
        ],
      },
    ],
  });
  assert.deepEqual(cues[0], {
    id: "segment-1",
    name: "Caption 1",
    text: "Language analysis result",
    startMilliseconds: 1_000,
    endMilliseconds: 3_000,
    language: "en-US",
    speaker: "Speaker A",
    style: null,
    timelineRelationships: [{ timeline_id: TIMELINE, clip_id: CLIP }],
  });
  const fractionalFrames = captionCuesFromTranscript({
    expectedProjectRevision: 9,
    projectRevision: 9,
    currentSourceFingerprint: "source-a",
    analysisSourceFingerprint: "source-a",
    language: "en-US",
    segments: [
      {
        segment_id: "segment-fractional",
        text: "Nearest millisecond",
        start_frame: 1,
        end_frame: 2,
        rate_numerator: 24,
        rate_denominator: 1,
        speaker: null,
        timeline_relationships: [],
      },
    ],
  });
  assert.deepEqual(
    [fractionalFrames[0]?.startMilliseconds, fractionalFrames[0]?.endMilliseconds],
    [42, 83],
  );
  assert.throws(
    () =>
      captionCuesFromTranscript({
        expectedProjectRevision: 9,
        projectRevision: 10,
        currentSourceFingerprint: "source-a",
        analysisSourceFingerprint: "source-a",
        language: "en-US",
        segments: [],
      }),
    /revision/i,
  );
  assert.throws(
    () =>
      captionCuesFromTranscript({
        expectedProjectRevision: 9,
        projectRevision: 9,
        currentSourceFingerprint: "source-b",
        analysisSourceFingerprint: "source-a",
        language: "en-US",
        segments: [],
      }),
    /stale/i,
  );
});
