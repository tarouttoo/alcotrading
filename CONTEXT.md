# CONTEXT

The agent's map of this repository: what it is, the vocabulary used in
prompts and code, and where deeper material lives. Read the relevant section
before resolving anything substantive; add terms here when you introduce a
new concept.

## What this repo is

A template repository: a label-driven agent pipeline that runs coding agents
(pi CLI) on GitHub Actions. Issues and PRs are worked on by adding trigger
labels (`agent:implement`, `agent:review`, `agent:update-branch`,
`agent:explore`); workflow code lives under `.sandcastle/agent-workflows/`.

## Glossary

- **Trigger label** — a label (`agent:*`) whose addition starts a workflow.
  Removed by the workflow itself when the run begins.
- **Lifecycle label** — a label the pipeline manages on its own:
  `agent:in-progress` while a run is active, `agent:blocked` when a run
  fails (with a failure comment naming the trigger label to re-add).
- **Subject** — the issue or PR a run is about. Concurrency groups are
  keyed per subject so parallel label events queue instead of racing.
- **Draft PR** — the pull request the implement workflow opens against
  `main`, body starting with `Closes #N` for the triggering issue, drafted
  by the implement agent per the `pr` skill and audited (never edited) by
  the review pass.
- **Spec** — a parent issue whose sub-issues are the Tickets that implement
  it (upstream calls this shape a PRD). The implement preflight refuses a
  Spec labelled directly; when one of its Tickets runs, the Spec is pulled
  in as the `PARENT_CONTEXT` prompt arg.
- **Ticket** — an issue the implement workflow can act on: no sub-issues of
  its own (one change per issue). May be standalone or a child of a Spec; a
  child-of-spec Ticket's run receives its parent Spec as context.
- **Batch triage** — the explore workflow: one run collects every open
  `agent:explore` issue, the agent plans per-issue effects, and a separate
  apply job validates and applies the plan (issue bodies are untrusted
  input and never enter the shell).
- **Converter** — the ready-for-agent converter (`agent-convert.yml`, no
  upstream counterpart, ADR 0003): a Ticket labelled `ready-for-agent` (the
  skills' "AFK-ready" role) is swapped to `agent:implement` once every
  blocking edge is closed; edges come from native blocked-by relationships
  with the body's `## Blocked by` section as fallback, and a blocker closing
  re-scans its dependents. The label is the approval; the decision is a
  deterministic script, not an agent.
- **Canonical triage roles** — the five roles the skills speak in
  (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`,
  `wontfix`), mapped to this repo's label strings in
  `docs/agents/triage.md`.
- **Extraction** — the second, structured-output pass a run may take
  (`extraction.md`) to turn free-form agent output into typed JSON.
- **PAT cascade** — label events fired by `GITHUB_TOKEN` never start other
  workflows; `AGENT_PAT` is used where a workflow must trigger another one
  (e.g. implement requesting review).

## Where things live

- `docs/labels.md` — the reference for every label the pipeline uses
  (trigger and lifecycle) and the adopter setup command.
- `docs/triage.md` — how a human feeds work into the pipeline: triageable
  issues, the draft-PR review loop, and acting on explore verdicts.
- `docs/pr-body.md` — how to read an agent PR when deciding to merge: the
  agent-drafted body, the review-pass audit, and the merge checklist.
- `docs/adr/` — architecture decision records. Add one when making a
  non-obvious, hard-to-reverse choice; read the relevant ones before
  changing pipeline behaviour.
- `docs/agents/` — agent-facing conventions restored from upstream: the
  canonical triage roles mapped to this repo's labels (`agents/triage.md`),
  the GitHub issue-tracker command conventions (`agents/issue-tracker.md`),
  how skills consume domain docs (`agents/domain.md`), and upstream
  sandcastle-package contributor guides (`agents/adding-an-agent-provider.md`,
  `agents/adding-an-issue-tracker.md`).
- `.agents/skills/triage-repro/` — skill for interactively triaging an
  issue number and attempting a reproduction; complements the label-driven
  batch triage, which it does not replace.
- `.sandcastle/CODING_STANDARDS.md` — coding standards the workflow agents
  apply to the host repo; they describe how code in an adopting repo
  should look.
- `README.md` — adopter setup (repo settings, secrets, labels) and
  deviations from the upstream pipeline this was ported from.
