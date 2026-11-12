# Labels

The pipeline is driven entirely by labels. There are two kinds, matching the
glossary in [CONTEXT.md](../CONTEXT.md):

- **Trigger labels** — you add these to drive the pipeline. Adding one starts
  the matching workflow; the workflow removes the label when the run begins.
- **Lifecycle labels** — the workflows add and remove these themselves.
- **Triage label** — `ready-for-agent`: the approval the converter consumes
  (see below).

What each triggered workflow does in detail: the
[Workflows table in the README](../README.md#workflows). How a human drives
the loop day to day — writing triageable issues, reviewing draft PRs,
acting on verdicts: [docs/triage.md](triage.md). The canonical triage roles
mattpocock/skills speaks in (`needs-triage`, `needs-info`, `ready-for-agent`,
`ready-for-human`, `wontfix`) map onto the labels below in
[docs/agents/triage.md](agents/triage.md).

## Trigger labels

| Label | Add it to | Starts | Notes |
|---|---|---|---|
| `agent:implement` | issues | `agent-implement.yml` | Refuses PRD-shaped issues (sub-issues of their own) / issues that already have an open collaborator PR; a child-of-spec Ticket passes and its parent Spec is pulled in as context. On success labels the new draft PR `agent:review`. |
| `agent:implement` | PRs | `agent-implement-pr.yml` | Processes review feedback: replies to threads, addresses inline comments, pushes with a lease. |
| `agent:review` | PRs | `agent-review.yml` | On success marks the PR ready. |
| `agent:update-branch` | PRs | `agent-update-branch.yml` | Always resolves merge conflicts with `main`, never aborts. |
| `agent:explore` | issues | `agent-explore.yml` | Batch triage. See the exception below. |

A failed run labels the subject `agent:blocked` and posts a failure comment
naming the trigger label to re-add — **retrying means re-adding that label**.

The exception is `agent:explore`: the label stays on the issue across runs
(the apply job swaps it for a verdict label or removes it when closing a
duplicate), and each run ignores the triggering event and explores every open
`agent:explore` issue together. It can also be run via `workflow_dispatch`,
which is a dry-run by default — set the `dry_run` input to `false` to apply
the plan for real.

## Lifecycle labels

| Label | Meaning |
|---|---|
| `agent:in-progress` | Added when a run begins (alongside removing the trigger label and any `agent:blocked`); removed when the run ends, success or failure. |
| `agent:blocked` | Added when a run fails, together with the failure comment naming the trigger label to re-add; removed by the next run's start. The explore batch does not use it — a failed batch leaves every `agent:explore` label in place for the next run. |
| `ready-for-human` | The explore `needs-a-human` verdict, applied to a surviving issue that needs a human decision. Must exist or the apply job's label swap fails. |

The other two explore verdicts reuse labels above: an `easy-call` survivor is
handed `agent:implement` (added with `AGENT_PAT` so the label event actually
starts the implement workflow), and a `blocked` survivor gets
`agent:blocked`.

## Triage label

| Label | Put it on | What happens |
|---|---|---|
| `ready-for-agent` | Ticket issues | The converter's approval (`agent-convert.yml`, no upstream counterpart). The Ticket is evaluated when it is labelled and re-scanned whenever another issue closes. All blockers closed → the label is swapped for `agent:implement` (added with `AGENT_PAT` so the label event actually starts the implement workflow). Open blockers → a comment names them; the label is kept and the swap fires when the last blocker closes. |

Blocking edges are read from the tracker's native blocked-by relationships
where available, falling back to the issue body's `## Blocked by` section —
parsed in TypeScript, never the shell. This is the label the triage skills
speak in: `docs/agents/triage.md` maps the canonical `ready-for-agent` role
onto it, and the converter is what makes the pipeline honor that vocabulary
(`docs/adr/0003-ready-for-agent-converter.md`).

## Setup

Create the labels the workflows expect:

```sh
for label in "agent:implement" "agent:review" "agent:update-branch" "agent:explore" "agent:in-progress" "agent:blocked" "ready-for-human" "ready-for-agent"; do
  gh label create "$label" --color d4c5f9 || true
done
```
