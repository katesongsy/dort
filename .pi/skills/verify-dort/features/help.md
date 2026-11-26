# Help

## Sub-features

- Page guides explain what you see and where to start.
- Form guides explain fields and what happens when you continue.
- Help over a review explains how to read the effects before confirming.

## How to get to it (user POV)

Press `F1` on a page, in a form, or during a review. Press `F1` again or `Esc` to return. `?` also opens Help when you are outside a text field.

## Driving it with the PTY harness

```sh
python3 .pi/skills/verify-dort/drive.py tui <name> --size 14 48 \
  'wait:loaded.' "send:'?'" 'wait:Start with a repository' \
  "send:'\x1b[6~'*30" 'wait:Use a repository you already have' \
  "send:'\x1b[H'" 'wait:Start with a repository' \
  "send:'\x1bOP'" 'wait:F1 Help'
```

`scripts/test-terminal.py` also checks Help over an adoption form, a write review, and a profile form. Check that closing Help restores the name and default in the profile draft. Over a review, send `y` with Help open and verify the reviewed change did not happen.

## Gotchas

- VT100 PF1 sends `\x1bOP`. Some terminals send `\x1b[11~`. The profile case tests both.
- Opening Help over a review requires reading the effects again before confirming.
