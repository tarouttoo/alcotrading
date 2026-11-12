import { readFileSync, existsSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string): string =>
  readFileSync(new URL(path, import.meta.url), "utf8");

const labels = read("../../../docs/labels.md");
const triage = read("../../../docs/triage.md");
const agentsTriage = read("../../../docs/agents/triage.md");
const readme = read("../../../README.md");

describe("ready-for-agent converter docs", () => {
  it("creates the label in the setup loop", () => {
    expect(labels).toContain('"ready-for-agent"');
  });

  it("documents the converter workflow and its semantics", () => {
    expect(labels).toContain("agent-convert.yml");
    expect(labels).toContain("`agent:implement`");
    expect(labels).toContain("AGENT_PAT");
    expect(labels).toContain("## Blocked by");
  });

  it("keeps the waiting-ticket promise: convert when the last blocker closes", () => {
    expect(labels).toMatch(/last blocker clos/i);
  });

  it("is reachable from the human triage guide", () => {
    expect(triage).toContain("ready-for-agent");
    expect(triage).toMatch(/agent-convert\.yml|converter/i);
  });

  it("maps the canonical role to the label in docs/agents/triage.md", () => {
    expect(agentsTriage).toMatch(
      /\|\s*`ready-for-agent`\s*\|\s*`ready-for-agent`/,
    );
  });

  it("is listed in the README workflows table", () => {
    expect(readme).toContain("agent-convert.yml");
    expect(readme).toMatch(/ready-for-agent.*agent-convert\.yml/s);
  });

  it("documents the converter's failure and retry story in the README", () => {
    expect(readme).toMatch(/Every agent run posts a failure comment/);
    expect(readme).toMatch(
      /retry it by removing and re-adding\s+`ready-for-agent`/s,
    );
  });

  it("is recorded as a documented deviation with an ADR", () => {
    expect(readme).toMatch(/Deviations[\s\S]*ready-for-agent`? converter/i);
    expect(
      existsSync(
        new URL(
          "../../../docs/adr/0003-ready-for-agent-converter.md",
          import.meta.url,
        ),
      ),
    ).toBe(true);
  });
});
