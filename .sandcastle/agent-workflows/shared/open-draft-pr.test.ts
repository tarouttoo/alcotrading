import { spawnSync } from "node:child_process";
import { chmodSync, readFileSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parse } from "yaml";
import { describe, expect, it } from "vitest";

const WORKFLOW_PATH = new URL(
  "../../../.github/workflows/agent-implement.yml",
  import.meta.url,
);

const OPEN_DRAFT_PR_STEP = "Open draft PR";

/** The run script of a named step, exactly as GitHub Actions would execute it. */
const runScriptOf = (stepName: string): string => {
  const workflow = parse(readFileSync(WORKFLOW_PATH, "utf8")) as {
    jobs: Record<string, { steps: { name?: string; run?: string }[] }>;
  };
  const step = Object.values(workflow.jobs)
    .flatMap((job) => job.steps)
    .find((step) => step.name === stepName);
  if (!step?.run) throw new Error(`No run script found for step: ${stepName}`);
  return step.run;
};

interface StepResult {
  status: number;
  /** Contents of the step-outputs file (GitHub's GITHUB_OUTPUT). */
  outputs: string;
}

/**
 * Runs a step's run script the way GitHub Actions does (bash with -e, no
 * pipefail by default), with a stub `gh` executable first on PATH.
 */
const runStep = async (
  script: string,
  ghStub: string,
): Promise<StepResult> => {
  const dir = await mkdtemp(join(tmpdir(), "open-draft-pr-"));
  try {
    const stubBin = join(dir, "bin");
    const stub = join(stubBin, "gh");
    await mkdir(stubBin, { recursive: true });
    await writeFile(stub, `#!/usr/bin/env bash\n${ghStub}\n`, { mode: 0o755 });
    chmodSync(stub, 0o755);
    const scriptPath = join(dir, "step.sh");
    await writeFile(scriptPath, script);
    const githubOutput = join(dir, "github-output.txt");
    const prBodyFile = join(dir, "pr-body.md");
    await writeFile(prBodyFile, "Closes #11\n");
    const { status } = spawnSync(
      "bash",
      ["--noprofile", "--norc", "-e", scriptPath],
      {
        cwd: dir,
        env: {
          ...process.env,
          PATH: `${stubBin}:${process.env.PATH}`,
          ISSUE_NUMBER: "11",
          ISSUE_TITLE: "Some issue title",
          BRANCH: "agent/issue-11-some-issue-title",
          PR_BODY_FILE: prBodyFile,
          GITHUB_OUTPUT: githubOutput,
        },
      },
    );
    const outputs = await readFile(githubOutput, "utf8").catch(() => "");
    return { status: status ?? -1, outputs };
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
};

describe("Open draft PR step", () => {
  it("fails when gh pr create fails", async () => {
    const result = await runStep(runScriptOf(OPEN_DRAFT_PR_STEP), `
      echo "could not create PR: branch is protected" >&2
      exit 1
    `);
    expect(result.status).not.toBe(0);
    expect(result.outputs).not.toContain("pr_number=");
  });

  it("does not publish a pr_number when gh fails after printing stdout", async () => {
    const result = await runStep(runScriptOf(OPEN_DRAFT_PR_STEP), `
      echo "Creating draft pull request for agent/issue-11-some-issue-title..."
      echo "GraphQL: Could not resolve to a PullRequest" >&2
      exit 1
    `);
    expect(result.status).not.toBe(0);
    expect(result.outputs).not.toContain("pr_number=");
  });

  it("extracts pr_number from the URL gh prints on success", async () => {
    const result = await runStep(runScriptOf(OPEN_DRAFT_PR_STEP), `
      echo "Creating draft pull request for agent/issue-11-some-issue-title..."
      echo "https://github.com/octo/repo/pull/42"
      exit 0
    `);
    expect(result.status).toBe(0);
    expect(result.outputs).toContain("pr_number=42");
  });
});
