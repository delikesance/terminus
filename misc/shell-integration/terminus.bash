# Terminus shell integration for bash.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished) and OSC 7 (current directory) so Terminus can record the command
# history of this machine. Terminus sources this automatically for local
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
  return "$status"
}

if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
  PROMPT_COMMAND=(__terminus_precmd "${PROMPT_COMMAND[@]}")
else
  PROMPT_COMMAND="__terminus_precmd${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi

PS1='\[\e]133;A\a\]'"$PS1"'\[\e]133;B\a\]'
# PS0 is printed after Enter, before the command runs.
PS0='\e]133;C\a'"${PS0-}"

# The first prompt may already be queued when this file is sourced from
# PROMPT_COMMAND, so announce the directory once now.
printf '\e]7;file://%s%s\a' "${HOSTNAME-}" "${PWD// /%20}"
