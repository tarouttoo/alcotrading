# AGENTS.md

This is a template repo for a label-driven agent pipeline (pi CLI on GitHub
Actions), ported from mattpocock/sandcastle. Agents run here via the
`agent:*` labels; humans and coding agents edit the pipeline code itself.

## Navigation

- **Naming or domain questions** — read `CONTEXT.md` (glossary) first.
- **Changing pipeline behaviour** — check `docs/adr/` for prior decisions;
  add an ADR when making a non-obvious, hard-to-reverse choice.
- **Code style in adopting repos** — `.sandcastle/CODING_STANDARDS.md` is
  what the workflow agents apply; do not edit it lightly, prompts reference
  it verbatim.
- **Skill conventions** (triage labels, issue-tracker commands, domain-doc
  layout) — read `docs/agents/`; the triage-repro skill also lives under
  `.agents/skills/triage-repro/`.
- **Adopter setup and upstream deviations** — `README.md`.

## Constraints

- Files under `.sandcastle/agent-workflows/` and `.github/workflows/` are a
  verbatim port of upstream (modulo the documented deviations in
  `README.md`). Verify against upstream before restructuring.
- Before finishing: `npm run typecheck` and `npm test` must pass.
