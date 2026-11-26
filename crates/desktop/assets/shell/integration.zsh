# Scoped to an interactive login shell Guild launched; no rc file is changed.
[[ -o interactive ]] || return
(( ${+functions[_guild_precmd]} )) && return
autoload -Uz add-zsh-hook
_guild_cwd() {
  local LC_ALL=C value=$PWD out='' c i
  for ((i=1;i<=${#value};i++)); do
    c=${value[i]}
    case $c in [a-zA-Z0-9/._~-]) out+=$c;; *) printf -v c '%%%02X' "'$c"; out+=$c;; esac
  done
  printf '\e]7;file://%s\a' "$out"
}
_guild_precmd() {
  local code=$?
  [[ -n ${_guild_running-} ]] && printf '\e]133;D;%d\a' "$code"
  unset _guild_running
  _guild_cwd
  printf '\e]133;A\a'
  return 0
}
_guild_preexec() { printf '\e]133;C\a'; _guild_running=1; }
add-zsh-hook precmd _guild_precmd
# End output before another prompt hook prints; keep their relative order.
precmd_functions=(_guild_precmd ${precmd_functions:#_guild_precmd})
autoload -Uz add-zle-hook-widget
_guild_line_init() { printf '\e]133;B\a'; }
add-zle-hook-widget line-init _guild_line_init
add-zsh-hook preexec _guild_preexec
add-zsh-hook chpwd _guild_cwd
