# Generated configuration

Preview how a repository template will render for this computer.

## Sub-features

- Create and edit inline templates from Files → Generated (`4`).
- Substitute named values from `dort.toml`.
- Check whether the destination exists without opening it.
- Preview without creating or changing files.

## How to get to it (user POV)

In **Files → Generated (`4`)**, choose **New generated file** (`s`). Enter an ID, destination, and template. Enter adds a line in Template. Use **Open in editor** (`F5`) to edit outside Dort. Choose **Continue** (`F4`), fill in the named inputs, then review and save. Select a saved template and choose **Preview generated file** (`v`). For an inline template, choose **Edit generated file** (`e`).

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
  "wait:loaded." "send:'f'" "wait:Managed" "send:'4'" "wait:Preview generated file" \
  "click:Preview generated file" "wait:font-size = 18" \
  "wait:Status: New file" "snap:output"
python3 "$drive" cli generated-preview --label cli-preview -- \
  --json --repo @REPO files preview-generated editor
(
  eval "$(python3 "$drive" env generated-preview)"
  test ! -e "$HOME/.editor.conf" && test ! -L "$HOME/.editor.conf"
  printf 'font-size = 27\n' > "$HOME/.editor.conf"
)
python3 "$drive" tui generated-preview --label existing \
  "wait:loaded." "send:'f'" "wait:Managed" "send:'4'" "wait:Preview generated file" \
  "send:'v'" "wait:font-size = 18" \
  "wait:Unrecorded existing file" "snap:existing" \
  "send:'\x1b'" "wait:Files / Generated"
(
  eval "$(python3 "$drive" env generated-preview)"
  printf 'font-size = 27\n' | cmp - "$HOME/.editor.conf"
  printf 'font-size = {{ missing }}\n' > "$DORT_REPO/templates/editor.conf"
)
python3 "$drive" tui generated-preview --label missing-input \
  "wait:loaded." "send:'f'" "wait:Managed" "send:'4'" "wait:Preview generated file" \
  "send:'\r'" "wait:Missing input missing" "snap:missing-input"
python3 "$drive" cleanup generated-preview
```

The CLI output should contain the same rendered line. The PTY driver checks that Dort restores the terminal when it exits. If a step fails, run `python3 "$drive" cleanup generated-preview` separately to remove the disposable home and keep the evidence.

## Create and edit a template

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
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:New generated file' 'click:New generated file' \
  'wait:Open in editor' "send:'editor\t~/.editor.conf\tfont-size = {{ size }}'" \
  'wait:font-size' "send:'\x1b[14~'" 'wait:Input: size' "send:'18\r'" \
  'wait:font-size = 18' 'snap:review' "send:'\x1b'" 'wait:Input: size' \
  "send:'\x1b'" 'wait:Open in editor' "send:'\x1b'" 'wait:No mappings'
(
  eval "$(python3 "$drive" env generated-setup-proof)"
  cmp "$DORT_EVIDENCE/before.toml" "$DORT_REPO/dort.toml"
  cmp "$DORT_EVIDENCE/home-before.txt" "$HOME/.editor.conf"
)
python3 "$drive" tui generated-setup-proof --label save \
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:New generated file' "send:'s'" \
  'wait:Open in editor' "send:'editor\t~/.editor.conf\tfont-size = {{ size }}\r'" \
  'wait:font-size' "send:'\x1b[200~color = dark\n\x1b[201~'" 'wait:color = dark' \
  "send:'\x1b[14~'" 'wait:Input: size' "send:'18\r'" 'wait:font-size = 18' \
  'wait:color = dark' 'wait:y Save generated file' 'snap:review' "send:'y'" \
  'wait:Manifest change completed.' 'snap:saved' 'click:Enter Done' 'wait:Files / Generated'
python3 "$drive" tui generated-setup-proof --label reopen \
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:Preview generated file' "send:'v'" \
  'wait:font-size = 18' 'wait:color = dark' 'snap:preview' "send:'\x1b'" 'wait:Files / Generated'
python3 "$drive" tui generated-setup-proof --label edit \
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:Edit generated file' "send:'e'" 'wait:Open in editor' \
  "send:'\x1b[14~'" 'wait:Input: size' "send:'\x7f\x7f22\r'" 'wait:font-size = 22' \
  'wait:y Save generated file' "send:'y'" 'wait:Manifest change completed.' \
  'snap:saved' 'click:Enter Done' 'wait:Files / Generated'
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
assert file['template'] == 'font-size = {{ size }}\ncolor = dark\n'
assert file['inputs'] == {'size': '22'}
PY
  cp "$DORT_REPO/dort.toml" "$DORT_EVIDENCE/after-edit.toml"
)
python3 "$drive" cleanup generated-setup-proof
```

## Deploy a generated file

Use the first fixture above, with `permissions = "0640"` in the mapping. Leave `$HOME/.editor.conf` missing. Run this before the fixture creates the existing destination.

```sh
python3 "$drive" tui generated-preview --label deploy-cancel \
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:Preview generated file' "send:'v'" \
  'wait:Review deployment' "send:'R'" 'wait:Review changes' 'snap:review' \
  "send:'\x1b'" 'wait:Review deployment' "send:'\x1b'" 'wait:Files / Generated'
(
  eval "$(python3 "$drive" env generated-preview)"
  test ! -e "$HOME/.editor.conf" && test ! -L "$HOME/.editor.conf"
)
python3 "$drive" tui generated-preview --label deploy \
  'wait:loaded.' "send:'f'" "wait:Managed" "send:'4'" 'wait:Preview generated file' "send:'v'" \
  'wait:Review deployment' 'click:Review deployment' 'wait:Review changes' \
  "send:'\x1b[F'" 'wait:y Deploy generated file' 'snap:approved' "send:'y'" \
  'wait:Generated file deployed.' 'snap:deployed'
(
  eval "$(python3 "$drive" env generated-preview)"
  printf 'font-size = 18\n' | cmp - "$HOME/.editor.conf"
  python3 -c 'import os, stat, sys; assert stat.S_IMODE(os.stat(sys.argv[1]).st_mode) == 0o640' "$HOME/.editor.conf"
)
```

Reopen the preview. It should say **Up to date**. Change `size` to `24` in the manifest. Reopen the preview and check **Update available** while the local file still contains `font-size = 18`. Change the local permission mode to `0600` and check **Local permissions changed**. Restore the mode, change the local contents, and check **Local edits**. Move the local file aside and check **Local file deleted**. Run `dort files preview-generated editor` again with `--json` and check `drift_status`. **Review deployment** is absent and `dort --dry-run files deploy-generated editor` refuses to recreate the file. Confirm the file is still absent and clean up the run.

## Update a deployed file

Deploy the fixture above before changing it locally. Change `size` to `24` and `permissions` to `"0600"` in `dort.toml`. In Files → Generated, open the preview and wait for **Update available**.

```sh
python3 "$drive" tui generated-preview --label update-cancel \
  'wait:loaded.' "send:'f'" 'wait:Managed' "send:'4'" 'wait:Files / Generated' \
  "send:'v'" 'wait:Update available' "send:'R'" 'wait:Review generated update' \
  'snap:review' "send:'\x1b'" 'wait:Files / Generated'
(
  eval "$(python3 "$drive" env generated-preview)"
  printf 'font-size = 18\n' | cmp - "$HOME/.editor.conf"
  python3 -c 'import os, stat, sys; assert stat.S_IMODE(os.stat(sys.argv[1]).st_mode) == 0o640' "$HOME/.editor.conf"
)
python3 "$drive" tui generated-preview --label update \
  'wait:loaded.' "send:'f'" 'wait:Managed' "send:'4'" 'wait:Files / Generated' \
  "send:'v'" 'wait:Update available' 'click:Review update' 'wait:Review generated update' \
  "send:'\x1b[F'" 'wait:y Update generated file' 'snap:approved' \
  'click:y Update generated file' 'wait:Generated update recorded.' 'snap:updated'
(
  eval "$(python3 "$drive" env generated-preview)"
  printf 'font-size = 24\n' | cmp - "$HOME/.editor.conf"
  python3 -c 'import os, stat, sys; assert stat.S_IMODE(os.stat(sys.argv[1]).st_mode) == 0o600' "$HOME/.editor.conf"
)
```

Check the retained file named in the result still contains `font-size = 18`. Edit the destination and reopen the preview. Confirm **Local edits** appears and **Review update** is absent.

## Replace or recreate after local changes

Open the preview of a deployed file after editing its contents and permissions. Choose **Review replacement**. Confirm the review names both changes and shows the exact diff and mode. Press Esc and confirm the local file did not change. Reopen the review, press End, and choose **Replace generated file**. Confirm the result's private path contains the original bytes and mode, and the destination contains the reviewed output and mode.

Move the destination aside in the disposable HOME. Reopen the preview and choose **Review recreation**. Press End and choose **Recreate generated file**. Compare the recreated file's contents and mode with the review. Only the replacement has a retained prior file. Clean up the run.

## Gotchas

- Exit the draft with Esc before the harness sends `q`. A focused field treats `q` as text.
- Save updates the manifest, not the destination. Edit file-backed templates in their source files.
- To test **Open in editor**, pass `--visual COMMAND` to `drive.py tui`. Use a script that checks its standard streams are terminals, writes template text to the final argument, and exits. Press F5 (`\x1b[15~`). Verify the new text appears in the draft before saving and that the temporary file is gone after the editor closes.
- Use one mapping in the fixture so Generated starts on the template.
