# The PR body: how to read an agent PR when deciding to merge

Every draft PR has a body drafted by the implement agent: what changed, how
it was verified, and how dangerous it is to land. You merge off the body
plus the review comment, so this page is how to read the two together when
deciding ([ADR 0001](adr/0001-agent-drafted-pr-bodies.md)). The loop around
it — feedback, re-reviews, conflicts — is [docs/triage.md](triage.md); label
semantics in [docs/labels.md](labels.md).

## Where the body comes from

```
implement agent ─▶ drafts body after its final commit ─▶ PR body
review agent    ─▶ audits the body's claims ─▶ review comment
you             ─▶ compare body vs review vs diff ─▶ merge decision
```

- The **implement agent** drafts the body per the shipped
  [pr skill](../.agents/skills/pr/SKILL.md). Its Evidence must come from
  commands it actually ran in its checkout.
- The **review pass** fetches the body and audits its Summary, Evidence,
  and Merge Danger claims against the diff and the linked issue, calling
  out any discrepancy explicitly in its review comment. It never edits the
  body — like a real team, the reviewer works in review comments.
- The two agents are deliberately separate — the author never grades its
  own door call ([ADR 0001](adr/0001-agent-drafted-pr-bodies.md)) — and
  **a discrepancy between body and review is itself signal**: resolve it
  before merging, either way.

## Anatomy of a body

The title is mechanical — `Fix #N: <issue title>`. The body is where the
agent's claims live:

```markdown
Closes #12

## Summary
<diagram, diff-sketch, or tree of what changed>

## Evidence
- **Before:** <failing run / output>
  **After:** <passing run / output>

## Merge Danger
**Door:** one-way or two-way
**Blast Radius:** <one word>

Body drafted by the implement agent; audited by the review pass.
```

- **`Closes #N`** — merging closes the linked issue: the one you labelled.
  One change per issue, so if the Summary claims more than the issue
  asked, that is scope creep to reject.
- **Summary / Evidence / Merge Danger** — the pr skill's three sections,
  each a claim you can check (below).
- **The footer** — every agent-drafted body ends with the disclosure line
  `Body drafted by the implement agent; audited by the review pass.`;
  machine authorship is never hidden.
- **No footer, no sections?** The workflow fell back to the stub
  (`Closes #N` plus "Implemented by the Sandcastle agent workflow.")
  because the agent wrote no body. There are no claims to audit — read the
  diff yourself and review extra hard.

## Reading each claim

### Summary — against the diff

The Summary is the scope claim, as pseudocode, a call or component tree,
a file tree, a Mermaid diagram, or a diff — the smallest of the skill's
views that makes the point. Open the `Files changed` tab and ask: does the
diff do exactly what the Summary shows — no more, no less?

### Evidence — by re-running it

Evidence is before/after pairs from commands the agent ran in its
checkout. The cheap ones are re-runnable: `npm run typecheck`, `npm test`,
whatever the body names. Run them yourself. Evidence that names no
commands ("all tests pass") is unverified; screenshots cannot be re-run —
weigh accordingly.

### Merge Danger — be most suspicious when it says two-way

`Door` (one-way: hard to reverse; two-way: cheap to roll back) and
`Blast Radius` are judgement calls made by the agent that wrote the change
— the exact claim the [ADR](adr/0001-agent-drafted-pr-bodies.md) warns is
cheapest to get wrong. Two checks before you trust it:

- **Your one-way-door list.** Adopters record the changes that are always
  irreversible in their repo (schema migrations, anything that ships
  outward or deletes — see the README's adopting steps); the review pass
  reads that list when auditing this call. A diff touching those paths
  labelled "two-way" is a discrepancy — resolve it.
- **The explore comment, if there was one.** Batch triage estimates blast
  radius and door independently, verified against the code. Disagreement
  between the explore verdict and the PR body is signal.

## Stale bodies

The body reflects the branch as of the last implement pass, not
necessarily HEAD:

- The **implement-pr feedback pass** rewrites the body when your feedback
  made it stale (Summary, Evidence, or Merge Danger no longer matching the
  branch). If it should have been refreshed and wasn't, say so in a
  comment and re-add `agent:implement` to the PR.
- The **review pass** may push self-fixes, and **`agent:update-branch`**
  resolves conflicts — neither rewrites the body (the reviewer never edits
  it). Re-read the Summary against the newer diff before merging.

## Merge checklist

- [ ] `Closes #N` names the issue you labelled
- [ ] Summary matches the diff — you read the diff, not just the body
- [ ] Evidence re-runs clean in your checkout
- [ ] Door call survives your one-way-door list; blast radius makes sense
- [ ] Review comment audited the body — no unresolved discrepancy
- [ ] Threads resolved by you (agents never resolve them)
- [ ] You merge — the pipeline never does
