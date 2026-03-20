# PR Body Template

```markdown
## Summary

- <concise bullet points describing what changed and why>

## Changes

| File | Description |
|------|-------------|
| `path/to/file` | Brief description of change |

## Test plan

- [x] `cargo build` passes
- [x] `cargo test --workspace` passes
- [x] `cargo clippy --workspace -- -D warnings` zero warnings
- [x] `cargo fmt --check` passes
- [ ] <additional manual or CI verification items>

Closes #<issue-number>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

## Title Conventions

- feat: <description> — new feature
- fix: <description> — bug fix
- refactor: <description> — code restructuring
- docs: <description> — documentation only
- test: <description> — test additions/changes
- chore: <description> — build, CI, dependencies

Append `(Issue #N)` when linked to an issue.

Examples:
- `feat: ウィンドウ + wgpu 初期化 (Issue #4)`
- `fix: cursor clamping on terminal resize`
- `docs: Add development workflow documentation to CLAUDE.md`

## Summary Guidelines

- Use bullet points, not paragraphs
- Lead with what changed, then why
- Include architectural decisions if significant
- Mention new crates, modules, or files introduced

## Changes Table Guidelines

- List every file with meaningful changes
- Group by crate/directory when many files change
- Keep descriptions to one line each

## Test Plan Guidelines

- Always include the 4 standard checks (build, test, clippy, fmt)
- Mark automated checks as [x] if verified before PR creation
- Add manual verification items as [ ] (unchecked)
- Add CI-specific items as [ ] (unchecked)
