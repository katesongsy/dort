# Generated configuration

Preview how a repository template will render for this computer.

## Sub-features

- Substitute named values from `dort.toml`.
- Check whether the destination exists without opening it.
- Preview without creating or changing files.

## How to get to it (user POV)

Open **Files → Managed**, select a template, and choose **Preview generated file**. Read the inputs, output, and destination status. Return to Managed when done.

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

## Gotchas

- The recipe and its manifest entry must already exist. Preview does not deploy a file.
- Use one mapping in the fixture so Managed starts on the template.
