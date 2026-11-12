import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const CONVERT_TS = new URL("./convert.ts", import.meta.url).pathname;
const TSX = new URL("../../../node_modules/.bin/tsx", import.meta.url).pathname;

interface Fixture {
  state: "open" | "closed";
  labels?: string[];
  body?: string;
}

interface RunOptions {
  env: Record<string, string>;
  /** Issue fixtures served for `gh api repos/.../issues/N`, keyed by number. */
  issues?: Record<string, Fixture>;
  /** Response for the native `dependencies/blocked_by` call. */
  blockedBy?: string;
  /** Exit code to force on the native `dependencies/blocked_by` call. */
  blockedByExit?: string;
  /** Response for the native `dependencies/blocking` call. */
  blocking?: string;
  /** Response body for `gh issue list --label ready-for-agent --json number,body`. */
  readyTickets?: string;
  /** Issue fixture swapped in when a forced edit failure occurs (race simulation: a concurrent run's completed swap). */
  editFixture?: Fixture;
}

interface RunResult {
  status: number;
  stdout: string;
  /** Lines the stub `gh` logged for every write (comment/edit) it served. */
  log: string;
}

/**
 * Runs convert.ts the way the workflow step does, with a stub `gh` first on
 * PATH serving JSON fixtures and logging every write it is asked to make.
 */
const runConvert = (options: RunOptions): RunResult => {
  const dir = mkdtempSync(join(tmpdir(), "agent-convert-"));
  try {
    const stubBin = join(dir, "bin");
    mkdirSync(stubBin, { recursive: true });
    writeFileSync(
      join(stubBin, "gh"),
      [
        "#!/usr/bin/env bash",
        'args="$*"',
        'case "$args" in',
        '  *"issue list"*"--label ready-for-agent"*) cat "$STUB_READY_TICKETS" ;;',
        '  *"dependencies/blocked_by"*)',
        '    echo "api $* GH_TOKEN=$GH_TOKEN" >> "$STUB_LOG"',
        '    if [ -n "${STUB_BLOCKED_BY_EXIT:-}" ]; then exit "$STUB_BLOCKED_BY_EXIT"; fi',
        '    echo "${STUB_BLOCKED_BY:-[]}" ;;',
        '  *"dependencies/blocking"*)',
        '    echo "api $* GH_TOKEN=$GH_TOKEN" >> "$STUB_LOG"',
        '    echo "${STUB_BLOCKING:-[]}" ;;',
        '  *"api "*issues/*)',
        '    echo "api $* GH_TOKEN=$GH_TOKEN" >> "$STUB_LOG"',
        '    n=$(echo "$args" | grep -o "issues/[0-9]*" | head -1 | cut -d/ -f2)',
        '    cat "$STUB_ISSUES_DIR/issue-$n.json" ;;',
        '  *"issue edit"*)',
        '    echo "edit $* GH_TOKEN=$GH_TOKEN" >> "$STUB_LOG"',
        '    echo "edited: $args"',
        '    if [ -n "${STUB_EDIT_EXIT:-}" ]; then',
        '      if [ -n "${STUB_EDIT_FIXTURE:-}" ]; then',
        '        n=$(echo "$args" | grep -oE "edit [0-9]+" | head -1 | grep -oE "[0-9]+")',
        '        cat "$STUB_EDIT_FIXTURE" > "$STUB_ISSUES_DIR/issue-$n.json"',
        '      fi',
        '      exit "$STUB_EDIT_EXIT"',
        '    fi ;;',
        '  *"issue comment"*)',
        "    cat > /dev/null",
        '    echo "comment $* GH_TOKEN=$GH_TOKEN" >> "$STUB_LOG"',
        '    echo "commented: $args" ;;',
        "  *)",
        '    echo "unexpected gh call: $args" >&2',
        "    exit 1 ;;",
        "esac",
      ].join("\n"),
      { mode: 0o755 },
    );
    const issuesDir = join(dir, "issues");
    for (const [number, fixture] of Object.entries(options.issues ?? {})) {
      mkdirSync(issuesDir, { recursive: true });
      writeFileSync(
        join(issuesDir, `issue-${number}.json`),
        JSON.stringify({
          number: Number(number),
          state: fixture.state,
          labels: (fixture.labels ?? []).map((name) => ({ name })),
          body: fixture.body ?? "",
        }),
      );
    }
    const readyTickets = join(dir, "ready.json");
    writeFileSync(readyTickets, options.readyTickets ?? "[]");
    const editFixture = join(dir, "edited.json");
    if (options.editFixture) {
      writeFileSync(
        editFixture,
        JSON.stringify({
          number: 0,
          state: options.editFixture.state,
          labels: (options.editFixture.labels ?? []).map((name) => ({ name })),
          body: options.editFixture.body ?? "",
        }),
      );
    }
    const log = join(dir, "log");
    const { status, stdout } = spawnSync(
      TSX,
      [CONVERT_TS],
      {
        cwd: dir,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${stubBin}:${process.env.PATH}`,
          GH_TOKEN: "github-token",
          GH_REPO: "octo/repo",
          OUTPUT_DIR: dir,
          ...options.env,
          STUB_ISSUES_DIR: issuesDir,
          STUB_READY_TICKETS: readyTickets,
          STUB_LOG: log,
          STUB_EDIT_FIXTURE: options.editFixture ? editFixture : "",
        },
      },
    );
    let logged = "";
    try {
      logged = readFileSync(log, "utf8");
    } catch {
      logged = "";
    }
    return { status: status ?? -1, stdout, log: logged };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

describe("convert.ts", () => {
  it("converts an unblocked ready-for-agent ticket by adding agent:implement with AGENT_PAT", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain(
      "edit 12 --remove-label ready-for-agent --add-label agent:implement GH_TOKEN=pat-token",
    );
    expect(result.log).not.toContain("comment 12");
  });

  it("comments naming an open body-parsed blocker and keeps the label", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
      },
      issues: {
        12: {
          state: "open",
          labels: ["ready-for-agent"],
          body: "## Blocked by\n\n- #8",
        },
        8: { state: "open" },
      },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain("comment 12 --body-file -");
    expect(result.log).not.toContain("edit 12");
    expect(result.stdout).toContain("#8");
  });

  it("degrades with the explore warning when AGENT_PAT is unset but still labels", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        STUB_BLOCKED_BY: "[]",
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("::warning::AGENT_PAT is not set");
    expect(result.log).toContain(
      "edit 12 --remove-label ready-for-agent --add-label agent:implement GH_TOKEN=github-token",
    );
  });

  it("falls back to the body section when the native dependencies call fails", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY_EXIT: "1",
      },
      issues: {
        12: {
          state: "open",
          labels: ["ready-for-agent"],
          body: "## Blocked by\n\n- #8",
        },
        8: { state: "closed" },
      },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain(
      "edit 12 --remove-label ready-for-agent --add-label agent:implement",
    );
  });

  it("re-scans dependents when a blocker closes and converts them", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "closed",
        ISSUE_NUMBER: "8",
        AGENT_PAT: "pat-token",
        STUB_BLOCKING: '[{"number":12}]',
        STUB_BLOCKED_BY: '[{"number":8,"state":"closed"}]',
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain(
      "edit 12 --remove-label ready-for-agent --add-label agent:implement GH_TOKEN=pat-token",
    );
  });

  it("reads native edges with --paginate so lists beyond one page are not truncated", () => {
    const labeled = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(labeled.log).toContain(
      "api --paginate repos/octo/repo/issues/12/dependencies/blocked_by",
    );
    const closed = runConvert({
      env: {
        EVENT_ACTION: "closed",
        ISSUE_NUMBER: "8",
        AGENT_PAT: "pat-token",
        STUB_BLOCKING: "[]",
        STUB_BLOCKED_BY: "[]",
      },
      readyTickets: "[]",
    });
    expect(closed.status).toBe(0);
    expect(closed.log).toContain(
      "api --paginate repos/octo/repo/issues/8/dependencies/blocking",
    );
  });

  it("tolerates the double-conversion race when a concurrent run already swapped the labels", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
        STUB_EDIT_EXIT: "1",
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
      editFixture: { state: "open", labels: ["agent:implement"] },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain("edit 12");
    expect(result.stdout).toContain("already converted by a concurrent run");
  });

  it("fails the run when the label swap fails and no conversion actually happened", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
        STUB_EDIT_EXIT: "1",
      },
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(result.status).not.toBe(0);
    expect(result.log).not.toContain("comment 12");
  });

  it("fails a labeled run whose ticket cannot be read instead of silently skipping it", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
      },
      issues: {},
    });
    expect(result.status).not.toBe(0);
    expect(result.log).not.toContain("edit 12");
    expect(result.log).not.toContain("comment 12");
  });

  it("finds body-scanned dependents of a closed blocker without native edges", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "closed",
        ISSUE_NUMBER: "8",
        AGENT_PAT: "pat-token",
        STUB_BLOCKING: "[]",
        STUB_BLOCKED_BY: '[{"number":8,"state":"closed"}]',
      },
      readyTickets: '[{"number":12,"body":"## Blocked by\\n\\n- #8"}]',
      issues: { 12: { state: "open", labels: ["ready-for-agent"] } },
    });
    expect(result.status).toBe(0);
    expect(result.log).toContain(
      "edit 12 --remove-label ready-for-agent --add-label agent:implement",
    );
  });

  it("skips candidates that no longer carry ready-for-agent", () => {
    const result = runConvert({
      env: {
        EVENT_ACTION: "labeled",
        ISSUE_NUMBER: "12",
        AGENT_PAT: "pat-token",
        STUB_BLOCKED_BY: "[]",
      },
      issues: { 12: { state: "open", labels: ["agent:implement"] } },
    });
    expect(result.status).toBe(0);
    expect(result.log).not.toContain("edit 12");
    expect(result.stdout).toContain("no longer carries");
  });
});
