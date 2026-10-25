// The two read-only surfaces the package QuickPicks are built from — the
// folder registry's `GET /v1/packages` listing
// ([PACKAGE-FOLDER-REGISTRY]: `{"packages":[{"key","releases":[…]}]}`) and the
// direct `[dependencies]` lines of `osprey.toml` ([PACKAGE-MANIFEST]:
// `"@scope/name" = { epoch = N, min_release = M }`). Reading is all the
// editor ever does to them; every mutation goes through the Osprey CLI.
//
// The listing carries NO security-audit evidence (spec 0045's audit service
// is not deployed), so an entry with no evidence must never be presented as
// safe: every QuickPick item says so.

/** One release row of the listing: `{"digest","epoch","ordinal"}`. */
export interface ReleaseRow {
  readonly digest: string;
  readonly epoch: number;
  readonly ordinal: number;
}

/** One org-qualified package with its releases, in listing order. */
export interface PackageEntry {
  readonly key: string;
  readonly releases: readonly ReleaseRow[];
}

/**
 * `@org/name` and nothing else: one non-empty `@`, one `/`, both sides
 * non-empty (spec 0045 — leaf names are reusable across orgs, unscoped names
 * are rejected). The listing itself is untrusted input, so entries are only
 * ever surfaced after passing this.
 */
export function isOrgQualifiedName(key: string): boolean {
  const component = "[a-z0-9](?:[a-z0-9-]*[a-z0-9])?";
  const parts = new RegExp(`^@(${component})/(${component})$`).exec(key);
  return !!parts && parts[1].length <= 39 && parts[2].length <= 63;
}

function rowOf(item: any): ReleaseRow | undefined {
  return typeof item?.digest === "string" &&
    Number.isSafeInteger(item.epoch) && item.epoch >= 1 &&
    Number.isSafeInteger(item.ordinal) && item.ordinal >= 0
    ? { digest: item.digest, epoch: item.epoch, ordinal: item.ordinal }
    : undefined;
}

/**
 * Every org-qualified entry of a listing, or undefined when the text is not
 * the listing shape. A package item that is malformed or unscoped is skipped
 * — an entry the editor cannot validate is never offered, let alone
 * presented as safe.
 */
export function parseListing(text: string): PackageEntry[] | undefined {
  let parsed: any;
  try {
    parsed = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (!Array.isArray(parsed?.packages)) {
    return undefined;
  }
  const entries: PackageEntry[] = [];
  for (const item of parsed.packages) {
    if (typeof item?.key !== "string" || !isOrgQualifiedName(item.key) || !Array.isArray(item.releases)) {
      continue;
    }
    const releases = (item.releases as any[]).map((row: any) => rowOf(row)).filter((row): row is ReleaseRow => row !== undefined);
    if (releases.length > 0) {
      entries.push({ key: item.key, releases });
    }
  }
  return entries;
}

/** The identity of `entries` — the pick list is the listing, nothing derived. */
export function listingOf(entries: readonly PackageEntry[] | undefined): readonly PackageEntry[] {
  return entries ?? [];
}

/** The newest release of an entry: highest epoch, then highest ordinal. */
export function newestRelease(entry: PackageEntry): ReleaseRow {
  return entry.releases.reduce((best, row) =>
    row.epoch > best.epoch || (row.epoch === best.epoch && row.ordinal > best.ordinal) ? row : best,
  );
}

/**
 * The QuickPick detail every entry gets. The registry listing carries no
 * audit evidence, so the honest label is a warning — never "safe",
 * "verified" or "trusted".
 */
export function securityDetail(_entry: PackageEntry): string {
  return "No security audit evidence is available for this release — treat it as unverified.";
}

/** Validated direct dependency identities reported by the Osprey manifest parser. */
export function dependencyKeys(report: { status: number; lines: readonly string[] }): string[] | undefined {
  if (report.status !== 0 || !report.lines.every(isOrgQualifiedName)) return undefined;
  return [...new Set(report.lines)];
}
