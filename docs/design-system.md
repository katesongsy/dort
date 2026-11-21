# TUI design guide

Dort uses one workbench. Shared visual choices live in `crates/dort/src/tui/components/`; workflow controllers stay outside.

## Principles

- Destinations are places. Check, install, approve and retry are actions.
- One clear title, relevant scope and one foreground action strip.
- A list Enter opens details. A named action prepares its own review.
- Show exact effects before approval. Keep approvals single-use and tied to the shown content.
- Prefer task words over internal terms. Name the object, problem and next action.
- Keep compact and wide layouts usable from the keyboard. Keep mouse targets on the same actions.
- Preserve drafts and previews across menus. Discard approval on real navigation.
- Report honest progress. No fake percentages or automatic rollback.
- Escapes metadata. Trusted decoration stays separate.

## Shared components

Reuse the shared tables, forms, notices, readers, choices, action strips and navigation. Do not add local colors, glyph meanings or animation clocks. Reuse existing semantic roles.

## Checks

Run:

```sh
python3 scripts/check-tui-style.py
python3 scripts/check-tui-style.py --self-test
python3 scripts/test-terminal.py
```

Export actual components for visual review:

```sh
python3 scripts/render-tui-gallery.py --output temp/tui-gallery-review
```

Automated checks do not replace human review. Inspect compact, baseline and wide layouts, ASCII/monochrome, focus and error states.
