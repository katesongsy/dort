# Profiles, clone and environment

## Sub-features

- Save a repository profile choice and reopen Dort. A temporary addition applies once.
- Clone and register into a new folder without applying its files or tools.
- Review one ordered environment plan. Check and resume an uncertain install without repeating it.
- Abandon an unfinished environment run without changing completed work.
- Repair the invalid manifest of an unregistered clone without changing its registration.

## How to get to it (user POV)

Open **Profiles**, **Clone repository**, or **Review environment** in **Overview**. A blocked environment review offers **Check tools** and reopens the plan after checking. A kept clone offers **Repair manifest** and then **Review attach**. Open **Environment run** in **Recovery** after a stopped run. Scroll to the end of each review before approving.

## Driving it with the harness

Run the CLI and TUI cases with an isolated test directory for each process:

```sh
python3 scripts/test.py full
```

The CLI cases are `saved_profile_off_keeps_installed_link_and_temporary_flag_wins_once`, `clone_review_and_invalid_manifest_keep_home_targets_untouched`, `environment_plan_stops_on_uncertain_result_and_resumes_without_repeating_installs`, and `environment_process_interruption_leaves_durable_intent_and_never_replays`. The TUI cases are `saved_profiles_requires_review_then_reopens_from_local_configuration`, `clone_form_review_registers_without_creating_home_link`, and `environment_review_and_resume_check_tools_before_touching_editor`. They use the production input and approval handlers, a real local Git source, and a disposable Homebrew fixture. The environment cases read the actual link and verify that each package installs once.

For TUI recovery, create a disposable run with `drive.py new`. Use the `support/environment.toml` and `support/environment-brew.sh` fixtures under `crates/dort/tests/` to stop a run after `a` is confirmed and `b` is uncertain. In **Recovery**, open **Environment run**, choose **Abandon run**, inspect the review, then cancel. Check that neither the run record nor provider call log changed. Open the review again and confirm. Check that the old record is archived in the repository's private `environment` directory, the completed `a` command and package remain, and no new provider command ran. Open **Review environment** and verify it requires **Check tools** before retry. The CLI interruption test `environment_abandon_archives_and_requires_fresh_check_and_install_close` also proves that a fresh check alone cannot bypass an unresolved journal.

For manifest recovery, clone a local Git source whose `dort.toml` contains `schema = 1` and a legacy `ubuntu` key. Confirm clone publishes the folder but does not register it. Choose **Repair manifest** on the clone result and inspect the `ubuntu`/`linux` diff. Verify the draft opens for the clone without asking for its path. Cancel and compare the original bytes. Approve a new review, then compare the private `before.toml` to the original and verify the repaired manifest parses. Confirm the registry is unchanged until a separate attach.

## Gotchas

- **Plan** and `--dry-run` must leave the package-manager log unchanged.
- A local clone source must use an absolute path. Check that an invalid clone stays on disk but has no registration.
- After a failed or interrupted install, Resume must remain blocked until a named check confirms the package and attributed command. Use a fixture provider. Never interrupt a real package install to verify this path.
