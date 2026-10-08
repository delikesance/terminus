# Terminus shell integration for bash.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished), OSC 633;E (the command line itself) and OSC 7 (current directory)
# so Terminus can record the command history of this machine. Terminus sources this automatically for local
# shells; for any other machine add this line to ~/.bashrc there:
#
#   [ -n "$TERMINUS_SHELL_INTEGRATION" ] || source /path/to/terminus.bash
#
# Commands that start with a space are not recorded (like HISTCONTROL).
# Set TERMINUS_HISTORY=0 in Terminus's environment to stop recording.

[[ $- == *i* ]] || return 0
[[ -n ${__terminus_integrated-} ]] && return 0
__terminus_integrated=1

# Drop the one-shot bootstrap Terminus puts in PROMPT_COMMAND.
if [[ -n ${TERMINUS_SHELL_INTEGRATION-} ]]; then
  __terminus_boot='. "$TERMINUS_SHELL_INTEGRATION/terminus.bash"'
  if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
    PROMPT_COMMAND=("${PROMPT_COMMAND[@]//"$__terminus_boot"/:}")
  else
    PROMPT_COMMAND=${PROMPT_COMMAND//"$__terminus_boot"/:}
  fi
  unset __terminus_boot
fi

__terminus_precmd() {
  local status=$?
  printf '\e]133;D;%s\a' "$status"
  printf '\e]7;file://%s%s\a' "${HOSTNAME-}" "${PWD// /%20}"
  # History number the next command gets if bash records it.
  __terminus_histcmd=${HISTCMD-}
  return "$status"
}

# OSC 633;E text: `\` as `\\`, `;` and control characters as `\xNN`.
__terminus_escape() {
  local s=$1 out= c i hex
  for ((i = 0; i < ${#s}; i++)); do
    c=${s:i:1}
    case $c in
      \\) out+='\\' ;;
      \;) out+='\x3b' ;;
      [[:cntrl:]])
        printf -v hex '\\x%02x' "'$c"
        out+=$hex
        ;;
      *) out+=$c ;;
    esac
  done
  printf '%s' "$out"
}

# Runs from PS0 (in a subshell, after Enter, before the command): send the
# command line as bash recorded it, so Terminus never has to read it back
# off a screen that readline may have redrawn. Nothing is sent when bash did
# not record it (leading space with ignorespace, history off): Terminus then
# falls back to the screen.
__terminus_cmdline() {
  [[ -n ${HISTCMD-} && -n ${__terminus_histcmd-} ]] || return 0
  ((HISTCMD > __terminus_histcmd)) || return 0
  local HISTTIMEFORMAT= line
  line=$(builtin history 1)
  [[ $line =~ ^\ *[0-9]+[\*\ ]\ (.*)$ ]] || return 0
  printf '\e]633;E;%s\a' "$(__terminus_escape "${BASH_REMATCH[1]}")"
}

if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
  PROMPT_COMMAND=(__terminus_precmd "${PROMPT_COMMAND[@]}")
else
  PROMPT_COMMAND="__terminus_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi

PS1='\[\e]133;A\a\]'"$PS1"'\[\e]133;B\a\]'
# PS0 is printed after Enter, before the command runs.
PS0='$(__terminus_cmdline)\e]133;C\a'"${PS0-}"

# The first prompt may already be queued when this file is sourced from
# PROMPT_COMMAND, so announce the directory (and the history number) now.
printf '\e]7;file://%s%s\a' "${HOSTNAME-}" "${PWD// /%20}"
__terminus_histcmd=${HISTCMD-}

# `ssh` passes our TERM to the remote, and no host ships a terminfo entry for
# `xterm-rio`: readline then cannot move the cursor and `tput`/`htop` fail.
# Advertise the entry every host has, for this one command only. A `ssh`
# function or alias of your own is left alone (and `function ssh` rather than
# `ssh()`, which bash would alias-expand into a syntax error).
if ! declare -F ssh >/dev/null && ! alias ssh >/dev/null 2>&1; then
  function ssh {
    case ${TERM-} in
      xterm-rio | rio) TERM=xterm-256color command ssh "$@" ;;
      *) command ssh "$@" ;;
    esac
  }
fi
