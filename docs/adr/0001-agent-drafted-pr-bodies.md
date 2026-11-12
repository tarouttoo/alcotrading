# Agent-drafted PR bodies

The pipeline's PR bodies are drafted by the implement agent per the shipped
`.agents/skills/pr/` skill (Summary / Evidence / Merge Danger, from
mattpocock/skills), not the upstream canned stub; the stub remains only as a
fallback when the agent writes no body. We first rejected this — agent prose
risks hallucinated claims, and the stub is honest — then adopted it once the
merge decision made the body load-bearing: the human merges off the body plus
the review comment, so the body must state what changed, how it was verified,
and how dangerous it is to land.

The design keeps author and auditor separate, unlike mattpocock's interactive
chain (implement → code-review → pr) where one agent writes the change,
reviews it, and grades its own door call. Here the implement agent claims,
the review agent audits the claims against the diff in its own comment, and a
discrepancy between the two is itself signal. The reviewer never edits the
body — like a real team, the reviewer works in review comments — and the
implement-pr feedback pass (the author's side) refreshes a stale body after
fixing review feedback.

## Considered options

- **Keep the stub forever** — rejected: gives the human no merge context.
- **Review-side drafting** (body written at mark-ready, mattpocock's
  ordering) — rejected: collapses claim and audit into one agent, which is
  exactly the "be most suspicious when it says two-way" problem his own docs
  warn about.

## Consequences

Bodies can be wrong; that is accepted. The controls are the review-pass audit
line and the human comparison of body vs review. The disclosure footer keeps
the stub's honesty about machine authorship.
