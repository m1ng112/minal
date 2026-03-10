---
name: shell-integration
description: "Create or update shell integration scripts for OSC 133 protocol support. Use when working on Zsh/Bash/Fish shell hooks for prompt detection and command tracking."
argument-hint: "[shell: zsh|bash|fish]"
---

Create or update shell integration for `$ARGUMENTS` in `shell-integration/`.

## Target Files

- `shell-integration/minal.zsh`: Zsh (precmd/preexec hooks)
- `shell-integration/minal.bash`: Bash (PROMPT_COMMAND hook)
- `shell-integration/minal.fish`: Fish (fish_prompt/fish_preexec)

## OSC 133 Protocol

```
OSC 133;A ST  -> Prompt start
OSC 133;B ST  -> Command input start
OSC 133;C ST  -> Command execution start
OSC 133;D;{exit_code} ST  -> Command finished
```

## Zsh Template

```zsh
# Minal Shell Integration for Zsh
if [[ "$TERM_PROGRAM" != "minal" ]]; then
  return
fi

__minal_precmd() {
  local exit_code=$?
  # Notify command finished
  printf '\e]133;D;%d\a' "$exit_code"
  # Notify prompt start
  printf '\e]133;A\a'
}

__minal_preexec() {
  # Notify command execution start
  printf '\e]133;C\a'
}

precmd_functions+=(__minal_precmd)
preexec_functions+=(__minal_preexec)

# Send A before first prompt
printf '\e]133;A\a'
```

## Terminal-Side Processing

- Parse OSC 133 in `crates/minal-core/src/handler.rs` `osc_dispatch`
- Track prompt/command/output state in `ShellIntegration` struct
- Generate `CommandRecord` on command completion -> auto-add to AI context
