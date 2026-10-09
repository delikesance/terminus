# Terminus shell integration for fish.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished), OSC 633;E (the command line itself) and OSC 7 (current directory)
# so Terminus can record the command history of this machine. Add this line to ~/.config/fish/config.fish:
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

# OSC 633;E text: `\` as `\\`, `;` and control characters as `\xNN`.
function __terminus_escape
    set -l out
    for c in (string split -- '' "$argv[1]")
        switch $c
            case '\\'
                set -a out '\\\\'
            case ';'
                set -a out '\\x3b'
            case '*'
                if string match -qr '^[\x00-\x1f\x7f]$' -- $c
                    set -a out (printf '\\\\x%02x' "'$c")
                else
                    set -a out $c
                end
        end
    end
    string join -- '' $out
end

function __terminus_preexec --on-event fish_preexec
    # $argv is the command line as typed: send it so Terminus never has to
    # read it back off a screen that the line editor may have redrawn.
    printf '\e]633;E;%s\a' (__terminus_escape "$argv")
    printf '\e]133;C\a'
end

function __terminus_postexec --on-event fish_postexec
    printf '\e]133;D;%s\a' $status
end

function __terminus_cwd --on-variable PWD
    printf '\e]7;file://%s%s\a' (hostname) (string escape --style=url -- $PWD)
end
__terminus_cwd

# `ssh` passes our TERM to the remote, and no host ships a terminfo entry for
# `xterm-rio`: the line editor then cannot move the cursor and `tput`/`htop`
# fail. Advertise the entry every host has, for this one command only. A
# `ssh` function of your own is left alone.
if not functions -q ssh
    function ssh --wraps ssh
        switch "$TERM"
            case xterm-rio rio
                TERM=xterm-256color command ssh $argv
            case '*'
                command ssh $argv
        end
    end
end
