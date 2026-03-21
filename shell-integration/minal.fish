# Minal shell integration for fish
# Provides OSC 133 semantic prompt markers for structured shell communication.
#
# Source this file in your config.fish:
#   if test "$TERM_PROGRAM" = "minal"
#       source "$MINAL_SHELL_INTEGRATION/minal.fish"
#   end

# Guard against double-sourcing.
if set -q MINAL_SHELL_INTEGRATION_LOADED
    return
end
set -gx MINAL_SHELL_INTEGRATION_LOADED 1

function __minal_fish_prompt --on-event fish_prompt
    set -l exit_code $status

    # D: command completed with exit code (only if C was sent).
    if set -q __minal_command_started
        printf '\e]133;D;%d\a' $exit_code
        set -e __minal_command_started
    end

    # A: prompt start.
    printf '\e]133;A\a'
end

function __minal_fish_postprompt --on-event fish_postexec
    # Handled by fish_prompt; this hook is reserved for future use.
end

function __minal_fish_preexec --on-event fish_preexec
    set -g __minal_command_started 1

    # C: command execution start.
    printf '\e]133;C\a'
end

# B: command input start – emitted after the prompt via a separate
# fish_prompt handler. Fish fires all fish_prompt handlers in order,
# so this appends after the main prompt function renders.
function __minal_fish_prompt_b --on-event fish_prompt
    printf '\e]133;B\a'
end
