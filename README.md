# Dort

**Dort** is a standalone, Git-native dotfile manager that lets users discover, adopt, edit, version, deploy, and synchronise configuration files through an intuitive terminal interface inspired by lazygit.

Dort uses Git as its version history and synchronisation engine. It manages destination mappings, machine profiles, templates, and encrypted secrets through a lightweight manifest.

## Guiding principles

1. Ordinary dotfiles should be directly editable through symlinks.
2. Every destructive filesystem operation must be previewed and recoverable.
3. Fetching or previewing remote changes must never modify live configurations.
4. Secrets must never enter Git in plaintext.
5. The CLI and TUI must share the same underlying engine.
6. No background daemon, database, or automatic remote deployment is required.

