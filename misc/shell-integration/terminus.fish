# Terminus shell integration for fish.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished) and OSC 7 (current directory) so Terminus can record the command
# history of this machine. Add this line to ~/.config/fish/config.fish:
#
#   test -n "$TERMINUS_SHELL_INTEGRATION"; or source /path/to/terminus.fish
#
# Commands that start with a space are not recorded (like HISTCONTROL).
# Set TERMINUS_HISTORY=0 in Terminus's environment to stop recording.

status is-interactive; or exit 0
set -q __terminus_integrated; and exit 0
set -g __terminus_integrated 1

functions -c fish_prompt __terminus_orig_prompt 2>/dev/null

function fish_prompt
    printf '\e]133;A\a'
    __terminus_orig_prompt
    printf '\e]133;B\a'
end

function __terminus_preexec --on-event fish_preexec
    printf '\e]133;C\a'
end

function __terminus_postexec --on-event fish_postexec
    printf '\e]133;D;%s\a' $status
end

function __terminus_cwd --on-variable PWD
    printf '\e]7;file://%s%s\a' (hostname) (string escape --style=url -- $PWD)
end
__terminus_cwd
