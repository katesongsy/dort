//! Disposable brew/npm programs for declaration, installed and history journeys.
use crate::services::{declared_tools, declared_tools_tests::Fixture};
use dort_core::model::Provider;
use std::fs;
pub(super) fn fixture(provider: Provider) -> Fixture {
    let mut f = Fixture::new();
    let spec = match provider {
        Provider::Brew => "provider='brew'\npackage='demo'\n",
        Provider::Npm => {
            "provider='npm'\npackage='demo'\nversion='1.2.3'\nruntime_tool='runtime'\n"
        }
    };
    let runtime = if provider == Provider::Npm {
        "[[tools]]\nid='runtime'\nbinary='node'\n[tools.install]\nprovider='brew'\npackage='node'\n"
    } else {
        ""
    };
    fs::write(
        f.repo.join("dort.toml"),
        format!("schema=1\n{runtime}[[tools]]\nid='demo'\nbinary='demo'\n[tools.install]\n{spec}"),
    )
    .unwrap();
    match provider {
        Provider::Npm => {
            f.bin = f.host.home.join("runtime/bin");
            fs::create_dir_all(&f.bin).unwrap();
            f.program("brew", r#"printf M >> "$HOME/calls"; printf '{"formulae":[{"full_name":"node","installed":[{"version":"24.0.0"}]}]}'"#);
            f.program("npm", r#"exec node "$HOME/runtime/lib/node_modules/npm/bin/npm-cli.js" "$@""#);
            f.program("node", r#"shift
if [ "$1" = prefix ]; then printf P >> "$HOME/calls"; printf '%s\n' "$HOME/runtime"; exit 0; fi
if [ "$1" = ls ]; then
printf Q >> "$HOME/calls"
if [ -f "$HOME/installed" ]; then printf '{"dependencies":{"demo":{"version":"1.2.3"}}}'; else printf '{"dependencies":{}}'; fi
exit 0
fi
printf I >> "$HOME/calls"; printf installed > "$HOME/installed"; printf PRIVATE_OUTPUT
if [ -f "$HOME/fail" ]; then exit 9; fi"#);
            let cli = f.bin.parent().unwrap().join("lib/node_modules/npm/bin/npm-cli.js");
            fs::create_dir_all(cli.parent().unwrap()).unwrap();
            fs::write(cli, "TRUSTED_CLI").unwrap();
        }
        Provider::Brew => f.program("brew", r#"printf '%s\n' "$*" >> "$HOME/calls"
case "$*" in
'help install') printf '%s' '--formula --no-ask --skip-post-install HOMEBREW_NO_INSTALL_UPGRADE HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK HOMEBREW_NO_INSTALL_CLEANUP';;
'--prefix') printf '%s\n' "$HOME/prefix";;
'--repository') printf '%s\n' "$HOME/prefix/Homebrew";;
'--cellar') printf '%s\n' "$HOME/prefix/Cellar";;
'info --json=v2 --formula homebrew/core/demo') printf '{"formulae":[{"name":"demo","full_name":"demo","tap":"homebrew/core","disabled":false,"versions":{"stable":"2.7"},"installed":['; if [ -f "$HOME/installed" ]; then printf '{"version":"2.7"}'; fi; printf ']}]}';;
'info --json=v2 --installed --formula') if [ -f "$HOME/installed" ]; then printf '{"formulae":[{"full_name":"demo","installed":[{"version":"2.7"}]}]}'; else printf '{"formulae":[]}'; fi;;
'install --formula --no-ask --skip-post-install homebrew/core/demo') printf installed > "$HOME/installed"; printf PRIVATE_OUTPUT; if [ -f "$HOME/fail" ]; then exit 9; fi;;
*) exit 99;;
esac"#),
    }
    f
}
pub(super) fn checked(f: &Fixture) -> declared_tools::Checked {
    let mut input = f.input();
    if fs::read_to_string(f.repo.join("dort.toml"))
        .unwrap()
        .contains("provider='npm'")
    {
        input.detection = input
            .detection
            .with_settings(
                &f.host,
                &input.manifest,
                &[],
                &[],
                &[format!("demo={}", f.host.home.join("globals").display())],
            )
            .unwrap();
    }
    input
        .prepare(&f.host)
        .unwrap()
        .approve()
        .check(&f.host)
        .unwrap()
}
