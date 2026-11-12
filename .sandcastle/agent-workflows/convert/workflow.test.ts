import { readFileSync } from "node:fs";
import { parse } from "yaml";
import { describe, expect, it } from "vitest";

const WORKFLOW_PATH = new URL(
  "../../../.github/workflows/agent-convert.yml",
  import.meta.url,
);

const workflow = parse(readFileSync(WORKFLOW_PATH, "utf8")) as {
  name: string;
  on: { issues?: { types?: string[] }; schedule?: unknown };
  concurrency: { group: string; "cancel-in-progress": boolean };
  jobs: Record<
    string,
    {
      if?: string;
      permissions: { issues: string; contents: string };
      steps: { name?: string; run?: string; env?: Record<string, string> }[];
    }
  >;
};

const job = workflow.jobs.convert;
const triggers = workflow.on;

const convertStep = Object.values(job?.steps ?? []).find((step) =>
  step.run?.includes("convert/convert.ts"),
);

describe("agent-convert.yml", () => {
  it("fires on issues labeled and closed — and nothing else (no polling)", () => {
    expect(triggers?.issues?.types).toEqual(["labeled", "closed"]);
    expect(triggers?.schedule).toBeUndefined();
  });

  it("gates the job on the ready-for-agent label or an issue closing", () => {
    expect(job?.if).toContain("ready-for-agent");
    expect(job?.if).toContain("closed");
  });

  it("groups runs per triggering issue so parallel label events queue", () => {
    expect(workflow.concurrency.group).toContain("github.event.issue.number");
    expect(workflow.concurrency["cancel-in-progress"]).toBe(false);
  });

  it("writes only to issues", () => {
    expect(job?.permissions).toMatchObject({ issues: "write", contents: "read" });
  });

  it("passes AGENT_PAT to the convert step so the hand-off label can cascade", () => {
    expect(convertStep?.env?.AGENT_PAT).toBe("${{ secrets.AGENT_PAT }}");
  });
});
