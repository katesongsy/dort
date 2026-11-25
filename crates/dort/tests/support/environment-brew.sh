case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\n' "$HOME/prefix";;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --installed --formula')
  printf Q >> "$HOME/calls"
  printf '{"formulae":['
  if [ -f "$HOME/a" ]; then printf '{"full_name":"a","installed":[{"version":"1.2.3"}]},'; fi
  if [ -f "$HOME/b" ]; then printf '{"full_name":"b","installed":[{"version":"1.2.3"}]},'; fi
  printf '{"full_name":"unrelated","installed":[{"version":"1.2.3"}]}]}';;
'info --json=v2 --formula homebrew/core/'*)
  for arg in "$@"; do :; done
  name=${arg##*/}
  printf '{"formulae":[{"name":"%s","full_name":"%s","tap":"homebrew/core","disabled":false,"versions":{"stable":"1.2.3"},"installed":[' "$name" "$name"
  if [ -f "$HOME/$name" ]; then printf '{"version":"1.2.3"}'; fi
  printf ']}]}';;
'install --formula --no-ask --skip-post-install homebrew/core/'*)
  for arg in "$@"; do :; done
  name=${arg##*/}
  if [ "$name" = a ]; then printf A >> "$HOME/calls"; else
    printf B >> "$HOME/calls"
    [ -f "$HOME/a" ] || exit 10
    [ ! -e "$HOME/.editor" ] || exit 11
  fi
  keg="$HOME/prefix/Cellar/$name/1.2.3/bin"
  /bin/mkdir -p "$keg" || exit 12
  printf '#!/bin/sh\nexit 0\n' > "$keg/$name"
  /bin/chmod 700 "$keg/$name" || exit 12
  /bin/ln -s "$keg/$name" "${PATH%%:*}/$name" || exit 12
  printf installed > "$HOME/$name"
  if [ "$name" = b ] && [ -f "$HOME/hold-b" ]; then
    printf ready > "$HOME/provider-ready"
    while [ -f "$HOME/hold-b" ]; do /bin/sleep 0.02; done
    printf done > "$HOME/provider-done"
  fi
  if [ "$name" = b ] && [ -f "$HOME/fail-b" ]; then exit 9; fi;;
*) exit 90;;
esac
