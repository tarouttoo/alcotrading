import { asArray } from "../shared/common";

/**
 * Pure model of the ready-for-agent converter: the blocking edges a Ticket
 * declares (native tracker relationships plus the body's "## Blocked by"
 * section), the decision those edges force, and the effect it turns into.
 * No I/O — `convert.ts` owns that. Issue bodies are untrusted input: they
 * enter only through `parseBlockedBy`.
 */

export const CONVERT_LABEL = "ready-for-agent";
export const HANDOFF_LABEL = "agent:implement";

export type BlockerState = "open" | "closed";

export interface Blocker {
  readonly number: number;
  readonly state: BlockerState;
}

// ---------------------------------------------------------------------------
// Parsing primitives
// ---------------------------------------------------------------------------

const asIssueNumber = (value: unknown, label: string): number => {
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return value;
};

/**
 * Anything that is not exactly GitHub's `"closed"` counts as open: the
 * converter may only fire when every blocker is provably closed.
 */
const asBlockerState = (value: unknown): BlockerState =>
  value === "closed" ? "closed" : "open";

/** Parse a native `dependencies/blocked_by` or `dependencies/blocking` response. */
export const parseNativeIssues = (
  raw: unknown,
): { number: number; state: BlockerState }[] =>
  asArray(raw, "dependencies response").map((value) => {
    if (typeof value !== "object" || value === null) {
      throw new Error("dependency entry must be an object");
    }
    const record = value as Record<string, unknown>;
    return {
      number: asIssueNumber(record.number, "dependency number"),
      state: asBlockerState(record.state),
    };
  });

// ---------------------------------------------------------------------------
// "## Blocked by" section (untrusted markdown)
// ---------------------------------------------------------------------------

const stripFences = (body: string): string =>
  body.replace(/^(```|~~~)[^\n]*\n[\s\S]*?^\1[^\n]*$/gm, "");

const HEADING = /^#{2,}\s+blocked by\b[^\S\n]*/i;
const ISSUE_REF = /(^|[\s(\[*])#(\d+)\b/g;
const LIST_ITEM = /^[-*+]\s+/;
const BARE_REF = /^#\d/;

/**
 * Parse the `## Blocked by` section of an issue body into blocker numbers.
 *
 * Only list items and bare reference lines count, so prose that merely
 * mentions an issue inside the section is ignored; every issue an edge line
 * references counts (a spurious extra edge only delays a conversion, while
 * a missed one would fire it early). The section ends at the next heading,
 * and fenced code blocks are stripped first. Deduped, ascending.
 */
export const parseBlockedBy = (body: string): number[] => {
  const numbers = new Set<number>();
  let inSection = false;
  for (const line of stripFences(body).split(/\r?\n/)) {
    if (/^#{1,6}\s/.test(line)) {
      inSection = HEADING.test(line);
      continue;
    }
    if (!inSection) continue;
    const trimmed = line.trim();
    if (!LIST_ITEM.test(trimmed) && !BARE_REF.test(trimmed)) continue;
    for (const match of trimmed.matchAll(ISSUE_REF)) {
      const number = Number(match[2]);
      if (number >= 1) numbers.add(number);
    }
  }
  return [...numbers].sort((a, b) => a - b);
};

// ---------------------------------------------------------------------------
// Blocking edges and the decision
// ---------------------------------------------------------------------------

/**
 * Union the native edges with the body-parsed edges. Native states win on
 * shared numbers; body-only blockers take their fetched state, and an
 * unknown state counts as open.
 */
export const mergeBlockers = (
  native: readonly { number: number; state: BlockerState }[],
  fromBody: readonly number[],
  stateOf: (number: number) => BlockerState | undefined,
): Blocker[] => {
  const byNumber = new Map<number, Blocker>();
  for (const blocker of native) byNumber.set(blocker.number, blocker);
  for (const number of fromBody) {
    if (!byNumber.has(number)) {
      byNumber.set(number, {
        number,
        state: stateOf(number) ?? "open",
      });
    }
  }
  return [...byNumber.values()].sort((a, b) => a.number - b.number);
};

export type ConvertEffect =
  | {
      readonly kind: "convert";
      readonly issue: number;
    }
  | {
      readonly kind: "waiting";
      readonly issue: number;
      readonly openBlockers: readonly number[];
    };

/** Convert when every blocker is closed (or none declared); otherwise wait. */
export const decide = (
  issue: number,
  blockers: readonly Blocker[],
): ConvertEffect => {
  const openBlockers = blockers
    .filter((blocker) => blocker.state === "open")
    .map((blocker) => blocker.number);
  return openBlockers.length === 0
    ? { kind: "convert", issue }
    : { kind: "waiting", issue, openBlockers };
};

/** The comment a waiting ticket gets, naming its open blockers. */
export const renderWaitingComment = (
  effect: Extract<ConvertEffect, { kind: "waiting" }>,
): string =>
  [
    `\`${CONVERT_LABEL}\` converter: this Ticket still has open blockers:`,
    "",
    ...effect.openBlockers.map((number) => `- #${number}`),
    "",
    `Keeping \`${CONVERT_LABEL}\`. When the last blocker closes, this run re-scans its dependents and swaps the label for \`${HANDOFF_LABEL}\`, which starts the implement workflow.`,
  ].join("\n");
