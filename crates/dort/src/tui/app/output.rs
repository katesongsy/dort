//! Result handling for one service result, grouped by feature.
//!
//! `App::complete` owns the shared bookkeeping and the failure policy. Each
//! handler owns the outcomes of its own feature.
use super::*;
use crate::tui::actions::{Completion, Data, Diagnostics, Pending, Task};
use dort_core::journal;
use std::path::PathBuf;

impl App {
    pub(super) fn accept_tools_installed_preview(
        &mut self,
        preview: crate::services::installed_tools::Preview,
    ) -> Option<Task> {
        if self.tab == Tab::Tools
            && self.tools_view.installed_open
            && self.tools_view.installed.waiting
            && !self.quit
        {
            self.tools_view.installed.waiting = false;
            self.tools_view.installed.preview = Some(preview);
        }

        None
    }

    pub(super) fn accept_tools_installed(
        &mut self,
        current: crate::services::installed_tools::Current,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.installed_open && !self.quit {
            self.tools_view.installed.completed(current);
        }

        None
    }

    pub(super) fn accept_tools_check_preview(
        &mut self,
        preview: crate::services::declared_tools::CheckPreview,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.declared.check_waiting {
            self.tools_view.declared.check_waiting = false;
            self.tools_view.declared.check_preview = Some(preview);
            self.details_scroll = 0;
        }

        None
    }

    pub(super) fn accept_tools_checked(
        &mut self,
        checked: crate::services::declared_tools::Checked,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.landing() {
            self.tools_view.declared.checked = Some(std::sync::Arc::new(checked));
            self.tools_view.declared.installed = None;
            self.tools_view.declared.stale = false;
            self.status =
                "Declared tools checked. Select a tool for details, or Install missing.".into();
        }

        None
    }

    pub(super) fn accept_tools_missing_preview(
        &mut self,
        preview: crate::services::declared_tools::InstallPreview,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.declared.install_waiting {
            self.tools_view.declared.install_waiting = false;
            self.tools_view.declared.install_preview = Some(preview);
            self.details_scroll = 0;
        }

        None
    }

    pub(super) fn accept_tools_missing_installed(
        &mut self,
        installed: crate::services::declared_tools::Installed,
    ) -> Option<Task> {
        self.write_warning |= !installed.succeeded();
        self.tools_view.declared.installed = Some(installed);
        self.tools_view.declared.stale = true;
        self.tools_view.declared.result_open = self.tab == Tab::Tools;
        self.details_scroll = 0;

        None
    }

    pub(super) fn accept_history(
        &mut self,
        rows: Vec<journal::Diagnosis>,
        error: Option<String>,
    ) -> Option<Task> {
        let identity = self
            .data
            .as_ref()
            .and_then(|d| d.history.get(self.selected))
            .map(|r| (r.repository_id.clone(), r.transaction_id.clone()));
        let data = self.data.get_or_insert_with(Default::default);
        data.history = rows;
        data.history_error = error;
        let position = identity.as_ref().and_then(|(r, t)| {
            data.history
                .iter()
                .position(|row| &row.repository_id == r && &row.transaction_id == t)
        });
        if data.history_error.is_some() {
            self.status = "Recovery unavailable. Write status unknown. r retries.".into();
        } else if identity.is_some() && position.is_none() {
            self.status = "The selected operation is gone. Showing the nearest row.".into();
        } else {
            self.status = "Recovery refreshed. Each preview checks current files.".into();
        }
        self.selected = position
            .unwrap_or(self.selected)
            .min(data.history.len().saturating_sub(1));
        self.clamp_history_selection();

        None
    }

    pub(super) fn accept_diagnostics(&mut self, report: Diagnostics) -> Option<Task> {
        self.diagnostics = Some(report);
        self.showing_diagnostics = true;
        self.tab = Tab::Checks;
        self.shell.overlay = None;
        self.details_scroll = 0;
        self.status = "Diagnostics loaded. Unavailable items show why.".into();

        None
    }

    pub(super) fn accept_tool_attempts(
        &mut self,
        result: std::result::Result<Vec<dort_core::tools::install::Attempt>, String>,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.install_open && !self.quit {
            self.tools_view.installation.attempts_ready(result);
            self.status = "Install records · read-only. An unresolved record needs a fresh check before it can close.".into();
        }

        None
    }

    pub(super) fn accept_tool_close_preview(
        &mut self,
        preview: Box<dort_core::tools::install::ClosePreview>,
    ) -> Option<Task> {
        if self.tab == Tab::Tools
            && self.tools_view.install_open
            && !self.quit
            && self.tools_view.installation.mode == crate::tui::tools_install::Mode::LoadingClose
        {
            self.tools_view.installation.close_reviewed(*preview);
            self.status = "Review the fresh check and record close. Approval applies once.".into();
        }

        None
    }

    pub(super) fn accept_tool_closed(
        &mut self,
        result: std::result::Result<dort_core::tools::install::Attempt, String>,
    ) -> Option<Task> {
        self.tools_view.installation.closed(result);
        self.status =
            "Record checked. Closing does not retry, repair or roll back an install.".into();

        None
    }

    pub(super) fn accept_tools_settings_detected(
        &mut self,
        tool: String,
        request: crate::services::tools::Request,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && !self.quit {
            self.tools_view.settings_ready(tool, request);
        }

        None
    }

    pub(super) fn accept_tools_settings_saved(
        &mut self,
        request: crate::services::tools::Request,
    ) -> Option<Task> {
        if self.tab == Tab::Tools && self.tools_view.settings_open && !self.quit {
            self.tools_view.settings_saved(request);
            self.status =
                "Tool settings saved for this session. Check installed to refresh status.".into();
        }

        None
    }

    pub(super) fn accept_git(&mut self, report: crate::tui::git::Metadata) -> Option<Task> {
        self.git_view.accept(report);
        self.sync_git_preview();
        self.status = "Git summary read.".into();
        None
    }
    pub(super) fn accept_data(&mut self, data: Data) -> Option<Task> {
        if self
            .tools_view
            .declared
            .checked
            .as_ref()
            .is_some_and(|c| data.tools_source.as_ref() != Some(&c.source))
        {
            self.tools_view.declared.invalidate();
        }
        self.replace_data(data);
        // Adoption must refresh managed/history observations as well as
        // Discover, so immediately opening Recovery sees the new receipt.
        if self.rediscovering_after_adoption && !self.quit {
            return Some(Task::Discover);
        }
        // Preserve execution errors/completion until the next explicit action.
        if self.status == "Reading..." {
            self.status = "Read-only data loaded.".into();
        } else if self.status.starts_with("Completed.") {
            self.status = "Completed. Refreshed.".into();
        }

        None
    }

    pub(super) fn accept_checked(
        &mut self,
        report: Box<crate::services::CheckReport>,
        standalone: bool,
    ) -> Option<Task> {
        self.check = Some((report, standalone));
        self.showing_diagnostics = false;
        self.tab = Tab::Checks;
        self.shell.overlay = None;
        self.details_scroll = 0;
        self.status =
                "Declarations validated; inspect Files for live state and verify installed tools separately.".into();

        None
    }

    pub(super) fn accept_profiles(
        &mut self,
        report: Box<crate::services::CheckReport>,
        requested: Vec<String>,
        standalone: bool,
        apply: bool,
    ) -> Option<Task> {
        self.preview = None;
        if apply {
            if standalone {
                self.shell.check_profiles = requested;
                self.check = Some((report, true));
                self.shell.overlay = Some(Overlay::Form(Form::Check));
            } else {
                if self.requested_profiles != requested {
                    self.tools_view.declared.invalidate();
                }
                self.requested_profiles = requested;
                self.shell.overlay = None;
                if self
                    .check
                    .as_ref()
                    .is_some_and(|(_, standalone)| !standalone)
                {
                    self.check = None;
                }
                self.status = "Temporary profile additions changed; refreshing full active scope. Configuration unchanged.".into();
                if !self.quit {
                    return Some(Task::Refresh);
                }
            }
            self.status = "Standalone profile additions validated; configuration unchanged.".into();
        } else {
            self.shell.scroll = 0;
            self.shell.overlay = Some(Overlay::Profiles(Box::new(ProfilePicker {
                report,
                requested,
                standalone,
                selected: 0,
            })));
        }

        None
    }

    pub(super) fn accept_discovery(
        &mut self,
        discovery: crate::services::Discovery,
    ) -> Option<Task> {
        let old_path = self
            .discovery
            .as_ref()
            .and_then(|d| d.report.candidates.get(self.candidate_selected))
            .map(|c| &c.entry.path);
        let position = old_path.and_then(|path| {
            discovery
                .report
                .candidates
                .iter()
                .position(|c| &c.entry.path == path)
        });
        if old_path.is_some() && position.is_none() {
            self.status = "Selected candidate disappeared; showing nearest remaining row.".into();
        } else {
            self.status = "Discovery loaded; review each candidate before adoption.".into();
        }
        self.candidate_selected = position
            .unwrap_or(self.candidate_selected)
            .min(discovery.report.candidates.len().saturating_sub(1));
        self.discovery = Some(std::sync::Arc::new(discovery));
        self.clamp_file_selection();
        if std::mem::take(&mut self.rediscovering_after_adoption)
            && let Some(receipt) = &self.receipt
        {
            self.status = format!(
                    "{} completed; Discover refreshed. Details show the record; Recovery keeps history.",
                    receipt.operation
                ).into();
        }

        None
    }

    pub(super) fn accept_selected(
        &mut self,
        repository: Option<PathBuf>,
        data: Data,
    ) -> Option<Task> {
        self.origin = if repository.is_some() {
            "session choice"
        } else {
            "registered active"
        };
        self.repository = repository;
        self.reset_repository_scope();
        self.discovery = None;
        self.candidate_selected = 0;
        self.result = None;
        self.receipt = None;
        self.show_result = false;
        self.data = None;
        self.selected = 0;
        self.details_scroll = 0;
        self.preview = None;
        self.shell.back();
        self.replace_data(data);
        self.status = "Repository selected for this session only; configuration unchanged.".into();

        None
    }

    pub(super) fn accept_ignored_paths(&mut self, paths: Vec<PathBuf>) -> Option<Task> {
        self.shell.overlay = Some(Overlay::Ignored { paths, selected: 0 });
        self.shell.ignored_viewport = Default::default();
        self.shell.scroll = 0;
        self.status =
            "Select an exact ignored path to review Unignore. Ancestor rules still apply.".into();

        None
    }

    pub(super) fn accept_preview(&mut self, pending: Pending) -> Option<Task> {
        match pending.report() {
            Ok(text) => {
                self.reviewed_activity =
                    Some(crate::tui::task_surfaces::Activity::for_pending(&pending));
                self.preview = Some(Preview {
                    pending,
                    text,
                    scroll: 0,
                    horizontal: 0,
                    full: false,
                    selected: 0,
                    effect_scroll: 0,
                    report_window: Default::default(),
                });
                self.status = if self.preview.as_ref().is_some_and(|p| p.pending.adoption()) {
                    "Review all mappings; Adopt all approves this exact set once."
                } else {
                    "Review effects before confirming this exact change."
                }
                .into();
            }
            Err(error) => self.status = crate::tui::actions::diagnostic(&error).into(),
        }
        None
    }

    pub(super) fn accept_completed(
        &mut self,
        completion: Completion,
        activity: Option<crate::tui::task_surfaces::Activity>,
    ) -> Option<Task> {
        let returned_to_installed = if completion.operation == "Tool declaration" {
            self.form_focus = None;
            self.tools_view.declaration_finished()
        } else {
            false
        };
        if completion.operation == "Discovery preferences" {
            self.content = Default::default();
            self.discovery = None;
            self.inventory.candidate_marks.clear();
            self.inventory.candidate_mark_revision =
                self.inventory.candidate_mark_revision.wrapping_add(1);
            self.shell.back();
            self.tab = Tab::Files;
            self.discovering = true;
            self.inventory.details = false;
            self.status = "Discovery preferences saved. Refreshing discovery; undo in Actions > Manage ignored paths.".into();
            if !self.quit {
                return Some(Task::Discover);
            }
            return None;
        }
        self.receipt = Some(crate::tui::task_surfaces::Receipt {
            operation: completion.operation,
            transaction_id: completion.transaction_id.clone(),
            activity: activity.or_else(|| self.reviewed_activity.clone()),
        });
        self.show_result = !returned_to_installed;
        self.details_scroll = 0;
        if let Some(editor) = &mut self.batch_editor {
            editor.open = false;
        }
        let adopted = std::mem::take(&mut self.return_to_discover);
        if !adopted {
            self.discovery = None;
        }
        if let Some(root) = completion.repository {
            self.repository = Some(root);
            self.reset_repository_scope();
            self.origin = "approved create/attach";
            self.tab = if self.shell.return_to_adopt {
                Tab::Files
            } else {
                Tab::Overview
            };
            self.data = None;
            self.selected = 0;
            self.details_scroll = 0;
        }
        self.shell.back();
        self.result = Some(format!(
            "{} completed.\n{}",
            completion.operation,
            completion
                .transaction_id
                .map(|id| format!("Operation ID: {id}"))
                .unwrap_or_else(|| "Everything is up to date.".into())
        ));
        self.status = "Completed. Refreshing...".into();
        if adopted {
            self.rediscovering_after_adoption = true;
            self.tab = Tab::Files;
            self.discovering = true;
            self.show_result = false;
            self.inventory.details = false;
            self.inventory.candidate_marks.clear();
            self.inventory.candidate_mark_revision =
                self.inventory.candidate_mark_revision.wrapping_add(1);
        }
        if !self.quit {
            return Some(Task::Refresh);
        }

        None
    }
}
