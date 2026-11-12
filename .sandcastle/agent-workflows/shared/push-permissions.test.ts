import { readdirSync, readFileSync } from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import { parse } from "yaml";

/**
 * Contract tests over every workflow that pushes a branch: pushing commits
 * that create or update files under `.github/workflows/` is refused with a
 * plain GITHUB_TOKEN ("refusing to allow a GitHub App to create or update
 * workflow … without `workflows` permission"), and `workflows` is not a
 * grantable scope in the Actions `permissions` block — the checkout token
 * is the only lever. Upstream never pushes workflow-file changes; this
 * repo's agents build the pipeline itself, so every branch-pushing job
 * must check out with `AGENT_PAT || GITHUB_TOKEN` (README, "Deviations
 * from upstream" 8). Without it the run fails at its final "Push branch"
 * step after the agent did all the work, and the PR lands `agent:blocked`.
 */

type Step = {
  uses?: string;
  with?: { token?: string };
  run?: string;
};

type Job = { steps?: Step[] };

const workflowsDir = path.join(import.meta.dirname, "../../../.github/workflows");

function branchPushingJobs(): Map<string, Job> {
  const found = new Map<string, Job>();
  for (const file of readdirSync(workflowsDir).filter((f) => f.endsWith(".yml"))) {
    const doc = parse(
      readFileSync(path.join(workflowsDir, file), "utf8"),
    ) as { jobs?: Record<string, Job> };
    for (const [name, job] of Object.entries(doc.jobs ?? {})) {
      if ((job.steps ?? []).some((step) => step.run?.includes("git push"))) {
        found.set(`${file} > ${name}`, job);
      }
    }
  }
  return found;
}

describe("branch-pushing jobs can push workflow files", () => {
  const jobs = branchPushingJobs();

  it("covers exactly the workflows that push a branch", () => {
    // Fails when a workflow gains or loses a push step, forcing a look at
    // whether the new pusher can still push workflow-file changes.
    expect([...jobs.keys()].sort()).toEqual([
      "agent-implement-pr.yml > implement-pr",
      "agent-implement.yml > implement",
      "agent-review.yml > review",
      "agent-update-branch.yml > update-branch",
    ]);
  });

  it.each([...jobs.entries()])(
    "%s checks out with AGENT_PAT || GITHUB_TOKEN",
    (_, job) => {
      const checkouts = (job.steps ?? []).filter((step) =>
        step.uses?.startsWith("actions/checkout"),
      );
      expect(checkouts.length).toBeGreaterThan(0);
      for (const checkout of checkouts) {
        expect(checkout.with?.token).toBe(
          "${{ secrets.AGENT_PAT || secrets.GITHUB_TOKEN }}",
        );
      }
    },
  );
});
