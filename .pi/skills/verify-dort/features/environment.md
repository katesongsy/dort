# Profiles, clone and environment

## Sub-features

- Save a repository profile choice and reopen Dort. A temporary addition applies once.
- Clone and register into a new folder without applying its files or tools.
- Review one ordered environment plan. Check and resume an uncertain install without repeating it.

## How to get to it (user POV)

Use `F2` **More** for **Saved profiles**, **Clone repository**, **Review environment plan**, **Environment check**, and **Review environment resume**. Select the named action and press Enter. Scroll to the end of effects before approving.

## Driving it with the harness

Run the CLI and TUI cases with an isolated test directory for each process:

```sh
python3 scripts/test.py full
```

The CLI cases are `saved_profile_off_keeps_installed_link_and_temporary_flag_wins_once`, `clone_review_and_invalid_manifest_keep_home_targets_untouched`, `environment_plan_stops_on_uncertain_result_and_resumes_without_repeating_installs`, and `environment_process_interruption_leaves_durable_intent_and_never_replays`. The TUI cases are `saved_profiles_requires_review_then_reopens_from_local_configuration`, `clone_form_review_registers_without_creating_home_link`, and `environment_review_and_resume_check_tools_before_touching_editor`. They use the production input and approval handlers, a real local Git source, and a disposable Homebrew fixture. The environment cases read the actual link and verify that each package installs once.

## Gotchas

- **Plan** and `--dry-run` must leave the package-manager log unchanged.
- A local clone source must use an absolute path. Check that an invalid clone stays on disk but has no registration.
- After a failed or interrupted install, Resume must remain blocked until a named check confirms the package and attributed command. Use a fixture provider. Never interrupt a real package install to verify this path.
