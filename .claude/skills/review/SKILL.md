---
name: review
description: "Review recent code changes for quality, safety, and project conventions. Produces a severity-organized report with actionable feedback."
disable-model-invocation: true
context: fork
agent: code-reviewer
argument-hint: "[scope: 'staged', 'branch', or file path]"
---

Review the code changes specified below and produce a detailed report.

## Scope

$ARGUMENTS

- If scope is **"staged"** or empty: review `git diff --cached`
- If scope is **"branch"**: review `git diff origin/main...HEAD`
- If scope is a **file path**: review that specific file
- If scope is a **crate name** (e.g. "minal-core"): review changes in that crate

## Review Process

1. **Gather changes**: Run the appropriate git diff or read the specified files
2. **Read context**: For each changed file, read surrounding code to understand the full picture
3. **Apply checklist**: Check every item below
4. **Report findings**: Organize by severity

## Checklist

### CRITICAL (must fix)
- No `unwrap()` outside tests
- `unsafe` only in PTY/FFI with `// SAFETY:` comment
- No hardcoded secrets or API keys
- No command injection vulnerabilities
- AI context respects `[ai.privacy]` settings
- No circular crate dependencies

### HIGH (should fix)
- Error handling uses `thiserror` custom types
- Logging uses `tracing` macros
- No duplicated code
- Public APIs have doc comments
- Ownership/borrowing is correct and efficient
- No unnecessary cloning

### MEDIUM (consider)
- No allocations in hot paths (render loop, VT parser)
- Appropriate `&str` vs `String` usage
- Thread communication uses crossbeam channels correctly
- Config changes are backward-compatible
- Tests cover new functionality

## Output Format

```markdown
## Review Summary

| Severity | Count |
|----------|-------|
| CRITICAL | N     |
| WARNING  | N     |
| SUGGESTION | N  |

**Verdict**: Approve / Needs Changes / Block

## Findings

### CRITICAL
1. **[Issue title]** (`path/to/file.rs:LINE`)
   - Problem: ...
   - Fix: ...

### WARNING
...

### SUGGESTION
...
```
