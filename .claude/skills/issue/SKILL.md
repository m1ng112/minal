---
name: issue
description: "Fetch a GitHub issue and work through the full development workflow: Planning → Review → Implement → Review → iterate until complete."
argument-hint: "[issue number or URL]"
---

Work on the GitHub issue specified below, following the full development workflow.

## Target Issue

$ARGUMENTS

## Instructions

### 1. Fetch Issue Details

Use `gh` CLI to retrieve the issue:

```bash
# If given just a number:
gh issue view <number>

# If given a URL:
gh issue view <url>
```

Read the issue title, description, labels, and any linked issues/PRs to fully understand the requirements.

### 2. Planning (設計)

Use the `planner` agent to create a detailed implementation plan:

- Analyze the issue requirements and acceptance criteria
- Research the relevant codebase areas
- Identify affected crates and files
- Create a phased implementation plan
- Consider architectural impact (3-thread model, crate dependencies)

### 3. Planning Review (設計レビュー)

Review the plan yourself before implementing:

- Verify alignment with existing architecture
- Check crate dependency rules (minal-config and minal-core have no internal deps)
- Ensure the plan addresses all issue requirements
- Identify risks or missing considerations
- Revise the plan if needed

### 4. Implement (実装 - 1st pass)

Use the `implementer` agent to execute the plan:

- Follow the plan step by step
- Write code following project conventions
- Add tests for new functionality
- Verify:
  ```bash
  cargo fmt --all
  cargo clippy --workspace -- -D warnings
  cargo test --workspace
  ```

### 5. Review (レビュー - 1st pass)

Use the `code-reviewer` agent to review the implementation:

- Check for CRITICAL issues (unwrap, unsafe, secrets, injection)
- Check for HIGH issues (error handling, logging, duplication)
- Check for MEDIUM issues (performance, testing, compatibility)
- List all findings with severity

### 6. Iterate (修正・再レビュー)

If the review found issues:

1. Fix all CRITICAL and HIGH findings
2. Address MEDIUM findings where practical
3. Re-run verification (`cargo fmt`, `cargo clippy`, `cargo test`)
4. Re-review until the verdict is **Approve**

Repeat implementation and review cycles until quality is satisfactory.

### 7. Commit & Summarize

After all reviews pass:

1. Stage and commit the changes with a descriptive message
2. Reference the issue in the commit: `Closes #<issue-number>` or `Refs #<issue-number>`
3. Push to the current working branch
4. Provide a summary of:
   - What was implemented
   - Files changed
   - Tests added
   - Any remaining considerations or follow-up items
