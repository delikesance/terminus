# Terminus shell integration for zsh.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished) and OSC 7 (current directory) so Terminus can record the command
# history of this machine. Terminus loads this automatically for local shells;
# for any other machine add this line to ~/.zshrc there:
#
#   [[ -n $TERMINUS_SHELL_INTEGRATION ]] || source /path/to/terminus.zsh
#
# Commands that start with a space are not recorded (like HISTCONTROL).
# Set TERMINUS_HISTORY=0 in Terminus's environment to stop recording.

[[ -o interactive ]] || return 0
(( ${+__terminus_integrated} )) && return 0
typeset -g __terminus_integrated=1
typeset -g __terminus_ran=

__terminus_precmd() {
  local status=$?
  if [[ -n $__terminus_ran ]]; then
    printf '\e]133;D;%s\a' "$status"
  fi
  __terminus_ran=
  printf '\e]7;file://%s%s\a' "${HOST-}" "${PWD// /%20}"
  # Prompt themes rebuild PS1 on every prompt: wrap it again when needed.
  if [[ $PS1 != *']133;A'* ]]; then
    PS1=$'%{\e]133;A\a%}'"$PS1"$'%{\e]133;B\a%}'
  fi
  return $status
}

__terminus_preexec() {
  __terminus_ran=1
  printf '\e]133;C\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __terminus_precmd
add-zsh-hook preexec __terminus_preexec
