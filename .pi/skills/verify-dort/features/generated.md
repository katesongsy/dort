# Generated configuration

Preview how a repository template will render for this computer.

## Sub-features

- Create and edit inline recipes from Files → Managed.
- Substitute named values from `dort.toml`.
- Check whether the destination exists without opening it.
- Preview without creating or changing files.

## How to get to it (user POV)

In **Files → Managed**, choose **New generated file** (`s`). Enter an ID, destination, and recipe. Choose **Continue**, fill in the named inputs, then review and save. Select a saved template and choose **Preview generated file** (`v`). For an inline recipe, choose **Edit generated file** (`e`).

## Driving it with drive.py

Run these commands from the checkout. They use a disposable HOME.

```sh
set -e
drive="$PWD/.pi/skills/verify-dort/drive.py"
python3 "$drive" build
python3 "$drive" new generated-preview
(
  eval "$(python3 "$drive" env generated-preview)"
  mkdir -p "$DORT_REPO/templates"
  printf 'font-size = {{ size }}\n' > "$DORT_REPO/templates/editor.conf"
  cat >> "$DORT_REPO/dort.toml" <<'TOML'

[[files]]
id = "editor"
source = "templates/editor.conf"
target = "~/.editor.conf"
mode = "template"
inputs = { size = "18" }
TOML
)
python3 "$drive" tui generated-preview --label preview \
  "wait:loaded." "send:'f'" "wait:Preview generated file" \
  "click:Preview generated file" "wait:font-size = 18" \
  "wait:Destination status: Missing" "snap:output"
python3 "$drive" cli generated-preview --label cli-preview -- \
  --json --repo @REPO files preview-generated editor
(
  eval "$(python3 "$drive" env generated-preview)"
  test ! -e "$HOME/.editor.conf" && test ! -L "$HOME/.editor.conf"
  printf 'font-size = 27\n' > "$HOME/.editor.conf"
)
python3 "$drive" tui generated-preview --label existing \
  "wait:loaded." "send:'f'" "wait:Preview generated file" \
  "send:'v'" "wait:font-size = 18" \
  "wait:Destination status: Existing file" "snap:existing" \
  "send:'\x1b'" "wait:Managed"
(
  eval "$(python3 "$drive" env generated-preview)"
  printf 'font-size = 27\n' | cmp - "$HOME/.editor.conf"
  printf 'font-size = {{ missing }}\n' > "$DORT_REPO/templates/editor.conf"
)
python3 "$drive" tui generated-preview --label missing-input \
  "wait:loaded." "send:'f'" "wait:Preview generated file" \
  "send:'\r'" "wait:Missing input missing" "snap:missing-input"
python3 "$drive" cleanup generated-preview
```

The CLI output should contain the same rendered line. The PTY driver checks that Dort restores the terminal when it exits. If a step fails, run `python3 "$drive" cleanup generated-preview` separately to remove the disposable home and keep the evidence.

## Create and edit a recipe

Start with a blank repository and an existing local destination. Run from the checkout.

```sh
set -e
drive="$PWD/.pi/skills/verify-dort/drive.py"
python3 "$drive" build
python3 "$drive" new generated-setup-proof
(
  eval "$(python3 "$drive" env generated-setup-proof)"
  cp "$DORT_REPO/dort.toml" "$DORT_EVIDENCE/before.toml"
  printf 'local settings\n' > "$HOME/.editor.conf"
  cp "$HOME/.editor.conf" "$DORT_EVIDENCE/home-before.txt"
)
python3 "$drive" tui generated-setup-proof --label cancel \
  'wait:loaded.' "send:'f'" 'wait:New generated file' 'click:New generated file' \
  'wait:Recipe line 1' "send:'editor\t~/.editor.conf\tfont-size = {{ size }}'" \
  'wait:font-size' "send:'\r'" 'wait:Input: size' "send:'18\r'" \
  'wait:font-size = 18' 'snap:review' "send:'\x1b'" 'wait:Input: size' \
  "send:'\x1b'" 'wait:Recipe line 1' "send:'\x1b'" 'wait:No mappings'
(
  eval "$(python3 "$drive" env generated-setup-proof)"
  cmp "$DORT_EVIDENCE/before.toml" "$DORT_REPO/dort.toml"
  cmp "$DORT_EVIDENCE/home-before.txt" "$HOME/.editor.conf"
)
python3 "$drive" tui generated-setup-proof --label save \
  'wait:loaded.' "send:'f'" 'wait:New generated file' "send:'s'" \
  'wait:Recipe line 1' "send:'editor\t~/.editor.conf\tfont-size = {{ size }}'" \
  'wait:font-size' "send:'\x1b[200~\ncolor = dark\n\x1b[201~'" 'wait:Recipe line 3' \
  "send:'\r'" 'wait:Input: size' "send:'18\r'" 'wait:font-size = 18' \
  'wait:color = dark' 'wait:y Save generated file' 'snap:review' "send:'y'" \
  'wait:Manifest change completed.' 'snap:saved' "send:'\r'" 'wait:Managed'
python3 "$drive" tui generated-setup-proof --label reopen \
  'wait:loaded.' "send:'f'" 'wait:Preview generated file' "send:'v'" \
  'wait:font-size = 18' 'wait:color = dark' 'snap:preview' "send:'\x1b'" 'wait:Managed'
python3 "$drive" tui generated-setup-proof --label edit \
  'wait:loaded.' "send:'f'" 'wait:Edit generated file' "send:'e'" 'wait:Recipe line 1' \
  "send:'\r'" 'wait:Input: size' "send:'\x7f\x7f22\r'" 'wait:font-size = 22' \
  'wait:y Save generated file' "send:'y'" 'wait:Manifest change completed.' \
  'snap:saved' "send:'\r'" 'wait:Managed'
python3 "$drive" cli generated-setup-proof --label cli -- \
  --json --repo @REPO files preview-generated editor
(
  eval "$(python3 "$drive" env generated-setup-proof)"
  cmp "$DORT_EVIDENCE/home-before.txt" "$HOME/.editor.conf"
  python3 - "$DORT_REPO/dort.toml" <<'PY'
import sys, tomllib
with open(sys.argv[1], 'rb') as manifest:
    [file] = tomllib.load(manifest)['files']
assert file['id'] == 'editor'
assert file['target'] == '~/.editor.conf'
assert file['recipe'] == 'font-size = {{ size }}\ncolor = dark\n'
assert file['inputs'] == {'size': '22'}
PY
  cp "$DORT_REPO/dort.toml" "$DORT_EVIDENCE/after-edit.toml"
)
python3 "$drive" cleanup generated-setup-proof
```

## Gotchas

- Exit the draft with Esc before the harness sends `q`. A focused field treats `q` as text.
- Save updates the manifest, not the destination. File-backed recipes can be previewed in the TUI; edit their source separately.
- Use one mapping in the fixture so Managed starts on the template.
