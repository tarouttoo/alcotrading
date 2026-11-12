import { describe, expect, it } from "vitest";
import {
  NO_PARENT_CONTEXT,
  parseParentNumber,
  renderParentContext,
} from "./parent-context";

describe("parseParentNumber", () => {
  it("accepts the plain positive integer the workflow's shape step emits", () => {
    expect(parseParentNumber("12")).toBe("12");
  });

  it("treats a missing pointer as no parent", () => {
    expect(parseParentNumber(undefined)).toBe(null);
  });

  it("treats an empty pointer as no parent", () => {
    expect(parseParentNumber("")).toBe(null);
  });

  it("refuses anything that is not a plain integer before it reaches a shell", () => {
    expect(parseParentNumber("12; rm -rf /")).toBe(null);
    expect(parseParentNumber("0")).toBe(null);
    expect(parseParentNumber("12\n12")).toBe(null);
  });
});

describe("renderParentContext", () => {
  it("returns the placeholder for a standalone Ticket without fetching anything", () => {
    expect(
      renderParentContext(null, () => {
        throw new Error("must not fetch for a standalone Ticket");
      }),
    ).toBe(NO_PARENT_CONTEXT);
  });

  it("hands the parent spec body to the agent, framed as context", () => {
    const context = renderParentContext("5", () => "Spec: the parser milestone.");
    expect(context).toContain('<parent-spec issue="5">');
    expect(context).toContain("Spec: the parser milestone.");
  });

  it("degrades to a named pointer when the parent spec cannot be loaded", () => {
    const context = renderParentContext("5", () => "");
    expect(context).toContain("#5");
    expect(context).toContain("could not be loaded");
  });
});
