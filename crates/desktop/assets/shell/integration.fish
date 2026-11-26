status is-interactive; or return
functions -q _guild_preexec; and return
function _guild_preexec --on-event fish_preexec
  printf '\e]133;C\a'
end
function _guild_postexec --on-event fish_postexec
  printf '\e]133;D;%d\a' $status
end
function _guild_cwd --on-variable PWD
  printf '\e]7;file://%s\a' (string escape --style=url -- $PWD | string replace -a '%2F' '/')
end
function _guild_prompt --on-event fish_prompt
  printf '\e]7;file://%s\a' (string escape --style=url -- $PWD | string replace -a '%2F' '/')
  printf '\e]133;A\a'
end
if functions -q fish_prompt
  functions -c fish_prompt _guild_original_prompt
  function fish_prompt
    _guild_original_prompt
    printf '\e]133;B\a'
  end
end
