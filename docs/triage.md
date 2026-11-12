# Triage: feeding work into the pipeline

This is the human's side of the loop: how to write issues the pipeline can
act on, how to hand work to the agents, and what is still yours to do.
Label semantics live in [docs/labels.md](labels.md); the canonical triage
roles the skills speak in (`needs-triage`, `needs-info`, `ready-for-agent`,
`ready-for-human`, `wontfix`) map onto those labels in
[docs/agents/triage.md](agents/triage.md); per-workflow detail in the
[Workflows table](../README.md#workflows).

## The core loop

```
write issue ──▶ label agent:implement ──▶ agent opens draft PR
                                              │
              ┌───────────────────────────────┘
              ▼
        you review the PR ──▶ feedback? ──▶ label the PR agent:implement
              │                                    │
              │                          agent addresses comments
              │                                    │
              └──▶ happy? ──▶ YOU merge ◀──────────┘
```

The pipeline implements, reviews, and fixes. **It never merges** — merging
is always the human's call.

## 1. Write issues the pipeline can act on

The implement workflow is gated by a preflight that refuses issues it can't
handle. Before labelling, check the issue is:

- **A Ticket, not a Spec** — it has no sub-issues of its own. A PRD-shaped
  issue (one with sub-issues) is refused outright with `agent:blocked`;
  label its sub-issue Tickets instead. A Ticket may sit under a parent
  Spec: the implement pass pulls the parent Spec in as context, so keep
  the big picture on the parent and the one change on the Ticket.
- **One change per issue** — the agent opens one draft PR per issue and the
  PR body starts with `Closes #N`, so splitting unrelated asks into
  separate issues keeps the loop clean.
- **Self-contained** — the issue body is the agent's spec: repro steps for
  bugs, expected behaviour, constraints. Anything an implementer would need
  to ask you belongs in the issue before you label it.
- **Not already covered** — an issue with an open collaborator PR is
  refused; close the existing PR first if you want a fresh run.

A refusal comments on the issue explaining why and removes the trigger
label; the PRD-shaped refusal also adds `agent:blocked`. Fix
what the comment names and re-add `agent:implement`.

## 2. Hand an issue to an agent

Add `agent:implement` to the issue. The workflow picks it up, removes the
trigger label, and labels the issue `agent:in-progress` while it works.
When it finishes you have a **draft PR** that closes the issue on merge,
already labelled `agent:review`.

If the run fails, the issue gets `agent:blocked` and a failure comment
naming the trigger label to re-add — **retrying means re-adding that
label**.

### The automated alternative: `ready-for-agent`

If the Ticket is fully specified but waiting on other issues — or it was
published by the triage skills' `to-spec`/`to-tickets` flow, which label
child Tickets `ready-for-agent` by construction — you can label it
`ready-for-agent` instead of `agent:implement`. The converter
(`agent-convert.yml`) swaps it for `agent:implement` automatically once
every blocker is closed; until then it comments naming the open blockers.
Blockers are read from native blocked-by relationships where available,
falling back to the issue body's `## Blocked by` section
([docs/labels.md](labels.md),
[ADR 0003](adr/0003-ready-for-agent-converter.md)).

## 3. Review the draft PR (your main job)

The review loop is where the human judgement lives. How to read the PR
body itself when deciding to merge: [docs/pr-body.md](pr-body.md).

- **Give feedback** by leaving normal PR review comments — top-level and
  inline — then add `agent:implement` **to the PR**. The agent replies to
  threads, addresses actionable comments (or pushes back with a reason),
  and pushes. Agents never resolve review threads or merge; check the
  replies, then resolve the threads yourself.
- **Ask for another review pass** with `agent:review` on the PR. The agent
  reviews the diff, self-fixes what it finds, posts a review, and marks the
  PR ready for review when it passes.
- **Resolve merge conflicts** with `agent:update-branch` on the PR — the
  agent always resolves against `main`, never aborts.
- **Merge** when you're satisfied. That closes the linked issue via the
  `Closes #N` in the PR body.

## 4. Feed the backlog through batch triage

For a pile of unexamined issues, label each one `agent:explore`. The first
label event starts a run that groups, explores, and closes duplicates
across **every** open `agent:explore` issue at once — the label stays on
the issue across runs, so labelling one more issue fires a fresh batch that
re-lists all of them. A batch can also be started via `workflow_dispatch`,
which is a dry-run by default; set `dry_run` to `false` to apply the plan.

Each surviving issue ends up with a verdict you act on:

| Verdict label | Meaning | Your move |
|---|---|---|
| `agent:implement` | `easy-call`: change is clear, no judgement call needed | Nothing — the apply job queued it into the implement workflow automatically |
| `ready-for-human` | `needs-a-human`: a design, product, or priority decision comes first, or the issue's claims didn't hold up | Make the call, then re-scope the issue and label it `agent:implement` (or close it) |
| `agent:blocked` | `blocked`: waiting on something outside the repo or on missing information from the reporter | Get the missing input, then re-label `agent:implement` (or `agent:explore` to re-triage) |

The triage comment on each survivor is skimmable and verified against the
code: blast radius, one-way vs two-way door, relevant files, open
questions. Read the survivor's comment before acting on it. Duplicate
issues are closed with a one-line comment pointing at the survivor.

## Quick checklist

- [ ] Issue is a Ticket (no sub-issues), one change, self-contained → add `agent:implement`
- [ ] Draft PR arrives → review it
- [ ] Feedback → comment + `agent:implement` on the PR
- [ ] Conflicts → `agent:update-branch` on the PR
- [ ] Ready → you merge (the pipeline never does)
- [ ] Backlog → `agent:explore`, act on `ready-for-human` / `agent:blocked` survivors
- [ ] Failure comment → re-add the trigger label it names
