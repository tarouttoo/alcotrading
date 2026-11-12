import { describe, expect, it } from "vitest";
import {
  CONVERT_LABEL,
  HANDOFF_LABEL,
  decide,
  mergeBlockers,
  parseBlockedBy,
  parseNativeIssues,
  renderWaitingComment,
  type Blocker,
} from "./blockers";

describe("labels", () => {
  it("converts the ready-for-agent label into agent:implement", () => {
    expect(CONVERT_LABEL).toBe("ready-for-agent");
    expect(HANDOFF_LABEL).toBe("agent:implement");
  });
});

describe("parseBlockedBy", () => {
  it("returns nothing for a body without a Blocked by section", () => {
    expect(parseBlockedBy("Just a bug report.\n\n## Steps\n\n1. click")).toEqual(
      [],
    );
  });

  it("parses a single bullet", () => {
    expect(parseBlockedBy("## Blocked by\n\n- #8")).toEqual([8]);
  });

  it("parses one number per bullet, sorted and deduped", () => {
    expect(
      parseBlockedBy("## Blocked by\n\n- #9\n- #8\n- #8 again"),
    ).toEqual([8, 9]);
  });

  it("ignores description text after the numbers on a bullet", () => {
    expect(
      parseBlockedBy("## Blocked by\n\n- #8 — the preflight must admit it"),
    ).toEqual([8]);
  });

  it("matches the heading case-insensitively and with a colon", () => {
    expect(parseBlockedBy("## blocked by:\n\n- #8")).toEqual([8]);
  });

  it("stops at the next heading", () => {
    expect(
      parseBlockedBy("## Blocked by\n\n- #8\n\n## Acceptance criteria\n\n- [ ] #99"),
    ).toEqual([8]);
  });

  it("ignores Blocked by sections inside fenced code blocks", () => {
    expect(
      parseBlockedBy(
        "```\n## Blocked by\n\n- #999\n```\n\n## Blocked by\n\n- #8",
      ),
    ).toEqual([8]);
  });

  it("handles CRLF line endings", () => {
    expect(parseBlockedBy("## Blocked by\r\n\r\n- #8\r\n- #9")).toEqual([8, 9]);
  });

  it("reads bare and bolded references", () => {
    expect(parseBlockedBy("## Blocked by\n\n#12\n\n- **#13**")).toEqual([
      12, 13,
    ]);
  });

  it("ignores prose lines that merely mention an issue", () => {
    expect(
      parseBlockedBy("## Blocked by\n\nWaiting on the outcome of #55."),
    ).toEqual([]);
  });

  it("ignores invalid issue numbers", () => {
    expect(parseBlockedBy("## Blocked by\n\n- #0")).toEqual([]);
  });

  it("returns nothing for an empty section", () => {
    expect(parseBlockedBy("## Blocked by\n\n-")).toEqual([]);
  });
});

describe("parseNativeIssues", () => {
  it("reads numbers and states from a dependencies response", () => {
    expect(
      parseNativeIssues([
        { number: 8, state: "closed" },
        { number: 3, state: "open" },
      ]),
    ).toEqual([
      { number: 8, state: "closed" },
      { number: 3, state: "open" },
    ]);
  });

  it("defaults a missing state to open", () => {
    expect(parseNativeIssues([{ number: 5 }])).toEqual([
      { number: 5, state: "open" },
    ]);
  });

  it("rejects a non-array response", () => {
    expect(() => parseNativeIssues({ message: "Not Found" })).toThrow();
  });

  it("rejects entries without a positive integer number", () => {
    expect(() => parseNativeIssues([{ number: 0 }])).toThrow();
    expect(() => parseNativeIssues([{ number: "8" }])).toThrow();
  });
});

describe("mergeBlockers", () => {
  const closed = (number: number): Blocker => ({ number, state: "closed" });
  const open = (number: number): Blocker => ({ number, state: "open" });

  it("unions native edges with body-parsed edges, deduped and sorted", () => {
    expect(
      mergeBlockers([closed(9), open(8)], [8, 2], () => "closed"),
    ).toEqual([closed(2), open(8), closed(9)]);
  });

  it("keeps the native state for shared edges", () => {
    expect(
      mergeBlockers([closed(8)], [8], () => "open"),
    ).toEqual([closed(8)]);
  });

  it("treats a body-only blocker of unknown state as open", () => {
    expect(mergeBlockers([], [8], () => undefined)).toEqual([open(8)]);
  });
});

describe("decide", () => {
  const closed = (number: number): Blocker => ({ number, state: "closed" });
  const open = (number: number): Blocker => ({ number, state: "open" });

  it("converts a ticket with no blockers", () => {
    expect(decide(12, [])).toEqual({ kind: "convert", issue: 12 });
  });

  it("converts a ticket whose blockers are all closed", () => {
    expect(decide(12, [closed(8), closed(9)])).toEqual({
      kind: "convert",
      issue: 12,
    });
  });

  it("waits, naming the open blockers, when any is open", () => {
    expect(decide(12, [closed(8), open(9)])).toEqual({
      kind: "waiting",
      issue: 12,
      openBlockers: [9],
    });
  });
});

describe("renderWaitingComment", () => {
  it("names the open blockers and the labels involved", () => {
    const comment = renderWaitingComment({
      kind: "waiting",
      issue: 12,
      openBlockers: [9, 8],
    });
    expect(comment).toContain("#8");
    expect(comment).toContain("#9");
    expect(comment).toContain("`ready-for-agent`");
    expect(comment).toContain("`agent:implement`");
  });
});
