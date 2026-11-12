import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const readme = readFileSync(
  new URL("../README.md", import.meta.url),
  "utf8",
);

const adopting = readme
  .split(/^## /m)
  .find((section) => section.startsWith("Adopting this template"));

if (!adopting) {
  throw new Error("README.md has no '## Adopting this template' section");
}

describe("adopter setup docs", () => {
  it("requires 'Allow GitHub Actions to create and approve pull requests'", () => {
    expect(adopting).toContain(
      "Allow GitHub Actions to create and approve pull requests",
    );
  });

  it("says why: the implement workflow opens its draft PR with GITHUB_TOKEN", () => {
    expect(adopting).toMatch(/agent-implement\.yml/);
    expect(adopting).toMatch(/GITHUB_TOKEN/);
    expect(adopting).toMatch(/draft PR/);
  });

  it("says AGENT_PAT does not bypass the requirement", () => {
    expect(adopting).toMatch(/AGENT_PAT/);
  });

  it("tells the adopter where the setting lives", () => {
    expect(adopting).toMatch(/Settings.*Actions.*General/s);
  });

  it("gives a CLI command to enable the setting", () => {
    expect(adopting).toMatch(/can_approve_pull_request_reviews=true/);
  });

  it("documents the failure signature when the setting is missing", () => {
    // "Open draft PR" pipes `gh pr create` through `tail`, so that step
    // stays green and the run fails at "Request automated review" instead.
    expect(adopting).toMatch(/Open draft PR/);
    expect(adopting).toMatch(/Request automated review/);
    expect(adopting).toContain("no pull requests found for branch");
  });
});
