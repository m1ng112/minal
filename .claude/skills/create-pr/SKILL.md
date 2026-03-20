---
name: create-pr
description: Create a GitHub pull request with auto-generated title and body from branch changes. Use when the user asks to create a PR, open a pull request, or submit changes for review. Analyzes git diff and commit history to produce a structured PR following project conventions.
---

# Create PR

Create a GitHub pull request by analyzing branch changes and generating a structured title and body.

## Workflow

### 1. Gather context

Run these in parallel:

```bash
git status
git diff --staged --stat
git log --oneline main..HEAD
git diff main...HEAD --stat
git diff main...HEAD
```

Determine:
- Current branch name
- Base branch (default: `main`, check `claude/ai-terminal-app-plan-QhYNo` as alternative)
- All commits since divergence
- All file changes (not just latest commit)
- Whether branch is pushed to remote

### 2. Identify linked issues

- Check branch name for issue references (e.g., `claude/issue-4-*` → Issue #4)
- Check commit messages for `#N` references
- If found, fetch issue details: `gh issue view <N>`

### 3. Determine PR type from changes

- `feat:` — new files, new modules, new functionality
- `fix:` — bug corrections, error handling fixes
- `refactor:` — restructuring without behavior change
- `docs:` — documentation-only changes
- `test:` — test additions/modifications
- `chore:` — CI, build config, dependencies

### 4. Generate title and body

Read [references/pr-template.md](references/pr-template.md) for the template format and conventions.

**Title**: `<type>: <concise description>` (under 70 chars). Append `(Issue #N)` if linked.

**Body**: Follow the template structure — Summary, Changes table, Test plan, issue link, footer.

### 5. Run verification checks

Before creating the PR, run and report results:

```bash
cargo build 2>&1
cargo test --workspace 2>&1
cargo clippy --workspace -- -D warnings 2>&1
cargo fmt --check 2>&1
```

Mark passing checks as `[x]` in the test plan. If any fail, inform the user and ask whether to proceed or fix first.

### 6. Push and create PR

```bash
git push -u origin <branch-name>
gh pr create --title "<title>" --body "$(cat <<'EOF'
<generated body>
EOF
)"
```

If a base branch other than `main` is appropriate, add `--base <branch>`.

Report the PR URL to the user when done.
