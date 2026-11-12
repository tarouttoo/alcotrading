# Child-of-spec Tickets are implementable

Upstream's implement preflight refuses any issue that has a parent, so a
Spec broken into sub-issue Tickets can never run there: every Ticket is a
sub-issue of something. We adopt the tracker flow the skills' vocabulary
describes — a published Spec with Tickets under it — so the preflight now
refuses only PRD-shaped issues (a Ticket must have no sub-issues of its
own, keeping one change per issue); a Ticket that is a child of another
issue passes, and the implement pass pulls the parent Spec in as the
`PARENT_CONTEXT` prompt arg via `gh issue view --comments`, mirroring
upstream's local parallel-planner prompt ("If it has a parent PRD, pull
that in too", pinned `a53e227`) — behavior upstream's CI workflow never
reached because of its own refusal.

## Considered options

- **Keep refusing all sub-issues** — rejected: makes Spec → Ticket triage
  impossible in the pipeline; every child Ticket would land `agent:blocked`
  on its first `agent:implement` run and need hand-relabelling.
- **Instruct the agent to pull the parent itself** (prompt text only, as
  upstream's local prompt does) — rejected: not demonstrable. The run must
  receive the parent context deterministically; downstream work (a
  ready-for-agent converter that labels child Tickets) is built on that
  guarantee.

## Consequences

The parent Spec is untrusted input like any issue body: it enters only
through the prompt-file channel, and the parent pointer is validated as a
plain positive integer before it may reach a shell. Implement runs can now
be influenced by a parent's content, but the blast radius is unchanged —
the Ticket, not the Spec, is what the PR closes, and the review pass and
human still audit the diff. The parent remains refused when labelled
directly, so a Spec cannot silently become a mega-PR.
