//! Explicit fresh-inspection closure, never installer replay or package rollback.
use super::{
    attempts::{self, ProbeScope},
    inspection, invalid, same_source,
};
use crate::{
    Result,
    context::HostContext,
    observation::{self, Entry, Kind},
    state::WriterLock,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone)]
pub struct CloseRequest {
    pub id: String,
    /// Only needed when the original intent has no trustworthy provider scope.
    pub executable: Option<PathBuf>,
    pub search_dirs: Vec<PathBuf>,
    /// Damaged-intent npm fallback; never a runtime-satisfaction claim.
    pub npm_binding: Option<inspection::NpmBinding>,
}
pub struct ClosePreview {
    id: String,
    head: String,
    original_scope_known: bool,
    scope: ProbeScope,
    executable: Entry,
    npm_cli: Option<Entry>,
    host_binding: String,
    preview: inspection::Preview,
}
pub struct CloseApproval(ClosePreview);
#[derive(Serialize)]
pub struct CloseReview<'a> {
    pub attempt_id: &'a str,
    pub original_scope_known: bool,
    pub query: &'a inspection::Description,
    pub explanation: &'static str,
}
impl ClosePreview {
    pub fn review(&self) -> CloseReview<'_> {
        CloseReview {
            attempt_id: &self.id,
            original_scope_known: self.original_scope_known,
            query: self.preview.description(),
            explanation: "Freshly inspects this provider scope and closes only the unchanged attempt metadata. It does not replay, verify readiness, repair packages or roll back earlier effects.",
        }
    }
    pub fn approve(self) -> CloseApproval {
        CloseApproval(self)
    }
}
fn executable(path: &std::path::Path) -> Result<Entry> {
    let e = observation::inspect(path, false)?;
    if e.kind != Kind::Regular
        || e.fingerprint
            .as_ref()
            .is_none_or(|f| f.mode & 0o111 == 0 || f.links != 1)
    {
        return Err(invalid(
            "closure needs a regular executable at the explicit canonical path",
        ));
    }
    Ok(e)
}
/// No provider query, directory creation, lock, cleanup or automatic repair.
pub fn prepare_close(host: &HostContext, request: CloseRequest) -> Result<ClosePreview> {
    let a = attempts::get(host, &request.id)?;
    if !a.unresolved {
        return Err(invalid("Tools attempt is already resolved"));
    }
    let known = a.scope.is_some();
    let scope = if let Some(s) = a.scope {
        if s.home != host.home
            || request
                .executable
                .as_ref()
                .is_some_and(|e| e != &s.executable)
            || (!request.search_dirs.is_empty() && request.search_dirs != s.search_dirs)
            || request
                .npm_binding
                .as_ref()
                .is_some_and(|b| s.npm.as_ref().is_none_or(|n| &n.binding != b))
        {
            return Err(invalid(
                "closure must use the displayed original provider scope",
            ));
        }
        ProbeScope {
            executable: s.executable,
            home: s.home,
            search_dirs: s.search_dirs,
            npm_binding: s.npm.map(|n| n.binding),
        }
    } else {
        ProbeScope { executable: request.executable.ok_or_else(|| invalid("damaged intent needs an explicit trusted provider executable and search directories"))?,
            home: host.home.clone(), search_dirs: request.search_dirs, npm_binding: request.npm_binding }
    };
    let preview = scope.preview()?;
    let npm_cli = scope
        .npm_binding
        .as_ref()
        .map(|b| super::npm::program(&b.npm_cli, false))
        .transpose()?;
    let executable = executable(&scope.executable)?;
    Ok(ClosePreview {
        id: request.id,
        head: a.head,
        original_scope_known: known,
        scope,
        executable,
        npm_cli,
        host_binding: crate::journal::digest(crate::journal::encode(host)?.as_bytes()),
        preview,
    })
}
fn same_cli(cli: &Option<Entry>, scope: &ProbeScope) -> Result<bool> {
    Ok(match (cli, &scope.npm_binding) {
        (Some(e), Some(b)) => same_source(e, &super::npm::program(&b.npm_cli, false)?),
        (None, None) => true,
        _ => false,
    })
}
pub fn close(
    host: &HostContext,
    lock: &WriterLock,
    approval: CloseApproval,
) -> Result<super::Attempt> {
    lock.check_context(host)?;
    let old = approval.0;
    if old.host_binding != crate::journal::digest(crate::journal::encode(host)?.as_bytes())
        || !same_cli(&old.npm_cli, &old.scope)?
        || !same_source(&old.executable, &executable(&old.scope.executable)?)
    {
        return Err(invalid(
            "Tools closure scope changed; request a fresh review",
        ));
    }
    let before = attempts::get(host, &old.id)?;
    if before.head != old.head || !before.unresolved {
        return Err(invalid(
            "Tools attempt changed; request a fresh closure review",
        ));
    }
    if !same_cli(&old.npm_cli, &old.scope)?
        || !same_source(&old.executable, &executable(&old.scope.executable)?)
    {
        return Err(invalid(
            "Tools closure executable changed before inspection",
        ));
    }
    let report = old.preview.approve().inspect();
    let inspection::Observation::Complete { installations } = report.observation else {
        return Err(invalid(
            "fresh closure inspection unavailable; Tools attempt remains unresolved",
        ));
    };
    if !same_cli(&old.npm_cli, &old.scope)?
        || !same_source(&old.executable, &executable(&old.scope.executable)?)
    {
        return Err(invalid(
            "Tools closure executable changed during inspection",
        ));
    }
    attempts::close(
        host,
        lock,
        &old.id,
        &old.head,
        old.scope,
        installations.len(),
    )
}
