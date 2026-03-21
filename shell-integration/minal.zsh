# Minal shell integration for zsh
# Provides OSC 133 semantic prompt markers for structured shell communication.
#
# Source this file in your .zshrc:
#   [[ "$TERM_PROGRAM" == "minal" ]] && source "$MINAL_SHELL_INTEGRATION/minal.zsh"

# Guard against double-sourcing.
[[ -n "$MINAL_SHELL_INTEGRATION_LOADED" ]] && return
export MINAL_SHELL_INTEGRATION_LOADED=1

# State flag: set when a command has been executed (C marker sent).
typeset -g __minal_command_started

__minal_precmd() {
    local exit_code=$?

    # D: command completed with exit code (only if C was sent).
    if [[ -n "$__minal_command_started" ]]; then
        builtin printf '\e]133;D;%d\a' "$exit_code"
        unset __minal_command_started
    fi

    # A: prompt start.
    builtin printf '\e]133;A\a'
}

__minal_preexec() {
    __minal_command_started=1

    # C: command execution start.
    builtin printf '\e]133;C\a'
}

# Install hooks using the standard zsh composable hook API.
# This coexists with Oh My Zsh, Powerlevel10k, Starship, etc.
autoload -Uz add-zsh-hook
add-zsh-hook precmd __minal_precmd
add-zsh-hook preexec __minal_preexec

# B: command input start – embedded in the prompt so it fires after the
# prompt is rendered but before the user types.
#
# Use a precmd hook that appends the B marker to PS1. This runs after
# all other precmd hooks (including prompt themes) so the marker always
# appears at the end, right before user input.
#
# The marker is stripped before re-adding to prevent PS1 from growing
# unbounded when the user's prompt is static (not regenerated each precmd).
typeset -g __minal_b_marker=$'%{\e]133;B\a%}'

__minal_precmd_mark_input() {
    PS1="${PS1//${__minal_b_marker}/}${__minal_b_marker}"
}
add-zsh-hook precmd __minal_precmd_mark_input
