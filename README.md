# agent-pipeline-template

A reusable template repository for a **label-driven agent pipeline**: you
triage issues and PRs by adding `agent:*` labels, and coding agents run on
GitHub Actions — implementing issues into draft PRs, reviewing PRs,
resolving merge conflicts, and batch-triaging backlogs.

The pipeline is a verbatim port of [mattpocock/sandcastle](https://github.com/mattpocock/sandcastle)
(commit `a53e227250fb51df0ad54155b85fecc1c090c937`, 2026-10-19), with one
deliberate adaptation: **the coding agent is the [pi CLI](https://github.com/earendil-works/pi-coding-agent),
not Claude Code**. See [Deviations from upstream](#deviations-from-upstream).

## Workflows

| Label | Workflow | What happens |
|---|---|---|
| `agent:implement` (issue) | `agent-implement.yml` | Preflight gates (refuses PRD-shaped issues / issues with an open collaborator PR; a child-of-spec Ticket passes and its parent Spec is pulled in as context), then an agent implements on an isolated branch and opens a draft PR that closes the issue, then labels the PR `agent:review`. |
| `agent:implement` (PR) | `agent-implement-pr.yml` | Processes review feedback on an existing PR: replies to threads, addresses inline comments, pushes with a lease. |
| `agent:review` | `agent-review.yml` | Reviews the PR, self-fixes findings on the branch, posts a PR review, replies to threads, marks the PR ready. |
| `agent:update-branch` | `agent-update-branch.yml` | Resolves merge conflicts with `main` (always resolve, never abort) and pushes. |
| `agent:explore` | `agent-explore.yml` | Batch triage: one agent run plans effects for every open `agent:explore` issue; a separate apply job validates the plan and applies it. Supports `workflow_dispatch` dry-run. |
| `ready-for-agent` (issue) | `agent-convert.yml` | Converter (no upstream counterpart): re-evaluates the Ticket when labelled and whenever another issue closes. All blockers closed → swaps the label for `agent:implement` (added with `AGENT_PAT` so the implement workflow actually starts); open blockers → comments naming them and keeps waiting. Blocking edges come from the tracker's native blocked-by relationships, falling back to the body's `## Blocked by` section. |

Every agent run posts a failure comment with a reason and the workflow-run
link, and relabels the subject `agent:blocked`. Retrying means re-adding the
trigger label named in that comment. The `ready-for-agent` converter is the
exception: it runs no agent, so a failed converter run is a plain workflow
failure with no failure comment — retry it by removing and re-adding
`ready-for-agent` on the affected Ticket.

## Adopting this template

1. **Create your repo from this template** (or clone it). The pipeline
   files live in `.github/workflows/` and `.sandcastle/agent-workflows/`.
2. **Create the labels** the workflows expect — the full reference
   (trigger, lifecycle, and verdict labels, what each one does, and the
   setup command) lives in [`docs/labels.md`](docs/labels.md).

3. **Allow GitHub Actions to create and approve pull requests** (repo →
   Settings → Actions → General → Workflow permissions). The implement
   workflow (`agent-implement.yml`) opens its draft PR with `GITHUB_TOKEN`,
   and with this setting off — the default for new repositories and
   organizations — GitHub rejects that PR creation. Enable it in the
   settings UI, or from the CLI (the `read` default is safe here: every
   workflow declares its own `permissions` block):

   ```sh
   gh api -X PUT repos/<owner>/<repo>/actions/permissions/workflow \
     -f default_workflow_permissions=read \
     -F can_approve_pull_request_reviews=true
   ```

   With the setting off, the failure is easy to misread: `gh pr create` in
   "Open draft PR" is piped through `tail`, so that step stays green and
   the run instead fails at "Request automated review" with
   `no pull requests found for branch`, leaving the issue `agent:blocked`
   with an opaque reason. Setting `AGENT_PAT` does not bypass this: the
   "Open draft PR" step always uses `GITHUB_TOKEN`.

4. **Set the secrets** (repo → Settings → Secrets and variables → Actions):
   - `ZAI_API_KEY` — API key for the pi provider (required by every agent run).
   - `AGENT_PAT` — a personal access token with `contents`, `issues` and
     `pull-requests` write (classic: `repo` scope; workflow-touching changes
     also need the `workflow` scope). Required in practice: label events
     fired by `GITHUB_TOKEN` never start other workflows, so without
     `AGENT_PAT` the automatic `implement → review` cascade silently degrades
     (the review label is added but never fires). The implement checkout also
     prefers `AGENT_PAT` for pushing.
5. **Tune the provider/model** if needed: each workflow sets
   `AGENT_PROVIDER: zai` and `AGENT_MODEL: glm-5.3-flash` in its env. Edit
   the env vars; nothing in code needs to change.
6. **First run**: label a standalone issue `agent:implement` and watch the
   Actions tab. A failure comments on the issue with the reason and labels
   it `agent:blocked`; re-add the trigger label to retry.
7. **Record your one-way doors**: in your repo's agent docs (e.g.
   `CONTEXT.md`), list the changes your project always treats as
   irreversible — schema migrations, anything that ships outward or
   deletes. The review pass reads them when auditing the Merge Danger
   call in the PR body.

Sanity checks after cloning:

```sh
npm install
npm run typecheck
npm test
```

## Attribution

Pipeline files ported from [mattpocock/sandcastle](https://github.com/mattpocock/sandcastle)
@ `a53e227250fb51df0ad54155b85fecc1c090c937` — © Matt Pocock, MIT.
Template scaffolding — © tarouttoo, MIT. See `LICENSE`.

## Deviations from upstream

Verbatim is the goal; every deviation is listed here.

1. **Runner swap** (the one deliberate adaptation):
   - `claudeAgent()` in `.sandcastle/agent-workflows/shared/common.ts` is
     replaced by `agentFromEnv()`, which runs pi over the provider/model
     named by the `AGENT_PROVIDER`/`AGENT_MODEL` env vars, authenticated
     with `ZAI_API_KEY` (upstream: `sandcastle.claudeCode` with a pinned
     model and `CLAUDE_CODE_OAUTH_TOKEN`).
   - Workflows install pi CLI (`npm install -g --ignore-scripts @earendil-works/pi-coding-agent`)
     instead of Claude Code, and pass `ZAI_API_KEY` instead of
     `CLAUDE_CODE_OAUTH_TOKEN`.
   - `node-version` 22 → 24.
2. **`npm run build` step removed in all five workflows** — upstream builds
   the sandcastle package before running workflow code; this template has
   nothing to build (`@ai-hero/sandcastle` is consumed as an npm
   dependency).
3. **Scaffolding is fresh, not upstream's** — upstream's `package.json` /
   `tsconfig.json` cover the sandcastle package itself and never type-check
   its `.sandcastle/` workflow starters; this template does, so it adds a
   `@standard-schema/spec` devDependency (a type-only import used by the
   ported code) and a `tsconfig.json` including `.sandcastle/**/*.ts` and
   `docs/**/*.ts` (the repo's doc-contract tests).
4. **Agent-drafted PR bodies** — the implement agent drafts the draft-PR
   body per the shipped `.agents/skills/pr/` skill (Summary / Evidence /
   Merge Danger, from mattpocock/skills, itself adapted from Humanlayer's
   show-me) instead of the canned stub; the stub remains only as fallback.
   The `implement-pr` feedback pass refreshes a stale body, and the review
   pass audits the body's claims against the diff but never edits it
   (rationale: `docs/adr/0001-agent-drafted-pr-bodies.md`; how to read a
   body when deciding to merge: [`docs/pr-body.md`](docs/pr-body.md)).
5. **Child-of-spec Tickets are implementable** — the implement preflight
   admits a Ticket whose parent is another issue (upstream refuses any
   sub-issue) and the implement pass pulls the parent Spec in as a
   `PARENT_CONTEXT` prompt arg: upstream's own local parallel-planner
   prompt ("If it has a parent PRD, pull that in too", pinned `a53e227`),
   which its CI workflow never reached because of that refusal. The
   PRD-shaped refusal and every other guardrail are unchanged
   (rationale: `docs/adr/0002-child-of-spec-tickets-implementable.md`).
6. **`docs/agents/` and the `triage-repro` skill restored** — the port
   initially dropped upstream's agent-facing docs; they are carried back
   verbatim, except the right-hand column of the label table in
   [`docs/agents/triage.md`](docs/agents/triage.md), which that file tells
   adopters to fill in: mapped to this template's labels (`agent:explore`,
   `agent:blocked`, `agent:implement`, `ready-for-human`; `wontfix` closes
   the issue — no label). Note that `docs/agents/adding-an-agent-provider.md`,
   `docs/agents/adding-an-issue-tracker.md` and the reproduction steps in
   `.agents/skills/triage-repro/SKILL.md` describe the upstream sandcastle
   package (`src/`, `sandcastle init`, Docker sandbox); this template
   consumes the pipeline as an npm dependency, so those references point at
   upstream, not at files in this repo.
7. **`Open draft PR` fails hard when `gh pr create` fails** — the step sets
   `set -euo pipefail` so the `gh pr create | tail -n1` pipe cannot swallow
   the exit code (upstream: the failure surfaces as an empty `pr_number` and
   the run instead fails later in `Request automated review`, masking the
   real cause). Regression-tested in
   `.sandcastle/agent-workflows/shared/open-draft-pr.test.ts`, which runs
   the step script with a stubbed `gh`.
8. **Branch-pushing workflows check out with `AGENT_PAT` when available** —
   `agent-implement`, `agent-implement-pr`, `agent-review` and
   `agent-update-branch` pass `token: ${{ secrets.AGENT_PAT ||
   secrets.GITHUB_TOKEN }}` to `actions/checkout`, so `git push` uses the
   PAT's persisted credentials while the `gh` API calls keep using
   `GITHUB_TOKEN`. Upstream never pushes commits that create or update
   files under `.github/workflows/`; this repo's agents build the pipeline
   itself, and a plain `GITHUB_TOKEN` cannot push such commits — GitHub
   refuses them ("refusing to allow a GitHub App to create or update
   workflow … without `workflows` permission"), and `workflows` is not a
   grantable scope in the Actions `permissions` block, so the token, not
   the permission, is the only lever. (This failed two review runs on the
   ready-for-agent converter at their final "Push branch" step, after the
   agent had finished, landing the PR `agent:blocked`;
   `agent-implement` was immune only because it already checked out with
   `AGENT_PAT`.) Without `AGENT_PAT` the `GITHUB_TOKEN` fallback still
   pushes everything except workflow-file changes. Contract-tested in
   `.sandcastle/agent-workflows/shared/push-permissions.test.ts`.
9. **The `ready-for-agent` converter** — a sixth workflow,
   `agent-convert.yml`, with no upstream counterpart: Tickets labelled
   `ready-for-agent` (the triage skills' vocabulary, published by
   construction by its `to-spec`/`to-tickets` flow) are handed to the
   implement workflow once their blocking edges are all closed, re-firing on
   `issues: closed` — no schedule. Edges come from the tracker's native
   blocked-by relationships where available, falling back to parsing the
   body's `## Blocked by` section in TypeScript (bodies stay untrusted
   input, never in the shell); the hand-off label is added with `AGENT_PAT`,
   degrading with the same warning explore's apply job emits when only
   `GITHUB_TOKEN` is available (rationale:
   [`docs/adr/0003-ready-for-agent-converter.md`](docs/adr/0003-ready-for-agent-converter.md);
   day-to-day usage: [`docs/triage.md`](docs/triage.md)).

Prompt and workflow code text is otherwise unchanged from upstream.
