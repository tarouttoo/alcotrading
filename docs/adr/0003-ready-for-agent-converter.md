# A ready-for-agent converter, with no upstream counterpart

The triage skills this pipeline adopts speak in five canonical roles
(`docs/agents/triage.md`); their `to-spec`/`to-tickets` flow publishes a
Spec with child Tickets labelled `ready-for-agent` by construction. Upstream
sandcastle has no workflow that consumes such a label — the only road into
implementation is a human adding `agent:implement` — so a skills-published
backlog lands dead until a human re-labels every Ticket. We add a sixth
workflow, `agent-convert.yml`, that converts `ready-for-agent` Tickets into
`agent:implement` across their blocking edges; upstream has no counterpart
to port, so this lands as a documented deviation instead of a verbatim port.

## Design

- **The label is the approval.** Only Tickets carrying `ready-for-agent` are
  candidates; the converter never inspects unlabelled issues on its own
  initiative (a `closed` event only re-scans Tickets that declare a
  dependency on the closed issue).
- **Edges: native first, body as fallback.** Blocked-by edges come from the
  tracker's native relationships (`GET /repos/…/issues/N/dependencies/blocked_by`)
  when that call succeeds, merged with the issue body's `## Blocked by`
  section (the skills' portable format). The merge — not a strict
  native-or-fallback — is deliberate: the native endpoint answers with an
  empty list for Tickets whose blockers were recorded only in the body, and
  trusting that empty answer would fire the implement workflow early. A
  spurious extra edge only delays a conversion; a missed one starts an agent
  run on a blocked Ticket, so the parser errs conservative. All parsing is
  in TypeScript (`blockers.ts`, tested like `batch.ts`); bodies never touch
  the shell.
- **Decide, then act, deterministically.** No agent runs in the workflow.
  Every blocker closed (or none declared) → swap `ready-for-agent` for
  `agent:implement`; any open blocker → comment naming them, keep the label.
  Re-firing rides `issues: closed` (the event a merged PR emits for its
  linked issue, so closing blocker A re-scans dependent Ticket B); there is
  no schedule or polling.
- **The PAT cascade applies.** `agent:implement` is added with `AGENT_PAT`
  so the label event actually starts `agent-implement.yml` — the same
  cascade, and the same no-PAT `::warning::` degrade, as explore's apply
  job. Without `AGENT_PAT` the label is still added, and the run summary
  says the cascade will not fire.
- **The swap keeps re-fires harmless.** Removing `ready-for-agent` on
  conversion means a later re-scan of the same Ticket finds it no longer
  carrying the label and skips it, exactly like explore's live-check.

## Considered options

- **Strict native-or-fallback (body ignored when the native call succeeds)** —
  rejected: the motivating cases (skills-published Tickets) carry blockers
  only in the body while the native endpoint answers `[]` successfully.
- **Schedule/polling for waiting Tickets** — rejected: noisy and slow; the
  `closed` event is exactly the state change that can unblock a dependent.
  The manual nudge is a label cycle — removing and re-adding
  `ready-for-agent` re-fires the `labeled` evaluation — which is also how a
  failed converter run is retried.
- **Letting the explore agent decide convert-or-wait** — rejected: the
  decision is a pure predicate over issue states; an agent adds cost and
  nondeterminism to a gate, which the pipeline keeps deterministic
  everywhere else (explore's apply job validates the agent's plan for the
  same reason).

## Consequences

A new label (`ready-for-agent`) joins the setup loop, and a sixth workflow
joins the port — both absent upstream, so adopters of this template get a
pipeline their skills-driven triage flow can feed without re-labelling.
Mis-parsed or stale body edges surface as a converter comment naming the
open blockers, so the failure mode is a human-readable queue, not a silent
stall or a premature agent run. The implement preflight (ADR 0002) must
admit child-of-spec Tickets, which it now does — otherwise every converted
Ticket would land `agent:blocked` on its first run.
