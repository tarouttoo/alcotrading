# TASK

Address unresolved review feedback on PR #{{PR_NUMBER}} on branch `{{BRANCH}}`.

PR title: {{PR_TITLE}}
Linked issue: #{{ISSUE_NUMBER}} {{ISSUE_TITLE}}

This is not a fresh review. Focus on the PR conversation and unresolved feedback.

# LINKED ISSUE

{{LINKED_ISSUE}}

# CURRENT DIFF TO MAIN

```diff
{{DIFF_TO_MAIN}}
```

# PR COMMENTS

```json
{{PR_COMMENTS_JSON}}
```

# PROCESS

For each actionable comment or unresolved thread:

- Change code when the reviewer is right.
- Reply when a reply adds useful context.
- Decline clearly when the requested change is wrong or out of scope.
- Ignore stale/context-only comments.

# PR BODY REFRESH

If addressing the feedback made the PR body stale (Summary, Evidence, or
Merge Danger no longer matches the branch), rewrite it:

1. Read `.agents/skills/pr/SKILL.md` and follow it.
2. Write the full body to `{{PR_BODY_PATH}}`, starting with
   `Closes #{{ISSUE_NUMBER}}` (when the PR links an issue) and ending with
   the line `Body drafted by the implement agent; audited by the review
   pass.`

If the body is still accurate, write nothing. Do not commit the body file;
the workflow applies it to the PR.

Run `npm run typecheck` before committing. Run focused tests where relevant.

If you change code, commit with a conventional commit message.

Do not push.
Do not edit labels.
Do not resolve review threads.
Do not create GitHub comments yourself.

When complete, output `<promise>COMPLETE</promise>`.
