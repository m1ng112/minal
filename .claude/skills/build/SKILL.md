---
name: build
description: "Run build, test, lint, and format checks for the Minal workspace. Use when verifying code compiles, tests pass, or preparing for commit."
disable-model-invocation: true
allowed-tools: Bash
---

Run the following checks for the Minal workspace. Stop at the first failure and report the error.

## Commands

### Build

```bash
cargo build --workspace
```

### Test

```bash
cargo test --workspace
```

### Lint

```bash
cargo clippy --workspace -- -D warnings
```

### Format check

```bash
cargo fmt --check
```

### Full CI check (all of the above)

If `$ARGUMENTS` includes "ci" or "all", run them all sequentially:

```bash
cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace
```

### Individual crate

If `$ARGUMENTS` specifies a crate name, scope to that crate:

```bash
cargo test -p $ARGUMENTS
cargo clippy -p $ARGUMENTS -- -D warnings
```

### Format fix

If `$ARGUMENTS` includes "fix" or "fmt", auto-fix formatting:

```bash
cargo fmt --all
```

## Reporting

After running, report:
1. Which checks passed
2. Which checks failed (with error output)
3. Suggested fixes for any failures
