export const INTERNATIONALIZATION_SCHEMA_VERSION = 1;

const RTL_LANGUAGES = new Set(["ar", "dv", "fa", "he", "ku", "ps", "ur", "yi"]);

export function resolveApplicationLocale(
  declared: string,
  preferred: readonly string[],
): string {
  const candidates = [declared.trim(), ...preferred].filter(Boolean);
  for (const candidate of candidates) {
    try {
      const locale = new Intl.Locale(candidate);
      if (Intl.NumberFormat.supportedLocalesOf([locale.baseName]).length > 0) {
        return locale.baseName;
      }
    } catch {
      continue;
    }
  }
  return "en";
}

export function applicationTextDirection(locale: string): "ltr" | "rtl" {
  const language = new Intl.Locale(locale).language.toLowerCase();
  return RTL_LANGUAGES.has(language) ? "rtl" : "ltr";
}

export function formatLocaleNumber(value: number, locale: string): string {
  if (!Number.isFinite(value)) throw new Error("locale number must be finite");
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 3 }).format(value);
}

export function formatLocaleDate(value: Date | number, locale: string): string {
  const date = value instanceof Date ? value : new Date(value);
  if (!Number.isFinite(date.getTime())) throw new Error("locale date must be valid");
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

export function formatLocaleTimecode(
  parts: readonly [number, number, number, number],
  locale: string,
  dropFrame = false,
): string {
  if (parts.some((part) => !Number.isInteger(part) || part < 0)) {
    throw new Error("timecode parts must be nonnegative integers");
  }
  const digits = new Intl.NumberFormat(locale, {
    minimumIntegerDigits: 2,
    useGrouping: false,
  });
  return parts.map((part) => digits.format(part)).join(dropFrame ? ";" : ":");
}

export function applyApplicationLocale(
  root: HTMLElement,
  preferred: readonly string[],
): string {
  const locale = resolveApplicationLocale(root.lang, preferred);
  root.lang = locale;
  root.dir = applicationTextDirection(locale);
  return locale;
}
