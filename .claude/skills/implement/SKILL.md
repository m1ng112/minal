---
name: implement
description: "Execute an implementation task or plan. Writes code across crates, runs tests, and ensures the build passes. Can target a specific crate or work across the workspace."
disable-model-invocation: true
context: fork
agent: implementer
argument-hint: "[task description or 'follow plan']"
---

Implement the following:

$ARGUMENTS

## Instructions

### If given a specific task:

1. Research the relevant code first — read files, understand patterns
2. Implement the change following project conventions
3. Add tests for new functionality
4. Run verification:
   ```bash
   cargo fmt --all
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```
5. Summarize what was done

### If told to "follow plan" or given a plan:

1. Read the plan carefully
2. Execute steps in the specified order, respecting dependencies
3. After each phase, verify the build still passes
4. Report progress and any deviations from the plan

## Key Conventions

- **Error handling**: `thiserror` custom types, no `unwrap()`
- **Logging**: `tracing` macros
- **Unsafe**: Only in PTY/FFI (minal-core), with `// SAFETY:` comment
- **Threading**: crossbeam channels for communication, `Arc<Mutex<>>` for shared state
- **Style**: `cargo fmt`, doc comments on public APIs

## Crate Dependency Order (build bottom-up)

1. `minal-config` (no internal deps)
2. `minal-core` (no internal deps)
3. `minal-ai` (depends on minal-core, minal-config)
4. `minal-renderer` (depends on minal-core)
5. `minal` bin (depends on all)
