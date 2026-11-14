use crate::services::adopt_request;
use clap::{Parser, Subcommand, ValueEnum};
use dort_core::{
    Error, bootstrap,
    context::HostContext,
    git, journal,
    local::LocalConfig,
    planner::{self, Request, UnmanageMode},
    recovery,
    state::WriterLock,
};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Git-native developer environment manager (experimental guarded file lifecycle)"
)]
struct Cli {
    /// Emit structured output; diagnostics remain on stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Override the locally registered active repository.
    #[arg(long, global = true)]
    repo: Option<PathBuf>,
    /// Preview writes or provider inspection without executing or creating state.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Explicitly approve writes or provider inspection without prompting.
    #[arg(long, global = true)]
    yes: bool,
    /// Optional extra ID guard for link deletion; must match the selected mapping.
    #[arg(long, global = true)]
    confirm_remove: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the interactive local workflows, recovery, checks and Git interface.
    Tui,
    /// Create a local repository at an absent path, or attach an already-valid one.
    Init { path: PathBuf },
    /// Inspect active link state and blockers without writing.
    Status {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Preview active link deployment (equivalent to files plan apply).
    Plan {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Read-only retained history across all repositories.
    History,
    /// Explicitly preview/approve guarded rollback of a schema-1/2/4 transaction.
    Rollback {
        repository_id: String,
        transaction_id: String,
    },
    /// Show supported host and local state locations without creating them.
    Doctor,
    /// Discover, preview, adopt, apply and unmanage regular-file links.
    Files {
        #[command(subcommand)]
        command: FilesCommand,
    },
    /// Diagnose history or explicitly preview/approve guarded recovery.
    Recovery {
        #[command(subcommand)]
        command: RecoveryCommand,
    },
    /// Hardened read-only Git inspection; no network, helpers or writes.
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
    /// Inspect tools, compare declarations, or review one bounded package install.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    /// Read-only manifest diagnostics.
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
}

#[derive(Subcommand)]
enum GitCommand {
    /// Metadata-only working-tree/index status, including untracked names.
    Status,
    /// Metadata-only tracked diff by default; untracked contents are excluded.
    Diff {
        /// Compare the index to HEAD (also works before the first commit).
        #[arg(long)]
        staged: bool,
        /// Explicitly disclose bounded patch CONTENT, with unsafe bytes escaped.
        #[arg(long)]
        patch: bool,
    },
}

#[derive(Subcommand)]
enum RecoveryCommand {
    /// Explicit guarded rollback for file or tool-declaration history (schemas 1/2/4/5).
    Rollback {
        repository_id: String,
        transaction_id: String,
    },
    /// Resume an intact prepared bootstrap (schema 3), never automatically.
    Resume {
        repository_id: String,
        transaction_id: String,
    },
    /// Mark bootstrap abandoned, retaining any published repository/registration.
    Abandon {
        repository_id: String,
        transaction_id: String,
    },
    /// List all local transaction checkpoints, integrity problems and write blockers.
    List,
}

#[derive(Subcommand)]
enum ManifestCommand {
    /// Validate a manifest and resolve profiles (does not inspect installed tools).
    Check {
        /// Explicit manifest; otherwise use --repo or the active local repository.
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Add an optional profile for this check only.
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
}

#[derive(Subcommand)]
enum FilesCommand {
    /// Adopt an owned regular file into a new repository source and mapping.
    #[command(alias = "adopt")]
    Add {
        target: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        source: PathBuf,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Deploy all active ordinary link mappings in one recoverable transaction.
    Apply {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Remove a mapping, retaining its source. Keep materializes; remove deletes the link.
    #[command(alias = "unmanage")]
    Remove {
        id: String,
        #[arg(long, value_enum)]
        mode: RemovalMode,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Discover hidden files and .config metadata; never read candidate contents.
    Discover,
    /// Inspect active manifest mappings (no tool execution).
    List {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Preview only; no writes, approvals, backups or journals are performed.
    Plan {
        #[arg(long = "profile", global = true)]
        profiles: Vec<String>,
        #[command(subcommand)]
        command: PlanCommand,
    },
}

#[derive(Subcommand)]
enum PlanCommand {
    /// Preview active link mappings.
    Apply,
    /// Preview regular-file adoption as one source/manifest/destination transaction.
    Adopt {
        /// Absolute or ~/ path; quote ~ when it must be expanded by Dort.
        target: PathBuf,
        #[arg(long)]
        id: String,
        /// New repository-relative source path; must not already exist.
        #[arg(long)]
        source: PathBuf,
    },
    /// Preview removal of a mapping; repository source is always retained.
    Unmanage {
        id: String,
        /// Keep materializes an ordinary file; remove deletes the managed link.
        #[arg(long, value_enum)]
        mode: RemovalMode,
    },
}

#[derive(Subcommand)]
enum ToolsCommand {
    /// Add one desired tool declaration to a registered repository; never install packages.
    Declare {
        /// Strict Tool JSON: id, binary, profiles, requires, install (macos/ubuntu specs).
        #[arg(long)]
        declaration: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Inspect scopes and declarations; optionally install one missing supported package.
    Reconcile {
        /// Review one missing exact mise/npm package, core formula or root-risk Ubuntu package.
        #[arg(long)]
        install_tool: Option<String>,
        /// Expected physical Homebrew prefix, only with --install-tool for brew.
        #[arg(long, requires = "install_tool")]
        brew_prefix: Option<PathBuf>,
        /// Trusted physical APT candidate-query program (APT installation only).
        #[arg(long, requires = "install_tool")]
        apt_cache: Option<PathBuf>,
        /// Trusted physical privileged APT installer (APT installation only).
        #[arg(long, requires = "install_tool")]
        apt_get: Option<PathBuf>,
        /// Trusted physical sudo with policy-permitted command timeouts (APT only).
        #[arg(long, requires = "install_tool")]
        sudo: Option<PathBuf>,
        #[arg(long)]
        manifest: PathBuf,
        /// Explicit tool lookup directories, independent of provider child PATH.
        #[arg(long = "tool-search-dir", required = true)]
        tool_search_dirs: Vec<PathBuf>,
        #[arg(long = "profile")]
        profiles: Vec<String>,
        /// Strict JSON: id, provider, executable, search_dirs, optional npm binding fields.
        #[arg(long = "scope", required = true)]
        scopes: Vec<String>,
        /// Explicit selected-tool to snapshot-ID binding. Unbound tools stay unobserved.
        #[arg(long = "bind")]
        bindings: Vec<String>,
    },
    /// List private Tools installation attempts; optionally inspect one full record.
    Attempts {
        #[arg(long)]
        id: Option<String>,
    },
    /// Freshly inspect and close an unchanged unresolved attempt; never replay or roll back.
    CloseAttempt {
        id: String,
        /// Damaged-scope provider (default: mise); known attempts keep their original scope.
        #[arg(long, value_enum)]
        provider: Option<InstallationProvider>,
        #[arg(long)]
        npm_cli: Option<PathBuf>,
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Required only when the original provider scope is damaged or missing.
        #[arg(long)]
        executable: Option<PathBuf>,
        #[arg(long = "search-dir")]
        search_dirs: Vec<PathBuf>,
    },
    /// Query a trusted provider; configuration/plugins may run and caches may change.
    Inspect {
        #[arg(value_enum)]
        provider: InspectionProvider,
        /// Trusted absolute provider path (dpkg-query for apt, Node for npm).
        #[arg(long)]
        executable: PathBuf,
        /// Explicit trusted absolute child search directory; repeat for each directory.
        #[arg(long = "search-dir", required = true)]
        search_dirs: Vec<PathBuf>,
        /// Absolute manifest declaring npm's runtime tool (npm only).
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        runtime_tool: Option<String>,
        /// Trusted absolute npm CLI script, executed by the selected Node (npm only).
        #[arg(long)]
        npm_cli: Option<PathBuf>,
        /// Explicit absolute global npm prefix (npm only).
        #[arg(long)]
        prefix: Option<PathBuf>,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum InstallationProvider {
    Apt,
    Mise,
    Npm,
    Brew,
}

#[derive(Clone, Copy, ValueEnum)]
enum InspectionProvider {
    Brew,
    Apt,
    Mise,
    Npm,
}
impl InspectionProvider {
    fn query(self) -> dort_core::tools::inspection::Query {
        use dort_core::tools::inspection::Query;
        match self {
            Self::Brew => Query::BrewFormulae,
            Self::Apt => Query::DpkgPackages,
            Self::Mise => Query::MiseInstalled,
            Self::Npm => Query::NpmGlobals,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum RemovalMode {
    Keep,
    Remove,
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
    match execute(&cli) {
        Ok(code) => code,
        Err(error) => {
            let (code, kind) = match &error {
                Error::Io { .. } => (1, "io"),
                Error::Locked => (3, "locked"),
                Error::Parse(_)
                | Error::Validation(_)
                | Error::Invalid(_)
                | Error::NoRepository => (2, "validation"),
            };
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::json!({"error": error.to_string(), "kind": kind})
                );
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::from(code)
        }
    }
}

fn tools_reconcile(
    cli: &Cli,
    host: &HostContext,
    command: &ToolsCommand,
) -> dort_core::Result<ExitCode> {
    use crate::services::reconciliation::{self, ScopeRequest, Snapshot};
    use std::collections::BTreeSet;
    let ToolsCommand::Reconcile {
        install_tool,
        brew_prefix,
        apt_cache,
        apt_get,
        sudo,
        manifest,
        tool_search_dirs,
        profiles,
        scopes,
        bindings,
    } = command
    else {
        unreachable!()
    };
    if scopes.len() > reconciliation::MAX_SNAPSHOTS {
        return Err(Error::Invalid("too many inspection scopes".into()));
    }
    let effective_profiles = if install_tool.is_some() {
        crate::services::installation::profiles(host, manifest, profiles)?
    } else {
        profiles.clone()
    };
    let prepared = reconciliation::Request {
        manifest: manifest.clone(),
        tool_search_dirs: tool_search_dirs.clone(),
        profiles: effective_profiles,
    }
    .prepare(host)?;
    let mut previews = Vec::new();
    let mut ids = BTreeSet::new();
    for scope in scopes {
        let request = ScopeRequest::parse(scope)?;
        if !ids.insert(request.id.clone()) {
            return Err(Error::Invalid("duplicate snapshot ID".into()));
        }
        previews.push((request.id.clone(), request.prepare(host, manifest)?));
    }
    let bindings = reconciliation::parse_bindings(bindings)?;
    prepared.validate_bindings(&bindings, &ids)?;
    if let Some(tool) = install_tool
        && !prepared.tools.iter().any(|t| {
            &t.id == tool
                && matches!(
                    t.provider,
                    Some(
                        dort_core::model::Provider::Mise
                            | dort_core::model::Provider::Npm
                            | dort_core::model::Provider::Brew
                            | dort_core::model::Provider::Apt
                    )
                )
        })
    {
        return Err(Error::Invalid(
            "installation must select an active mise, npm, Homebrew or APT tool".into(),
        ));
    }
    if (apt_cache.is_some() || apt_get.is_some() || sudo.is_some())
        && !prepared.tools.iter().any(|t| {
            Some(&t.id) == install_tool.as_ref()
                && t.provider == Some(dort_core::model::Provider::Apt)
        })
    {
        return Err(Error::Invalid(
            "--apt-cache, --apt-get and --sudo are only for APT installation".into(),
        ));
    }
    if brew_prefix.is_some()
        && !prepared.tools.iter().any(|t| {
            Some(&t.id) == install_tool.as_ref()
                && t.provider == Some(dort_core::model::Provider::Brew)
        })
    {
        return Err(Error::Invalid(
            "--brew-prefix is only for Homebrew installation".into(),
        ));
    }
    if cli.dry_run {
        print_git_json(
            &serde_json::json!({"reconciliation": &prepared.scope, "bindings": &bindings,
            "inspections": previews.iter().map(|(id, p)| serde_json::json!({"id":id,"review":p.review()})).collect::<Vec<_>>(),
            "requested_install": install_tool, "brew_prefix": brew_prefix, "apt_cache": apt_cache, "apt_get": apt_get, "sudo": sudo, "installation_review": "Requires approved inventories first; this dry-run runs no queries or installs"}),
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    if !cli.yes && !interactive(cli) {
        return Err(Error::Invalid("reconciliation inspections require --yes in JSON/nonterminal mode, or --dry-run to review without execution".into()));
    }
    if !cli.json {
        print_git_json(
            &serde_json::json!({"reconciliation":&prepared.scope,"bindings":&bindings}),
        )?;
    }
    let mut snapshots = Vec::new();
    for (id, preview) in previews {
        if !cli.json {
            print_git_json(&serde_json::json!({"snapshot":&id,"review":preview.review()}))?;
        }
        if !cli.yes
            && !confirmed(&format!(
                "Snapshot {id}: config/plugins may run; caches may change. Run query? [y/N] "
            ))?
        {
            return Err(Error::Invalid("scope not approved; earlier approved inspections may already have run, and nothing is automatically undone".into()));
        }
        snapshots.push(Snapshot {
            id,
            report: std::sync::Arc::new(preview.approve().inspect()),
        });
        if reconciliation::snapshot_records(&snapshots) > reconciliation::MAX_SNAPSHOT_RECORDS {
            return Err(Error::Invalid(
                "session snapshot record limit exceeded; approved inspections already ran and nothing is automatically undone".into(),
            ));
        }
    }
    let report = prepared.reconcile(&snapshots, bindings)?;
    let unavailable =
        !report.binaries.available() || snapshots.iter().any(|s| !s.report.available());
    let blocked = report.plan.has_blockers();
    if let Some(tool) = install_tool {
        let mut request =
            crate::services::installation::Request::selected(&report, tool, &snapshots)?;
        if let Some(prefix) = brew_prefix {
            if request.brew_executable().is_none() {
                return Err(Error::Invalid(
                    "--brew-prefix is only for Homebrew installation".into(),
                ));
            }
            request.set_brew_prefix(prefix.clone());
        }
        if let Some(defaults) = request.apt_programs().cloned() {
            request.set_apt_programs([
                apt_cache.clone().or(defaults[0].clone()),
                apt_get.clone().or(defaults[1].clone()),
                sudo.clone().or(defaults[2].clone()),
            ]);
        }
        let preview = request.prepare(host)?;
        if !cli.json {
            print_git_json(&preview.review())?;
        }
        approve(cli)?;
        let lock = WriterLock::acquire(host)?;
        let result = dort_core::tools::install::execute(host, &lock, preview.approve(), |_| {})?;
        let success = crate::services::installation::succeeded(&result);
        print_git_json(&result)?;
        return Ok(if success {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        });
    }
    print_git_json(&serde_json::json!({"snapshots":&snapshots,"reconciliation":&report}))?;
    Ok(if unavailable {
        ExitCode::from(1)
    } else if blocked {
        ExitCode::from(4)
    } else {
        ExitCode::SUCCESS
    })
}

fn tools_inspect(
    cli: &Cli,
    host: &HostContext,
    command: &ToolsCommand,
) -> dort_core::Result<ExitCode> {
    if let ToolsCommand::Declare {
        declaration,
        profiles,
    } = command
    {
        if cli.confirm_remove.is_some() {
            return Err(Error::Invalid(
                "tool declaration is add-only; omit --confirm-remove".into(),
            ));
        }
        let local = LocalConfig::load(host)?;
        let repository = local.select_repository(cli.repo.as_deref())?;
        let profiles = if profiles.is_empty() {
            &local.active_profiles
        } else {
            profiles
        };
        let preview = crate::services::declaration::Request::parse(declaration)?.prepare(
            host,
            &repository,
            profiles,
        )?;
        if cli.dry_run {
            print_git_json(&preview)?;
        } else {
            if !cli.json {
                print_git_json(&preview)?;
            }
            approve(cli)?;
            let approval = dort_core::tools::declaration::Approval::from_preview(preview)?;
            let lock = WriterLock::acquire(host)?;
            let tx = dort_core::tools::declaration::execute(host, &lock, approval, |_| Ok(()))?;
            mutation_report("declare-tool", tx)?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    if cli.repo.is_some() || cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Tools uses its explicit scope; omit --repo/--confirm-remove".into(),
        ));
    }
    if let ToolsCommand::Attempts { id } = command {
        let attempts = dort_core::tools::install::diagnose(host)?;
        if let Some(id) = id {
            let attempt = attempts
                .iter()
                .find(|a| &a.id == id)
                .ok_or_else(|| Error::Invalid("installation attempt not found".into()))?;
            print_git_json(attempt)?;
        } else {
            print_git_json(&attempts)?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    if let ToolsCommand::CloseAttempt {
        id,
        executable,
        search_dirs,
        provider,
        npm_cli,
        prefix,
    } = command
    {
        let npm_binding = if matches!(provider, Some(InstallationProvider::Npm)) {
            Some(dort_core::tools::inspection::NpmBinding {
                runtime_tool: "historical-runtime".into(),
                node: executable.clone().ok_or_else(|| {
                    Error::Invalid("npm fallback requires --executable Node".into())
                })?,
                npm_cli: npm_cli
                    .clone()
                    .ok_or_else(|| Error::Invalid("npm fallback requires --npm-cli".into()))?,
                prefix: prefix
                    .clone()
                    .ok_or_else(|| Error::Invalid("npm fallback requires --prefix".into()))?,
            })
        } else {
            if npm_cli.is_some()
                || (prefix.is_some() && !matches!(provider, Some(InstallationProvider::Brew)))
            {
                return Err(Error::Invalid(
                    "--npm-cli needs --provider npm; --prefix needs npm or brew".into(),
                ));
            }
            None
        };
        let preview = dort_core::tools::install::prepare_close(
            host,
            dort_core::tools::install::CloseRequest {
                id: id.clone(),
                executable: executable.clone(),
                search_dirs: search_dirs.clone(),
                npm_binding,
                apt: matches!(provider, Some(InstallationProvider::Apt)),
                brew_prefix: if matches!(provider, Some(InstallationProvider::Brew)) {
                    Some(prefix.clone().ok_or_else(|| {
                        Error::Invalid("Homebrew fallback requires --prefix".into())
                    })?)
                } else {
                    None
                },
            },
        )?;
        if cli.dry_run || !cli.json {
            print_git_json(&preview.review())?;
        }
        if !cli.dry_run {
            approve(cli)?;
            let lock = WriterLock::acquire(host)?;
            print_git_json(&dort_core::tools::install::close(
                host,
                &lock,
                preview.approve(),
            )?)?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    if matches!(command, ToolsCommand::Reconcile { .. }) {
        return tools_reconcile(cli, host, command);
    }
    let ToolsCommand::Inspect {
        provider,
        executable,
        search_dirs,
        manifest,
        runtime_tool,
        npm_cli,
        prefix,
    } = command
    else {
        unreachable!()
    };
    let preview = crate::services::tools::Request {
        query: provider.query(),
        executable: executable.clone(),
        search_dirs: search_dirs.clone(),
        manifest: manifest.clone(),
        runtime_tool: runtime_tool.clone(),
        npm_cli: npm_cli.clone(),
        prefix: prefix.clone(),
    }
    .prepare(host)?;
    if cli.dry_run {
        print_git_json(&preview.review())?;
        return Ok(ExitCode::SUCCESS);
    }
    if !cli.json {
        print_git_json(&preview.review())?;
    }
    if !cli.yes {
        if !interactive(cli) {
            return Err(Error::Invalid("provider inspection requires --yes in JSON/nonterminal mode, or use --dry-run to review without execution".into()));
        }
        if !confirmed("Provider config/plugins may run; caches may change. Run query? [y/N] ")? {
            return Err(Error::Invalid(
                "provider inspection not approved; nothing executed".into(),
            ));
        }
    }
    let report = preview.approve().inspect();
    let available = report.available();
    print_git_json(&report)?;
    Ok(if available {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn execute(cli: &Cli) -> dort_core::Result<ExitCode> {
    let command = cli.command.as_ref().unwrap_or(&Command::Tui);
    if matches!(command, Command::Tui) {
        if cli.json || cli.yes || cli.confirm_remove.is_some() || !interactive(cli) {
            return Err(Error::Invalid("TUI requires terminal stdin/stdout/stderr, without --json/--yes/--confirm-remove; use --help or an explicit CLI command".into()));
        }
        crate::tui::run(
            HostContext::from_env()?,
            cli.repo.clone(),
            cli.dry_run,
            crate::environment::launch_directory()?,
            crate::environment::resolve_git().map_err(|error| error.to_string()),
            crate::environment::terminal_appearance(),
            crate::environment::reduced_motion(),
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    let host = HostContext::from_env()?;
    if let Command::Tools { command } = command {
        return tools_inspect(cli, &host, command);
    }
    // Read only; missing configuration uses defaults without creating directories.
    let local = LocalConfig::load(&host)?;
    if cli.repo.is_some()
        && matches!(
            command,
            Command::Rollback { .. }
                | Command::Recovery {
                    command: RecoveryCommand::Rollback { .. }
                        | RecoveryCommand::Resume { .. }
                        | RecoveryCommand::Abandon { .. }
                }
        )
    {
        return Err(Error::Invalid(
            "recovery uses explicit repository/transaction IDs; omit --repo".into(),
        ));
    }
    match command {
        Command::Tui | Command::Tools { .. } => unreachable!(),
        Command::Init { path } => {
            if cli.repo.is_some() {
                return Err(Error::Invalid(
                    "init uses its explicit path; omit --repo".into(),
                ));
            }
            let root = if path.is_absolute() {
                path.clone()
            } else {
                crate::services::resolve_root(path, &crate::environment::launch_directory()?)
            };
            let git = crate::environment::resolve_git()?;
            let preview = crate::services::BootstrapPreview::prepare(&host, &root, &git)?;
            let report = &preview.report;
            if cli.dry_run {
                print_git_json(&report)?;
            } else {
                if !cli.json {
                    print_git_json(&report)?;
                }
                approve(cli)?;
                let tid = preview.approve()?.execute(&host, |_| Ok(()))?;
                print_git_json(
                    &serde_json::json!({"schema":1,"operation":"init","completed":true,"repository":root,"transaction_id":tid}),
                )?;
            }
        }
        Command::Status { profiles } | Command::Plan { profiles } => {
            return file_operation(cli, &host, &local, profiles, Request::Apply, false);
        }
        Command::Rollback {
            repository_id,
            transaction_id,
        }
        | Command::Recovery {
            command:
                RecoveryCommand::Rollback {
                    repository_id,
                    transaction_id,
                },
        } => {
            let report = recovery::preview(&host, repository_id, transaction_id)?;
            if cli.dry_run {
                print_git_json(&report)?;
            } else {
                if !cli.json {
                    print_git_json(&report)?;
                }
                approve(cli)?;
                let approval = recovery::Approval::from_preview(report)?;
                let lock = WriterLock::acquire(&host)?;
                recovery::execute(&host, &lock, approval)?;
                mutation_report("rollback", Some(transaction_id.clone()))?;
            }
        }
        Command::Recovery {
            command:
                RecoveryCommand::Resume {
                    repository_id,
                    transaction_id,
                },
        }
        | Command::Recovery {
            command:
                RecoveryCommand::Abandon {
                    repository_id,
                    transaction_id,
                },
        } => {
            let action = if matches!(
                command,
                Command::Recovery {
                    command: RecoveryCommand::Resume { .. }
                }
            ) {
                bootstrap::RecoveryAction::Resume
            } else {
                bootstrap::RecoveryAction::Abandon
            };
            let report = bootstrap::recovery_preview(&host, repository_id, transaction_id, action)?;
            if cli.dry_run {
                print_git_json(&report)?;
            } else {
                if !cli.json {
                    print_git_json(&report)?;
                }
                approve(cli)?;
                let approval = bootstrap::RecoveryApproval::from_preview(report)?;
                let lock = WriterLock::acquire(&host)?;
                bootstrap::recover(&host, &lock, approval, |_| Ok(()))?;
                mutation_report("bootstrap_recovery", Some(transaction_id.clone()))?;
            }
        }
        Command::Doctor => {
            let report = crate::services::doctor(&host, &local);
            if cli.json {
                print_json(&report)?;
            } else {
                println!("Dort: {}", report.mode);
                println!(
                    "Platform: {:?} / {:?}",
                    report.host.platform, report.host.architecture
                );
                println!("Config: {}", report.host.config_dir.display());
                println!("State: {}", report.host.state_dir.display());
                println!(
                    "Registered repositories: {}",
                    report.registered_repositories
                );
            }
        }
        Command::History
        | Command::Recovery {
            command: RecoveryCommand::List,
        } => {
            if cli.repo.is_some() {
                return Err(Error::Invalid(
                    "recovery list inspects all local repositories; omit --repo".into(),
                ));
            }
            let transactions = journal::diagnose(&host)?;
            let write_blocked = transactions.iter().any(journal::Diagnosis::blocks_writes);
            if cli.json {
                print_json(
                    &serde_json::json!({"schema": 1, "read_only": true, "write_blocked": write_blocked, "transactions": transactions}),
                )?;
            } else {
                for transaction in &transactions {
                    println!(
                        "{} / {}: {:?} (journal schema {:?}; {} valid checkpoints)",
                        transaction.repository_id,
                        transaction.transaction_id,
                        transaction.phase,
                        transaction.journal_schema,
                        transaction.valid_checkpoints
                    );
                    if let Some(problem) = &transaction.problem {
                        println!("  BLOCKED: {problem}");
                    }
                }
                println!(
                    "Read-only diagnosis; writes blocked: {write_blocked}. Recovery requires an explicit preview and approval."
                );
            }
            if write_blocked {
                return Ok(ExitCode::from(4));
            }
        }
        Command::Git { command } => {
            let root =
                local
                    .select_repository(cli.repo.as_deref())
                    .map_err(|error| match error {
                        Error::NoRepository => Error::NoRepository,
                        _ => Error::Invalid("could not resolve Git repository root".into()),
                    })?;
            let backend = git::Git::new(crate::environment::resolve_git()?)?;
            match command {
                GitCommand::Status => {
                    let report = backend.status(&root)?;
                    if cli.json {
                        print_git_json(&report)?;
                    } else {
                        for header in &report.branch {
                            println!("Branch: {header:?}");
                        }
                        for entry in &report.entries {
                            println!(
                                "{} {} {:?} (original: {:?})",
                                entry.kind,
                                entry.xy.as_deref().unwrap_or("?"),
                                entry.path,
                                entry.original_path
                            );
                        }
                        println!("Read-only Git status; {} entries.", report.entries.len());
                    }
                }
                GitCommand::Diff { staged, patch } => {
                    let report = backend.diff(&root, *staged, *patch)?;
                    if cli.json {
                        print_git_json(&report)?;
                    } else {
                        for entry in &report.entries {
                            println!(
                                "{} {} -> {} {:?}",
                                entry.change, entry.old_mode, entry.new_mode, entry.path
                            );
                        }
                        println!(
                            "Read-only Git diff; {} entries. Untracked contents excluded.",
                            report.entries.len()
                        );
                        if let Some(patch) = &report.patch {
                            println!("Explicit patch CONTENT (unsafe bytes escaped):\n{patch}");
                        }
                    }
                }
            }
        }
        Command::Files { command } => match command {
            FilesCommand::Add {
                target,
                id,
                source,
                profiles,
            } => {
                return file_operation(
                    cli,
                    &host,
                    &local,
                    profiles,
                    adopt_request(target, id, source),
                    true,
                );
            }
            FilesCommand::Apply { profiles } => {
                return file_operation(cli, &host, &local, profiles, Request::Apply, true);
            }
            FilesCommand::Remove { id, mode, profiles } => {
                return file_operation(
                    cli,
                    &host,
                    &local,
                    profiles,
                    Request::Unmanage {
                        id: id.clone(),
                        mode: match mode {
                            RemovalMode::Keep => UnmanageMode::Keep,
                            RemovalMode::Remove => UnmanageMode::Remove,
                        },
                    },
                    true,
                );
            }
            FilesCommand::Discover => {
                let report = crate::services::discover(&host, &local, cli.repo.as_deref())?.report;
                if cli.json {
                    print_json(&report)?;
                } else {
                    for candidate in &report.candidates {
                        println!(
                            "{}: {:?}, adoption candidate: {}{}",
                            candidate.entry.path.display(),
                            candidate.entry.kind,
                            candidate.supported_for_adoption,
                            candidate
                                .link_state
                                .as_ref()
                                .map_or(String::new(), |state| format!(", link: {state:?}"))
                        );
                    }
                    for warning in &report.warnings {
                        println!("Warning: {warning}");
                    }
                    println!("{}", report.sensitivity_notice);
                }
            }
            FilesCommand::List { profiles } => {
                let snapshot = crate::services::observe(
                    &host,
                    &local,
                    cli.repo.as_deref(),
                    profiles,
                    Request::Apply,
                )?;
                if cli.json {
                    print_json(&snapshot)?;
                } else {
                    for file in &snapshot.files {
                        println!(
                            "{}: {} -> {}",
                            file.mapping.id,
                            file.target_path.display(),
                            file.source_path.display()
                        );
                        println!(
                            "  Source: {:?}; destination: {:?}",
                            file.source.as_ref().map(|f| &f.kind),
                            file.destination.as_ref().map(|f| &f.kind)
                        );
                        for blocker in &file.blockers {
                            println!("  BLOCKED: {blocker}");
                        }
                    }
                    println!(
                        "Read-only inventory; use files plan apply for safety/conflict decisions."
                    );
                }
            }
            FilesCommand::Plan { profiles, command } => {
                let request = match command {
                    PlanCommand::Apply => Request::Apply,
                    PlanCommand::Adopt { target, id, source } => adopt_request(target, id, source),
                    PlanCommand::Unmanage { id, mode } => Request::Unmanage {
                        id: id.clone(),
                        mode: match mode {
                            RemovalMode::Keep => UnmanageMode::Keep,
                            RemovalMode::Remove => UnmanageMode::Remove,
                        },
                    },
                };
                let plan = planner::plan(crate::services::observe(
                    &host,
                    &local,
                    cli.repo.as_deref(),
                    profiles,
                    request,
                )?);
                if cli.json {
                    print_json(&plan)?;
                } else {
                    print!("{}", plan.human());
                }
                if plan.has_blockers() {
                    return Ok(ExitCode::from(4));
                }
            }
        },
        Command::Manifest {
            command: ManifestCommand::Check { manifest, profiles },
        } => {
            if manifest.is_some() && cli.repo.is_some() {
                return Err(Error::Invalid(
                    "choose either --manifest or --repo, not both".into(),
                ));
            }
            let path = if let Some(path) = manifest {
                path.clone()
            } else {
                local
                    .select_repository(cli.repo.as_deref())?
                    .join("dort.toml")
            };
            let report = crate::services::check_manifest(&host, &local, path, profiles)?;
            if cli.json {
                print_json(&report)?;
            } else {
                println!("Valid manifest: {}", report.manifest.display());
                println!(
                    "Profiles: {}",
                    report
                        .active_profiles
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                println!(
                    "Selected: {} tools, {} files",
                    report.selected_tools, report.selected_files
                );
                for capability in &report.unsupported_capabilities {
                    println!("Not implemented: {capability}");
                }
                println!(
                    "Read-only structural check; live files and installed tools were not inspected."
                );
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn mutation_report(operation: &str, transaction: Option<String>) -> dort_core::Result<()> {
    print_git_json(
        &serde_json::json!({"schema":1,"operation":operation,"completed":true,"transaction_id":transaction}),
    )
}

fn interactive(cli: &Cli) -> bool {
    use std::io::IsTerminal;
    !cli.json
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}
fn answer(prompt: &str) -> dort_core::Result<String> {
    use std::io::{BufRead, Read, Write};
    eprint!("{prompt}");
    std::io::stderr()
        .flush()
        .map_err(|_| Error::Invalid("could not present confirmation".into()))?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(257)
        .read_until(b'\n', &mut bytes)
        .map_err(|_| Error::Invalid("could not read confirmation".into()))?;
    if bytes.len() > 256 {
        return Err(Error::Invalid("confirmation too long".into()));
    }
    String::from_utf8(bytes)
        .map(|s| s.trim_end_matches(['\r', '\n']).to_owned())
        .map_err(|_| Error::Invalid("invalid confirmation".into()))
}
fn confirmed(prompt: &str) -> dort_core::Result<bool> {
    Ok(matches!(
        answer(prompt)?.to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
fn approve(cli: &Cli) -> dort_core::Result<()> {
    if cli.yes {
        return Ok(());
    }
    if !interactive(cli) {
        return Err(Error::Invalid(
            "write requires --yes in JSON/nonterminal mode; use --dry-run to preview".into(),
        ));
    }
    if !confirmed("Proceed with the reviewed changes? [y/N] ")? {
        return Err(Error::Invalid("write not approved".into()));
    }
    Ok(())
}
fn file_operation(
    cli: &Cli,
    host: &HostContext,
    local: &LocalConfig,
    profiles: &[String],
    request: Request,
    write: bool,
) -> dort_core::Result<ExitCode> {
    let removal = match &request {
        Request::Unmanage {
            id,
            mode: UnmanageMode::Remove,
        } => Some(id.clone()),
        _ => None,
    };
    let plan = planner::plan(crate::services::observe(
        host,
        local,
        cli.repo.as_deref(),
        profiles,
        request,
    )?);
    if !write || cli.dry_run || plan.has_blockers() {
        if cli.json {
            print_git_json(&plan)?;
        } else {
            print!("{}", plan.human());
        }
        return Ok(ExitCode::from(if plan.has_blockers() { 4 } else { 0 }));
    }
    if !cli.json {
        print!("{}", plan.human());
    }
    // A supplied legacy guard must match. Approval binds the preview's ID internally.
    if let Some(id) = &removal {
        if cli.confirm_remove.as_ref().is_some_and(|value| value != id) {
            return Err(Error::Invalid(
                "--confirm-remove does not match the selected mapping ID".into(),
            ));
        }
    } else if cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "--confirm-remove is only valid for remove mode".into(),
        ));
    }
    approve(cli)?;
    let approval = crate::services::FileApproval::from_plan(plan, removal.as_deref())?;
    let tid = approval.execute(host, |_| Ok(()))?;
    mutation_report("files", tid)?;
    Ok(ExitCode::SUCCESS)
}

// ASCII-only JSON protects terminals from C1/bidi controls while preserving the
// exact decoded Unicode path identity, including supplementary-plane characters.
fn print_git_json(value: &impl Serialize) -> dort_core::Result<()> {
    use std::fmt::Write;
    let json = serde_json::to_string_pretty(value)
        .map_err(|_| Error::Invalid("could not encode Git output".into()))?;
    let mut escaped = String::new();
    for character in json.chars() {
        if character >= '\u{7f}' {
            for unit in character.encode_utf16(&mut [0; 2]) {
                let _ = write!(escaped, "\\u{unit:04x}");
            }
        } else {
            escaped.push(character);
        }
    }
    println!("{escaped}");
    Ok(())
}

fn print_json(value: &impl Serialize) -> dort_core::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|_| Error::Invalid("could not encode output".into()))?;
    println!("{json}");
    Ok(())
}
