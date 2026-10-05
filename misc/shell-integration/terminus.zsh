# Terminus shell integration for zsh.
#
# Emits OSC 133 (A prompt start, B prompt end, C command submitted, D command
# finished), OSC 633;E (the command line itself) and OSC 7 (current directory)
# so Terminus can record the command history of this machine. Terminus loads this automatically for local shells;
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
  # Not `status`: zsh's `$status` is a read-only alias of `$?`.
  local ret=$?
  if [[ -n $__terminus_ran ]]; then
    printf '\e]133;D;%s\a' "$ret"
  fi
  __terminus_ran=
  printf '\e]7;file://%s%s\a' "${HOST-}" "${PWD// /%20}"
  # Prompt themes rebuild PS1 on every prompt: wrap it again when needed.
  if [[ $PS1 != *']133;A'* ]]; then
    PS1=$'%{\e]133;A\a%}'"$PS1"$'%{\e]133;B\a%}'
  fi
  return $ret
}

# OSC 633;E text: `\` as `\\`, `;` and control characters as `\xNN`.
__terminus_escape() {
  emulate -L zsh
  local s=$1 out= c hex
  local -i i
  for (( i = 1; i <= ${#s}; i++ )); do
    c=${s[i]}
    case $c in
      ('\') out+='\\' ;;
      (';') out+='\x3b' ;;
      ([[:cntrl:]])
        printf -v hex '\\x%02x' "'$c"
        out+=$hex
        ;;
      (*) out+=$c ;;
    esac
  done
  print -rn -- "$out"
}

__terminus_preexec() {
  __terminus_ran=1
  # $1 is the command line as typed: send it so Terminus never has to read
  # it back off a screen that the line editor may have redrawn.
  printf '\e]633;E;%s\a' "$(__terminus_escape "$1")"
  printf '\e]133;C\a'
}

autoload -Uz add-zsh-hook
add-zsh-hook precmd __terminus_precmd
add-zsh-hook preexec __terminus_preexec
