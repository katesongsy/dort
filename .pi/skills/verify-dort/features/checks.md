# Checks

Validate the manifest's structure, and see host and local state paths. Checks are structural only.

## Sub-features

- **Manifest checks** (`1`): validates declarations and profiles for a repository. Files and Tools columns show live state separately.
- **Diagnostics** (`2`): host and local state paths.
- **Standalone check** (`v` / `dort manifest check --manifest PATH`): a manifest with no repository and no Git.
- **`dort doctor`**: host and state paths only. Does not check packages and does not repair.

## How to get to it (user POV)

TUI: `c`. Footer shows `Enter Check repository`, `v Standalone check`.

CLI:

```sh
dort --repo "$DORT_REPO" manifest check
dort --repo "$DORT_REPO" manifest check --profile dev
dort manifest check --manifest /path/to/dort.toml
dort --json doctor
```

## Driving it with harness

```sh
python3 .pi/skills/verify-dort/drive.py cli my-proof --label manifest-check -- \
  --repo @REPO manifest check
python3 .pi/skills/verify-dort/drive.py tui my-proof --label checks \
  "wait:loaded." "send:'c'" "wait:Manifest checks" "snap:checks"
```

Break the manifest to prove the check bites — that is the half of the journey a passing run cannot show:

```sh
eval "$(python3 /abs/path/to/repo/.pi/skills/verify-dort/drive.py env my-proof)"
cp "$DORT_REPO/dort.toml" "$DORT_EVIDENCE/dort.toml.good"
printf 'schema=1\n[[tools]]\nid="x"\n' >> "$DORT_REPO/dort.toml"   # missing binary
dort --repo "$DORT_REPO" manifest check; echo "exit=$?"
cp "$DORT_EVIDENCE/dort.toml.good" "$DORT_REPO/dort.toml"
```

## Observable end state

- **Valid:** `Valid manifest: <path>` plus `Profiles:`, `Selected: N tools, M files`, exit `0`.
- **Invalid:** a named problem and a nonzero exit. The message names the declaration, not just a line number.
- **Either way:** `Read-only structural check; live files and installed tools were not inspected.` A check that claims files are fine is over-reading it.

## Gotchas

- **`--manifest` and `--repo` do not combine.** Use one.
- **Profiles come from the repository**, so `--repo ... manifest check --profile dev` needs `dev` defined; a standalone check only sees its own file.
- **A passing check proves structure, not usability.** It reads no file sources and runs no package managers.
- **`dort doctor` is not a health check of your setup.** Paths only.
- **The TUI keeps the last result.** After editing the manifest, `R` refreshes; the old result stays on screen otherwise.
