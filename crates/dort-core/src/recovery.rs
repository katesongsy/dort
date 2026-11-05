//! Explicit read-only rollback previews and opaque, head-bound approvals.
use crate::{Error, Result, adoption, context::HostContext, journal, lifecycle, state::WriterLock};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct Preview {
    pub schema: u32,
    pub preview_only: bool,
    pub repository_id: String,
    pub transaction_id: String,
    pub journal_schema: u32,
    pub effects: Vec<PathBuf>,
    pub journal_head_sha256: String,
    #[serde(skip)]
    binding: String,
}

pub struct Approval(Preview);
impl Approval {
    pub fn from_preview(preview: Preview) -> Result<Self> {
        if journal::digest(journal::encode(&preview)?.as_bytes()) != preview.binding {
            return Err(Error::Invalid("rollback preview was modified".into()));
        }
        Ok(Self(preview))
    }
}

pub fn preview(host: &HostContext, repository_id: &str, transaction_id: &str) -> Result<Preview> {
    let diagnosis = journal::diagnose(host)?
        .into_iter()
        .find(|d| d.repository_id == repository_id && d.transaction_id == transaction_id)
        .ok_or_else(|| Error::Invalid("unknown transaction".into()))?;
    if diagnosis.problem.is_some() {
        return Err(Error::Invalid("malformed history; rollback refused".into()));
    }
    let schema = diagnosis.journal_schema.unwrap_or(0);
    let (head, effects) = match schema {
        1 => adoption::rollback_preview(host, repository_id, transaction_id)?,
        2 => lifecycle::rollback_preview(host, repository_id, transaction_id)?,
        4 => crate::batch_execution::rollback_preview(host, repository_id, transaction_id)?,
        _ => {
            return Err(Error::Invalid(
                "use bootstrap resume/abandon for initialization history".into(),
            ));
        }
    };
    let mut report = Preview {
        schema: 1,
        preview_only: true,
        repository_id: repository_id.into(),
        transaction_id: transaction_id.into(),
        journal_schema: schema,
        effects,
        journal_head_sha256: head,
        binding: String::new(),
    };
    report.binding = journal::digest(journal::encode(&report)?.as_bytes());
    Ok(report)
}

pub fn execute(host: &HostContext, lock: &WriterLock, approval: Approval) -> Result<()> {
    execute_with_observer(host, lock, approval, |_| Ok(()))
}

/// The same guarded dispatch with metadata-only progress. Observer errors retain intent.
pub fn execute_with_observer(
    host: &HostContext,
    lock: &WriterLock,
    approval: Approval,
    observer: impl FnMut(&adoption::Event) -> Result<()>,
) -> Result<()> {
    lock.check_context(host)?;
    let old = approval.0;
    let fresh = preview(host, &old.repository_id, &old.transaction_id)?;
    if fresh.binding != old.binding {
        return Err(Error::Invalid("rollback approval is stale".into()));
    }
    match fresh.journal_schema {
        1 => adoption::rollback(
            host,
            lock,
            &fresh.repository_id,
            &fresh.transaction_id,
            observer,
        ),
        2 => lifecycle::rollback(
            host,
            lock,
            &fresh.repository_id,
            &fresh.transaction_id,
            observer,
        ),
        4 => crate::batch::rollback(
            host,
            lock,
            &fresh.repository_id,
            &fresh.transaction_id,
            observer,
        ),
        _ => unreachable!(),
    }
}
