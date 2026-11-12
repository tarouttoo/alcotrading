# TASK

Implement issue #{{ISSUE_NUMBER}}: {{ISSUE_TITLE}}

You are on branch `{{BRANCH}}`, already created from `main`.

# ISSUE

{{ISSUE_CONTEXT}}

# PARENT SPEC

{{PARENT_CONTEXT}}

If a parent spec is shown above, it is background: it explains why this
Ticket exists and what it is part of. Implement only issue #{{ISSUE_NUMBER}}
— never the spec itself or its other Tickets.

# CONTEXT

Read the project's domain and architecture docs before changing code:

- `CONTEXT.md`
- `docs/adr/` if relevant
- `.sandcastle/CODING_STANDARDS.md`

Explore the repo and relevant tests before editing.

# EXECUTION

Where a test seam already exists, or a new one is being proposed, do red-green-refactor:

1. RED: write a failing test
2. GREEN: implement the smallest correct change
3. REPEAT until the issue is done
4. REFACTOR

Do not improvise new test seams, such as extracting out a function so that it can be tested in isolation. This creates spaghetti tests.

Run `npm run typecheck` before committing. Run focused tests where relevant.

# COMMIT

Make one or more commits on `{{BRANCH}}` with conventional commit messages.

Do not push the branch.
Do not close the issue.
Do not edit labels.
Do not create or edit PRs.

# PR BODY

After the final commit, write the pull request body to `{{PR_BODY_PATH}}`:

1. Read `.agents/skills/pr/SKILL.md` and follow it.
2. Start the body with `Closes #{{ISSUE_NUMBER}}`, then the skill's
   Summary, Evidence, and Merge Danger sections.
3. End with exactly this line:

   Body drafted by the implement agent; audited by the review pass.

Evidence must come from commands you actually ran in this checkout. Do not
commit the body file; the workflow opens the PR from it.

When complete, output `<promise>COMPLETE</promise>`.
