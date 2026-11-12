import { readFileSync } from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Contract tests over the implement workflow's production artifacts. The
 * preflight lives in `agent-implement.yml` (bash) and reaches the agent
 * through `implement.ts` + `prompt.md`; these tests pin the wiring the
 * child-of-spec behavior depends on.
 */

const workflow = readFileSync(
  path.join(import.meta.dirname, "../../../.github/workflows/agent-implement.yml"),
  "utf8",
);
const script = readFileSync(path.join(import.meta.dirname, "implement.ts"), "utf8");
const prompt = readFileSync(path.join(import.meta.dirname, "prompt.md"), "utf8");

describe("implement preflight (Detect issue shape)", () => {
  it("does not refuse an issue for having a parent", () => {
    expect(workflow).not.toContain("Refuse sub-issue");
    expect(workflow).not.toContain('elif [ -n "$parent_number" ]');
  });

  it("still refuses a PRD-shaped issue, with the same blocked treatment", () => {
    expect(workflow).toContain("Refuse PRD-shaped issue");
    expect(workflow).toContain(
      "if: steps.shape.outputs.sub_count != '0'",
    );
    expect(workflow).toContain(
      '--body "Refused to run: issue #${ISSUE_NUMBER} has sub-issues',
    );
  });

  it("emits the parent_number output the implement step consumes", () => {
    // Without this line PARENT_NUMBER is always empty and every
    // child-of-spec Ticket silently runs without its parent spec.
    const shapeStep = workflow.slice(
      workflow.indexOf("Detect issue shape"),
      workflow.indexOf("- name: Refuse PRD-shaped issue"),
    );
    expect(shapeStep).toContain(
      'echo "parent_number=$parent_number" >> "$GITHUB_OUTPUT"',
    );
  });

  it("exports the parent pointer to the implement script", () => {
    const runStep = workflow.slice(workflow.indexOf("Run implementation agent"));
    const stepBody = runStep.slice(0, runStep.indexOf("- name:", 1));
    expect(stepBody).toContain(
      "PARENT_NUMBER: ${{ steps.shape.outputs.parent_number }}",
    );
  });
});

describe("implement pass parent context", () => {
  it("renders the parent context in the script and hands it to the prompt", () => {
    expect(script).toContain("renderParentContext");
    expect(script).toContain("PARENT_CONTEXT:");
  });

  it("renders {{PARENT_CONTEXT}} into the prompt", () => {
    expect(prompt).toContain("{{PARENT_CONTEXT}}");
  });
});
