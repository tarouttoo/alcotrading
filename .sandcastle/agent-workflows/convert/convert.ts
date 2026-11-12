import * as fs from "node:fs";
import { execFileSync } from "node:child_process";
import { fail, required } from "../shared/common";
import {
  CONVERT_LABEL,
  HANDOFF_LABEL,
  decide,
  mergeBlockers,
  parseBlockedBy,
  parseNativeIssues,
  renderWaitingComment,
  type BlockerState,
} from "./blockers";

/**
 * Deterministic converter step for the ready-for-agent label. No agent runs
 * here: blocking edges come from the tracker's native relationships where
 * available (falling back to the body's "## Blocked by" section, parsed in
 * `blockers.ts` — never the shell), and every GitHub write goes through the
 * fixed vocabulary in `blockers.ts`. Issue bodies are untrusted input: they
 * reach this script as JSON and enter decision logic only through the
 * parsers.
 *
 * Triggered by `agent-convert.yml`: a `ready-for-agent` label event
 * evaluates that Ticket; an issue closing re-scans the Tickets it was
 * blocking, so the last blocker closing converts the waiting Ticket — no
 * schedule or polling.
 */

const GH_REPO = required("GH_REPO");
const EVENT_ACTION = required("EVENT_ACTION");
const ISSUE_NUMBER = Number(required("ISSUE_NUMBER"));
const AGENT_PAT = process.env.AGENT_PAT ?? "";

const summary = (markdown: string): void => {
  const file = process.env.GITHUB_STEP_SUMMARY;
  if (file) {
    fs.appendFileSync(file, `${markdown}\n`);
  } else {
    console.log(markdown);
  }
};

const ghWith = (args: string[], options: { token?: string; input?: string }) =>
  execFileSync("gh", args, {
    encoding: "utf8",
    input: options.input,
    stdio: ["pipe", "pipe", "pipe"],
    env: options.token
      ? { ...process.env, GH_TOKEN: options.token }
      : undefined,
  });

const ghJson = (args: string[]): unknown => JSON.parse(ghWith(args, {}));

interface LiveIssue {
  readonly state: string;
  readonly labels: readonly string[];
  readonly body: string;
}

const labelNames = (raw: unknown): string[] =>
  Array.isArray(raw)
    ? raw.flatMap((label) => {
        if (typeof label !== "object" || label === null) return [];
        const name = (label as Record<string, unknown>).name;
        return typeof name === "string" ? [name] : [];
      })
    : [];

const fetchIssue = (number: number): LiveIssue | null => {
  try {
    const raw = ghJson([
      "api",
      `repos/${GH_REPO}/issues/${String(number)}`,
    ]) as Record<string, unknown>;
    return {
      state: typeof raw.state === "string" ? raw.state : "",
      labels: labelNames(raw.labels),
      body: typeof raw.body === "string" ? raw.body : "",
    };
  } catch {
    return null;
  }
};

/**
 * Native blockers of an issue, all pages (the endpoint caps at 30 per page
 * and a truncated list would silently drop an open blocker); null when the
 * dependencies API is unavailable.
 */
const nativeBlockers = (
  number: number,
): { number: number; state: BlockerState }[] | null => {
  try {
    return parseNativeIssues(
      ghJson([
        "api",
        "--paginate",
        `repos/${GH_REPO}/issues/${String(number)}/dependencies/blocked_by`,
      ]),
    );
  } catch {
    return null;
  }
};

/** Native dependents (issues the closed issue blocks, all pages); null on failure. */
const nativeDependents = (number: number): number[] | null => {
  try {
    return parseNativeIssues(
      ghJson([
        "api",
        "--paginate",
        `repos/${GH_REPO}/issues/${String(number)}/dependencies/blocking`,
      ]),
    ).map((issue) => issue.number);
  } catch {
    return null;
  }
};

const stateCache = new Map<number, BlockerState>();

/** Fetched state of a blocker; undefined when it cannot be read (→ open). */
const blockerState = (number: number): BlockerState | undefined => {
  const cached = stateCache.get(number);
  if (cached) return cached;
  const issue = fetchIssue(number);
  if (!issue) return undefined;
  const state: BlockerState = issue.state === "closed" ? "closed" : "open";
  stateCache.set(number, state);
  return state;
};

/**
 * Tickets to evaluate: the labelled issue itself, or — on an issue closing —
 * its dependents: the native blocking list plus every open
 * `ready-for-agent` Ticket whose parsed "## Blocked by" section names it.
 */
const candidates = (): number[] => {
  if (EVENT_ACTION === "labeled") return [ISSUE_NUMBER];
  const dependents = new Set(nativeDependents(ISSUE_NUMBER) ?? []);
  try {
    const raw = ghJson([
      "issue",
      "list",
      "--state",
      "open",
      "--label",
      CONVERT_LABEL,
      "--limit",
      "1000",
      "--json",
      "number,body",
    ]);
    for (const entry of Array.isArray(raw) ? raw : []) {
      if (typeof entry !== "object" || entry === null) continue;
      const record = entry as Record<string, unknown>;
      const number = record.number;
      const body = record.body;
      if (
        typeof number === "number" &&
        typeof body === "string" &&
        parseBlockedBy(body).includes(ISSUE_NUMBER)
      ) {
        dependents.add(number);
      }
    }
  } catch {
    // The native dependents list is already in; scanning is additive.
  }
  return [...dependents].sort((a, b) => a - b);
};

const describeBlockers = (numbers: readonly number[]): string =>
  numbers.length === 0 ? "—" : numbers.map((n) => `#${n}`).join(", ");

const main = (): void => {
  if (EVENT_ACTION !== "labeled" && EVENT_ACTION !== "closed") {
    fail(`Unsupported event action: ${EVENT_ACTION}`);
  }
  if (!Number.isInteger(ISSUE_NUMBER) || ISSUE_NUMBER <= 0) {
    fail(`Invalid issue number: ${ISSUE_NUMBER}`);
  }

  const numbers = candidates();
  if (numbers.length === 0) {
    summary(
      [
        "## ready-for-agent converter",
        "",
        `No \`${CONVERT_LABEL}\` Tickets depend on #${ISSUE_NUMBER}; nothing to convert.`,
      ].join("\n"),
    );
    console.log(`No dependents of #${ISSUE_NUMBER} to convert.`);
    return;
  }

  const rows: string[] = [];
  for (const number of numbers) {
    const issue = fetchIssue(number);
    if (issue === null) {
      // On a `labeled` event this is the only candidate and there may be no
      // future `closed` pulse for it, so a read failure must not pass as a
      // skip — that would silently stall the Ticket. Fail instead; the
      // retry is a label cycle (remove and re-add `ready-for-agent`).
      if (EVENT_ACTION === "labeled") {
        fail(
          `Could not read #${number}; cannot evaluate the ${CONVERT_LABEL} conversion. Remove and re-add the label to retry.`,
        );
      }
      rows.push(`| #${number} | — | skipped: could not be read |`);
      console.log(`Skipped #${number}: could not be read.`);
      continue;
    }
    if (issue.state !== "open" || !issue.labels.includes(CONVERT_LABEL)) {
      rows.push(
        `| #${number} | — | skipped: no longer open with \`${CONVERT_LABEL}\` |`,
      );
      console.log(`Skipped #${number}: no longer carries ${CONVERT_LABEL}.`);
      continue;
    }

    const blockers = mergeBlockers(
      nativeBlockers(number) ?? [],
      parseBlockedBy(issue.body),
      blockerState,
    );
    const effect = decide(number, blockers);

    if (effect.kind === "convert") {
      if (!AGENT_PAT) {
        console.log(
          `::warning::AGENT_PAT is not set; adding ${HANDOFF_LABEL} to #${number} with GITHUB_TOKEN will not trigger Agent Implement.`,
        );
      }
      // A label added with GITHUB_TOKEN does not trigger other workflows, so
      // the hand-off must go through AGENT_PAT (the PAT cascade).
      try {
        ghWith(
          [
            "issue",
            "edit",
            String(number),
            "--remove-label",
            CONVERT_LABEL,
            "--add-label",
            HANDOFF_LABEL,
          ],
          { token: AGENT_PAT },
        );
      } catch (error) {
        // One push closing several blockers of the same Ticket fires parallel
        // `closed` runs with different concurrency groups, and both can reach
        // this swap. The loser's --remove-label 404s on a label the winner
        // already removed — tolerate exactly that race by verifying the
        // outcome, and fail loudly on anything else.
        const fresh = fetchIssue(number);
        const alreadyConverted =
          fresh !== null &&
          fresh.labels.includes(HANDOFF_LABEL) &&
          !fresh.labels.includes(CONVERT_LABEL);
        if (!alreadyConverted) throw error;
        console.log(
          `#${number} was already converted by a concurrent run; duplicate swap skipped.`,
        );
      }
      rows.push(
        `| #${number} | — | \`${CONVERT_LABEL}\` → \`${HANDOFF_LABEL}\` |`,
      );
      console.log(`Converted #${number} to ${HANDOFF_LABEL}.`);
    } else {
      const comment = renderWaitingComment(effect);
      ghWith(["issue", "comment", String(number), "--body-file", "-"], {
        input: comment,
      });
      rows.push(
        `| #${number} | ${describeBlockers(effect.openBlockers)} | waiting — commented, \`${CONVERT_LABEL}\` kept |`,
      );
      console.log(
        `Waiting on #${effect.openBlockers.join(", #")} for #${number}.`,
      );
    }
  }

  summary(
    [
      "## ready-for-agent converter",
      "",
      "| Ticket | Open blockers | Outcome |",
      "| --- | --- | --- |",
      ...rows,
    ].join("\n"),
  );
};

try {
  main();
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
