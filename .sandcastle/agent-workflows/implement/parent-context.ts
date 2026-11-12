/**
 * Pure model of a child-of-spec Ticket's parent context: validating the
 * parent pointer the workflow's shape step exports, and rendering the
 * `PARENT_CONTEXT` prompt arg the implement agent receives. No I/O —
 * `implement.ts` owns the `gh` calls.
 */

/** The prompt arg handed to the agent when the Ticket has no parent spec. */
export const NO_PARENT_CONTEXT = "(none — this Ticket has no parent spec)";

/**
 * Validate the parent-issue pointer exported by the workflow's shape step.
 * Only a plain positive integer counts; anything else (missing, empty, or
 * carrying shell syntax) means "no parent". The pointer is interpolated into
 * a shell command by the caller, so it is treated as untrusted even though
 * GitHub's API produced it.
 */
export const parseParentNumber = (raw: string | undefined): string | null =>
  raw !== undefined && /^[1-9]\d*$/.test(raw) ? raw : null;

/**
 * Render the `PARENT_CONTEXT` prompt arg: the parent spec's `gh issue view`
 * output, framed as background for the Ticket. `view` is the I/O boundary
 * (the real caller passes a `safeSh` fetch); an empty result means the
 * parent could not be loaded, which degrades to a named pointer instead of
 * failing the run.
 */
export const renderParentContext = (
  parentNumber: string | null,
  view: (number: string) => string,
): string => {
  if (parentNumber === null) {
    return NO_PARENT_CONTEXT;
  }
  const spec = view(parentNumber).trim();
  if (!spec) {
    return `The parent spec issue #${parentNumber} could not be loaded.`;
  }
  return [`<parent-spec issue="${parentNumber}">`, spec, "</parent-spec>"].join(
    "\n",
  );
};
