import assert from "node:assert/strict";
import test from "node:test";
import {
  INTERNATIONALIZATION_SCHEMA_VERSION,
  applicationTextDirection,
  formatLocaleDate,
  formatLocaleNumber,
  formatLocaleTimecode,
  resolveApplicationLocale,
} from "../src/internationalization.ts";

test("locale resolution and text direction fail safely", () => {
  assert.equal(INTERNATIONALIZATION_SCHEMA_VERSION, 1);
  assert.equal(resolveApplicationLocale("fr-FR", ["en-US"]), "fr-FR");
  assert.equal(resolveApplicationLocale("bad_locale", ["ar"]), "ar");
  assert.equal(resolveApplicationLocale("", ["bad_locale"]), "en");
  assert.equal(applicationTextDirection("ar"), "rtl");
  assert.equal(applicationTextDirection("he-IL"), "rtl");
  assert.equal(applicationTextDirection("ja-JP"), "ltr");
});

test("numbers, dates, and timecode localize display without changing timing", () => {
  assert.equal(formatLocaleNumber(1234.5, "en-US"), "1,234.5");
  assert.match(formatLocaleNumber(1234.5, "de-DE"), /1\.234,5/);
  assert.ok(formatLocaleDate(Date.UTC(2026, 6, 21, 12), "en-US").length > 5);
  assert.equal(formatLocaleTimecode([1, 2, 3, 4], "en-US"), "01:02:03:04");
  assert.equal(formatLocaleTimecode([1, 2, 3, 4], "en-US", true), "01;02;03;04");
  assert.throws(() => formatLocaleTimecode([1, -1, 3, 4], "en"), /nonnegative/);
});
