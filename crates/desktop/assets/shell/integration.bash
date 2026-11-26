# Bootstrapped by the inherited PROMPT_COMMAND when the user's rc preserves it.
[[ $- == *i* ]] || return
[[ ${_guild_loaded-} ]] && return
_guild_loaded=1
_guild_cwd() {
  local LC_ALL=C value=$PWD out='' c i
  for ((i=0;i<${#value};i++)); do
    c=${value:i:1}
    case $c in [a-zA-Z0-9/._~-]) out+=$c;; *) printf -v c '%%%02X' "'$c"; out+=$c;; esac
  done
  printf '\e]7;file://%s\a' "$out"
}
_guild_precmd() {
  local code=$?
  [[ ${_guild_running-} ]] && printf '\e]133;D;%d\a' "$code"
  unset _guild_running
  _guild_cwd
  printf '\e]133;A\a'
  if [[ $PS1 != *$'\e]133;B\a'* ]]; then PS1+=$'\[\e]133;B\a\]'; fi
  return "$code"
}
_guild_preexec() {
  [[ ${BASH_SUBSHELL:-0} -eq 0 ]] || return
  [[ $PROMPT_COMMAND == *"$BASH_COMMAND"* ]] && return
  [[ ${_guild_last_pwd-} == "$PWD" ]] || { _guild_cwd; _guild_last_pwd=$PWD; }
  [[ ${_guild_running-} ]] && return
  case $BASH_COMMAND in _guild_*|*GUILD_SHELL_BOOTSTRAP*) return;; esac
  printf '\e]133;C\a'; _guild_running=1
}
# Never replace an existing DEBUG trap. bash-preexec arrays are additive.
if [[ ${bash_preexec_imported-}${__bp_imported-} ]]; then
  preexec_functions+=(_guild_preexec)
  PROMPT_COMMAND="_guild_precmd${PROMPT_COMMAND#"$GUILD_SHELL_BOOTSTRAP_COMMAND"}"
elif [[ ! $(trap -p DEBUG) ]]; then
  PROMPT_COMMAND="_guild_precmd${PROMPT_COMMAND#"$GUILD_SHELL_BOOTSTRAP_COMMAND"}"
  trap '_guild_preexec' DEBUG
else
  PROMPT_COMMAND="_guild_precmd${PROMPT_COMMAND#"$GUILD_SHELL_BOOTSTRAP_COMMAND"}"
fi

# Private bootstrap commands/functions must not infect a nested shell.
export -n PROMPT_COMMAND
unset GUILD_SHELL_BOOTSTRAP GUILD_SHELL_BOOTSTRAP_COMMAND GUILD_SHELL_OLD_PROMPT
