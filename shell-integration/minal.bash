# Minal shell integration for bash
# Provides OSC 133 semantic prompt markers for structured shell communication.
#
# Source this file in your .bashrc:
#   [[ "$TERM_PROGRAM" == "minal" ]] && source "$MINAL_SHELL_INTEGRATION/minal.bash"

# Guard against double-sourcing.
[[ -n "$MINAL_SHELL_INTEGRATION_LOADED" ]] && return
export MINAL_SHELL_INTEGRATION_LOADED=1

__minal_command_started=0

__minal_prompt_command() {
    local exit_code=$?

    # D: command completed with exit code (only if C was sent).
    if [[ "$__minal_command_started" == "1" ]]; then
        builtin printf '\e]133;D;%d\a' "$exit_code"
        __minal_command_started=0
    fi

    # A: prompt start.
    builtin printf '\e]133;A\a'
}

__minal_preexec() {
    # The DEBUG trap fires for every command including PROMPT_COMMAND itself.
    # Filter those out to avoid spurious C markers.
    if [[ "$BASH_COMMAND" == "$PROMPT_COMMAND" ]] || \
       [[ "$BASH_COMMAND" == "__minal_prompt_command"* ]]; then
        return
    fi

    if [[ "$__minal_command_started" == "0" ]]; then
        __minal_command_started=1
        # C: command execution start.
        builtin printf '\e]133;C\a'
    fi
}

# Install PROMPT_COMMAND, preserving any existing value.
if [[ -z "$PROMPT_COMMAND" ]]; then
    PROMPT_COMMAND="__minal_prompt_command"
elif [[ "$PROMPT_COMMAND" != *"__minal_prompt_command"* ]]; then
    PROMPT_COMMAND="__minal_prompt_command;${PROMPT_COMMAND}"
fi

# Install DEBUG trap for preexec emulation, chaining any existing trap.
__minal_existing_debug_trap=$(trap -p DEBUG 2>/dev/null | sed "s/trap -- '\\(.*\\)' DEBUG/\\1/")
if [[ -n "$__minal_existing_debug_trap" ]]; then
    trap '__minal_preexec; '"$__minal_existing_debug_trap" DEBUG
else
    trap '__minal_preexec' DEBUG
fi
unset __minal_existing_debug_trap

# B: command input start – appended to PS1 so it fires after the prompt.
PS1="${PS1}\[\e]133;B\a\]"
