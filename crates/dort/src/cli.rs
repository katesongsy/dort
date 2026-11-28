use crate::services::{adopt_request, generated_update, presentation};
use clap::{Parser, Subcommand, ValueEnum};
use dort_core::{
    Error, bootstrap,
    context::HostContext,
    generated::{UpdateAction, UpdateOutcome, pending_update, recover_reviewed_update},
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
    about = "Git-native development environment manager for local files, tools and Git"
)]
struct Cli {
    /// Emit structured output; diagnostics remain on stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Override the locally registered active repository.
    #[arg(long, global = true)]
    repo: Option<PathBuf>,
    /// Preview writes or package-manager queries without changes.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Approve writes or package-manager queries without prompting.
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
    /// Open the interactive workbench.
    Tui,
    /// Create a local repository at a new path, or attach an existing one.
    Init { path: PathBuf },
    /// Clone into a new folder and register only after the manifest passes.
    Clone { source: String, path: PathBuf },
    /// Preview, check, apply, or resume a repository environment.
    Environment {
        #[command(subcommand)]
        command: EnvironmentCommand,
    },
    /// List and save profiles for a repository.
    Profiles {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Show active file links and blockers.
    Status {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Preview linking active files.
    Plan {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Read-only operation history across repositories.
    History,
    /// Preview or approve rollback of a completed file or declaration operation.
    Rollback {
        repository_id: String,
        transaction_id: String,
    },
    /// Show host and state paths.
    Doctor,
    /// Discover and manage regular-file links.
    Files {
        #[command(subcommand)]
        command: FilesCommand,
    },
    /// Inspect operation records or recover an unfinished change.
    Recovery {
        #[command(subcommand)]
        command: RecoveryCommand,
    },
    /// Read-only Git status and diff. No network or writes.
    Git {
        #[command(subcommand)]
        command: GitCommand,
    },
    /// Manage declarations, check installed tools and install missing packages.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    /// Check a manifest.
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
}

#[derive(Subcommand)]
enum EnvironmentCommand {
    /// Read-only files and tools plan. Does not run package managers.
    Plan {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Review package-manager checks and save their results.
    Check(DeclaredToolsArgs),
    /// Review and run one dependency-ordered environment plan.
    Apply {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Review and abandon the unfinished run without undoing effects.
    Abandon,
    /// Review the last run and continue only after checking completed work.
    Resume {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// List resolved profile choices and reasons.
    List {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Save a local profile choice after review.
    Set { name: String, choice: ProfileChoice },
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileChoice {
    Defaults,
    On,
    Off,
}

#[derive(Subcommand)]
enum GitCommand {
    /// Working-tree and index status, including untracked names.
    Status,
    /// Tracked diff. Untracked files are excluded.
    Diff {
        /// Compare the index to HEAD (also works before the first commit).
        #[arg(long)]
        staged: bool,
        /// Show patch content, with unsafe bytes escaped.
        #[arg(long)]
        patch: bool,
    },
}

#[derive(Subcommand)]
enum RecoveryCommand {
    /// Preview or approve rollback of a completed file or declaration operation.
    Rollback {
        repository_id: String,
        transaction_id: String,
    },
    /// Resume an intact repository setup. Never automatic.
    Resume {
        repository_id: String,
        transaction_id: String,
    },
    /// Abandon a repository setup. Keeps any published repository and registration.
    Abandon {
        repository_id: String,
        transaction_id: String,
    },
    /// Resolve an interrupted generated file change without writing the destination.
    RecoverUpdate,
    /// List operation records and write blockers.
    List,
}

#[derive(Subcommand)]
enum ManifestCommand {
    /// Validate a manifest and profiles. Does not check installed tools.
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
    /// Adopt one owned regular file into a new source and mapping.
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
    /// Link all changed active files as one recoverable operation.
    Apply {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Remove a mapping, keeping its source. Keep writes a file; remove deletes the link.
    #[command(alias = "unmanage")]
    Remove {
        id: String,
        #[arg(long, value_enum)]
        mode: RemovalMode,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// List hidden files and .config entries. Does not read contents.
    Discover,
    /// Show active mappings without running tools.
    List {
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Show rendered output and deployment status without writing files.
    PreviewGenerated { id: String },
    /// Show the declared local inputs and values for one generated file. No writes.
    LocalInputs { id: String },
    /// Set a non-secret value on this machine. Pass an empty string to set it empty.
    SetLocalInput {
        id: String,
        name: String,
        value: String,
    },
    /// Clear a local input value without changing its declaration or deploying the file.
    ClearLocalInput { id: String, name: String },
    /// Review and create one generated file at a missing Home destination.
    DeployGenerated {
        id: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Review new contents and permissions for an unchanged deployed generated file.
    UpdateGenerated {
        id: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Replace a locally changed tracked generated file and retain the local file.
    ReplaceGenerated {
        id: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Recreate a tracked generated file that was deleted locally.
    RecreateGenerated {
        id: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Preview only. No writes, approvals, backups or history records.
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
    /// Preview adopting one regular file as a source, mapping and link.
    Adopt {
        /// Absolute or ~/ path; quote ~ when it must be expanded by Dort.
        target: PathBuf,
        #[arg(long)]
        id: String,
        /// New repository-relative source path; must not already exist.
        #[arg(long)]
        source: PathBuf,
    },
    /// Preview removing a mapping. Keeps the source.
    Unmanage {
        id: String,
        /// Keep writes a regular file; remove deletes the link.
        #[arg(long, value_enum)]
        mode: RemovalMode,
    },
}

#[derive(clap::Args)]
struct DeclaredToolsArgs {
    /// Manifest to check; defaults to the selected repository's dort.toml.
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long = "profile")]
    profiles: Vec<String>,
    /// Package-manager program for a declared tool in a nonstandard layout: TOOL=PATH.
    #[arg(long = "program", value_name = "TOOL=PATH")]
    programs: Vec<String>,
    /// npm CLI script for a declared npm tool: TOOL=PATH.
    #[arg(long = "npm-cli", value_name = "TOOL=PATH")]
    npm_clis: Vec<String>,
    /// Global package destination for a declared npm tool: TOOL=PATH.
    #[arg(long = "npm-prefix", value_name = "TOOL=PATH")]
    npm_prefixes: Vec<String>,
}

#[derive(clap::Args)]
struct InstalledToolsArgs {
    /// Optional manifest for declared Node/npm runtime settings.
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long = "program", value_name = "TOOL=PATH")]
    programs: Vec<String>,
    #[arg(long = "npm-cli", value_name = "TOOL=PATH")]
    npm_clis: Vec<String>,
    #[arg(long = "npm-prefix", value_name = "TOOL=PATH")]
    npm_prefixes: Vec<String>,
}
#[derive(Subcommand)]
enum ToolsCommand {
    /// List installed packages from detected package managers. Keeps only this result.
    Inspect(InstalledToolsArgs),
    /// Check declared tools using detected package managers.
    Check(DeclaredToolsArgs),
    /// Check declarations and install all eligible missing tools, one at a time.
    InstallMissing(DeclaredToolsArgs),
    /// Add a tool declaration to a registered repository. Does not install packages.
    Declare {
        /// Tool JSON fields: id, binary, profiles, requires, install (provider/package/version/runtime_tool).
        #[arg(long)]
        declaration: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Edit one existing tool declaration, keeping its ID and installed packages.
    Edit {
        #[arg(long)]
        declaration: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// Remove a tool declaration; leave installed packages untouched.
    Remove {
        id: String,
        #[arg(long = "profile")]
        profiles: Vec<String>,
    },
    /// List installation records; show one full record with --id.
    Attempts {
        #[arg(long)]
        id: Option<String>,
    },
    /// Check current package-manager settings and close an unchanged record. Does not retry or roll back.
    CloseAttempt {
        id: String,
        /// Package manager for a damaged record. Known records keep their original settings.
        #[arg(long, value_enum)]
        provider: Option<InstallationProvider>,
        #[arg(long)]
        npm_cli: Option<PathBuf>,
        #[arg(long)]
        prefix: Option<PathBuf>,
        /// Only needed when the original package-manager settings are damaged or missing.
        #[arg(long)]
        executable: Option<PathBuf>,
        #[arg(long = "search-dir")]
        search_dirs: Vec<PathBuf>,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum InstallationProvider {
    Npm,
    Brew,
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
                | Error::RecoveryRequired(_)
                | Error::NoRepository => (2, "validation"),
            };
            let message = match error {
                Error::Parse(_) => "Invalid TOML. Check the file syntax.".into(),
                other => other.to_string(),
            };
            if cli.json {
                eprintln!("{}", serde_json::json!({"error": message, "kind": kind}));
            } else {
                eprintln!("error: {message}");
            }
            ExitCode::from(code)
        }
    }
}

fn declared_tools(
    cli: &Cli,
    host: &HostContext,
    args: &DeclaredToolsArgs,
    install_missing: bool,
    environment: bool,
) -> dort_core::Result<ExitCode> {
    use crate::services::declared_tools::{Input, Status};
    if cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Tool checks and installation do not use --confirm-remove".into(),
        ));
    }
    if environment && args.manifest.is_some() {
        return Err(Error::Invalid(
            "Environment check uses the selected registered repository. Omit --manifest.".into(),
        ));
    }
    let launch = crate::environment::launch_directory()?;
    let manifest = match &args.manifest {
        Some(path) => crate::services::resolve_root(path, &launch),
        None => {
            let local = LocalConfig::load(host)?;
            let explicit = cli
                .repo
                .as_ref()
                .map(|p| crate::services::resolve_root(p, &launch));
            crate::services::declared_tools::manifest(
                &local.select_repository(explicit.as_deref())?,
            )
        }
    };
    if environment
        && !LocalConfig::load(host)?
            .repositories
            .values()
            .any(|r| r.root.join("dort.toml") == manifest)
    {
        return Err(Error::Invalid(
            "Environment check needs a registered repository.".into(),
        ));
    }
    let detection = crate::environment::inspection_inputs().with_settings(
        host,
        &manifest,
        &args.programs,
        &args.npm_clis,
        &args.npm_prefixes,
    )?;
    let preview = Input {
        manifest,
        profiles: args.profiles.clone(),
        detection,
    }
    .prepare(host)?;
    let preview = if environment {
        preview.for_environment()
    } else {
        preview
    };
    if cli.dry_run {
        if cli.json {
            print_git_json(
                &serde_json::json!({"check_review":preview.review(), "install_missing":install_missing, "preview_only":true}),
            )?;
        } else {
            print!(
                "{}",
                crate::services::presentation::check_review(&preview.review())
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if !cli.yes && !interactive(cli) {
        return Err(Error::Invalid("Tool checks require terminal confirmation or --yes; use --dry-run for a process-free review.".into()));
    }
    if !cli.json {
        print!(
            "{}",
            crate::services::presentation::check_review(&preview.review())
        );
    }
    if !cli.yes
        && !confirmed(
            "Check declared tools? Package-manager configuration and plugins may run. [y/N] ",
        )?
    {
        return Err(Error::Invalid("Tool checks were not approved.".into()));
    }
    let checked = preview.approve().check(host)?;
    let statuses: Vec<_> = checked.reconciliation.plan.rows.iter().map(|r|
        serde_json::json!({"tool":r.id, "status":crate::services::declared_tools::status(r)})
    ).collect();
    if !cli.json {
        for row in &checked.reconciliation.plan.rows {
            println!(
                "{}: {}",
                crate::services::presentation::inline(&row.id),
                crate::services::declared_tools::status(row).label()
            );
        }
    }
    let complete = checked
        .reconciliation
        .plan
        .rows
        .iter()
        .all(|r| crate::services::declared_tools::status(r) == Status::Installed);
    let mut exit = if !checked.available() {
        1
    } else if complete || environment {
        0
    } else {
        4
    };
    if install_missing {
        let preview = checked.prepare_missing(host)?;
        if !cli.json {
            print!(
                "{}",
                crate::services::presentation::install_review(&preview.review())
            );
        }
        if preview.is_empty() {
            if cli.json {
                print_git_json(
                    &serde_json::json!({"check":checked, "statuses":statuses, "installation_review":preview.review(), "installation":null}),
                )?;
            }
        } else {
            if !cli.yes
                && !confirmed(
                    "Install the displayed missing tools? Partial effects are possible; no rollback. [y/N] ",
                )?
            {
                return Err(Error::Invalid("Installation was not approved. Approved checks may already have changed package-manager caches.".into()));
            }
            let deferred = preview.deferred.len();
            let approval = preview.approve()?;
            let lock = WriterLock::acquire(host)?;
            let installed = approval.execute(host, &lock, |_| {});
            exit = if !installed.succeeded() {
                1
            } else if deferred > 0
                || checked.reconciliation.plan.rows.iter().any(|r| {
                    !matches!(
                        crate::services::declared_tools::status(r),
                        Status::Installed | Status::Missing
                    )
                })
            {
                4
            } else {
                0
            };
            if cli.json {
                print_git_json(
                    &serde_json::json!({"before_installation":{"check":checked, "statuses":statuses}, "installation":installed}),
                )?;
            } else {
                print!(
                    "{}",
                    crate::services::presentation::installed_result(&installed)
                );
            }
        }
    } else if cli.json {
        print_git_json(&serde_json::json!({"check":checked, "statuses":statuses}))?;
    }
    Ok(ExitCode::from(exit))
}

fn installed_tools(
    cli: &Cli,
    host: &HostContext,
    args: &InstalledToolsArgs,
) -> dort_core::Result<ExitCode> {
    use crate::services::{installed_tools, presentation};
    if cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Inspection does not use --confirm-remove.".into(),
        ));
    }
    let launch = crate::environment::launch_directory()?;
    let explicit = cli
        .repo
        .as_ref()
        .map(|p| crate::services::resolve_root(p, &launch));
    let manifest = args
        .manifest
        .as_ref()
        .map(|p| crate::services::resolve_root(p, &launch))
        .or_else(|| {
            LocalConfig::load(host)
                .ok()?
                .select_repository(explicit.as_deref())
                .ok()
                .map(|r| r.join("dort.toml"))
        });
    let mut input = crate::environment::inspection_inputs();
    if !args.programs.is_empty() || !args.npm_clis.is_empty() || !args.npm_prefixes.is_empty() {
        let manifest = manifest.as_ref().ok_or_else(|| {
            Error::Invalid("Select a repository before supplying tool settings.".into())
        })?;
        input = input.with_settings(
            host,
            manifest,
            &args.programs,
            &args.npm_clis,
            &args.npm_prefixes,
        )?;
    }
    let preview = installed_tools::prepare(host, manifest, input)?;
    if cli.dry_run {
        if cli.json {
            print_git_json(
                &serde_json::json!({"inspection_review":preview.review(), "preview_only":true}),
            )?;
        } else {
            print!("{}", presentation::installed_review(&preview.review()));
        }
        return Ok(ExitCode::SUCCESS);
    }
    if !preview.is_empty() {
        if !cli.yes && !interactive(cli) {
            return Err(Error::Invalid("Inspection requires terminal confirmation or --yes; use --dry-run for a process-free review.".into()));
        }
        if !cli.json {
            print!("{}", presentation::installed_review(&preview.review()));
        }
        if !cli.yes
            && !confirmed(
                "Inspect installed tools? Package-manager configuration/plugins may run. [y/N] ",
            )?
        {
            return Err(Error::Invalid("Inspection was not approved.".into()));
        }
    }
    let current = preview.approve().inspect()?;
    if cli.json {
        print_git_json(&current)?;
    } else {
        print!("{}", presentation::installed(&current));
    }
    Ok(ExitCode::from(if current.available() { 0 } else { 1 }))
}

fn tools_command(
    cli: &Cli,
    host: &HostContext,
    command: &ToolsCommand,
) -> dort_core::Result<ExitCode> {
    match command {
        ToolsCommand::Inspect(args) => return installed_tools(cli, host, args),
        ToolsCommand::Check(args) => return declared_tools(cli, host, args, false, false),
        ToolsCommand::InstallMissing(args) => return declared_tools(cli, host, args, true, false),
        _ => (),
    }
    if matches!(
        command,
        ToolsCommand::Declare { .. } | ToolsCommand::Edit { .. } | ToolsCommand::Remove { .. }
    ) {
        if cli.confirm_remove.is_some() {
            return Err(Error::Invalid(
                "declaration changes use ordinary approval; omit --confirm-remove".into(),
            ));
        }
        let local = LocalConfig::load(host)?;
        let repository = local.select_repository(cli.repo.as_deref())?;
        let preview = match command {
            ToolsCommand::Declare {
                declaration,
                profiles,
            }
            | ToolsCommand::Edit {
                declaration,
                profiles,
            } => {
                let mut request = crate::services::declaration::Request::parse(declaration)?;
                if matches!(command, ToolsCommand::Edit { .. }) {
                    request.change = Some(dort_core::tools::declaration::Change::Edit);
                }
                request.prepare(host, &repository, profiles)?
            }
            ToolsCommand::Remove { id, profiles } => {
                crate::services::declaration::prepare_remove(host, &repository, profiles, id)?
            }
            _ => unreachable!(),
        };
        if cli.dry_run {
            if cli.json {
                print_git_json(&preview)?;
            } else {
                print!(
                    "{}",
                    crate::services::presentation::safe(&crate::services::declaration::report(
                        &preview
                    ))
                );
            }
        } else {
            if !cli.json {
                print!(
                    "{}",
                    crate::services::presentation::safe(&crate::services::declaration::report(
                        &preview
                    ))
                );
            }
            approve(cli)?;
            let approval = dort_core::tools::declaration::Approval::from_preview(preview)?;
            let lock = WriterLock::acquire(host)?;
            let tx = dort_core::tools::declaration::execute(host, &lock, approval, |_| Ok(()))?;
            if cli.json {
                mutation_report(
                    match command {
                        ToolsCommand::Edit { .. } => "edit-tool",
                        ToolsCommand::Remove { .. } => "remove-tool",
                        _ => "declare-tool",
                    },
                    tx,
                )?;
            } else {
                println!(
                    "{}",
                    if tx.is_some() {
                        "Declaration saved."
                    } else {
                        "No changes were needed."
                    }
                );
            }
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
                .ok_or_else(|| Error::Invalid("installation record not found".into()))?;
            if cli.json {
                print_git_json(attempt)?;
            } else {
                print!("{}", crate::services::presentation::attempt(attempt));
            }
        } else {
            if cli.json {
                print_git_json(&attempts)?;
            } else if attempts.is_empty() {
                println!("No installation history.");
            } else {
                for attempt in &attempts {
                    print!("{}", crate::services::presentation::attempt(attempt));
                }
            }
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
                brew_prefix: if matches!(provider, Some(InstallationProvider::Brew)) {
                    Some(prefix.clone().ok_or_else(|| {
                        Error::Invalid("Homebrew fallback requires --prefix".into())
                    })?)
                } else {
                    None
                },
            },
        )?;
        if cli.dry_run && cli.json {
            print_git_json(&preview.review())?;
        } else if !cli.json {
            print!(
                "{}",
                crate::services::presentation::close_review(&preview.review())
            );
        }
        if !cli.dry_run {
            approve(cli)?;
            let lock = WriterLock::acquire(host)?;
            let attempt = dort_core::tools::install::close(host, &lock, preview.approve())?;
            if cli.json {
                print_git_json(&attempt)?;
            } else {
                print!("{}", crate::services::presentation::attempt(&attempt));
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    unreachable!("all Tools commands are handled above")
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
        return tools_command(cli, &host, command);
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
            "recovery uses explicit repository and operation IDs; omit --repo".into(),
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
        Command::Clone { source, path } => {
            if cli.repo.is_some() {
                return Err(Error::Invalid(
                    "clone uses its own destination; omit --repo".into(),
                ));
            }
            let path =
                crate::services::resolve_root(path, &crate::environment::launch_directory()?);
            let preview = bootstrap::ClonePreview::prepare(
                &host,
                source,
                &path,
                &crate::environment::resolve_git()?,
            )?;
            if !cli.json {
                println!("{}", preview.human());
            }
            if cli.dry_run {
                if cli.json {
                    print_git_json(&preview)?;
                }
            } else {
                approve(cli)?;
                let id = preview.approve().execute(&host)?;
                print_git_json(
                    &serde_json::json!({"operation":"clone", "repository":path, "repository_id":id}),
                )?;
            }
        }
        Command::Environment { command } => {
            let (profiles, preview_only, resume) = match command {
                EnvironmentCommand::Check(args) => {
                    return declared_tools(cli, &host, args, false, true);
                }
                EnvironmentCommand::Abandon => {
                    let root = local.select_repository(cli.repo.as_deref())?;
                    let preview = dort_core::environment::AbandonPreview::prepare(&host, &root)?;
                    if !cli.json {
                        println!("{}", crate::services::presentation::safe(&preview.human()));
                    }
                    if cli.dry_run {
                        if cli.json {
                            print_git_json(&preview)?;
                        }
                    } else {
                        approve(cli)?;
                        let run = preview.approve().execute(&host)?;
                        if cli.json {
                            print_git_json(&run)?;
                        } else {
                            println!("{}", crate::services::presentation::safe(&run.human()));
                        }
                    }
                    return Ok(ExitCode::SUCCESS);
                }
                EnvironmentCommand::Plan { profiles } => (profiles, true, false),
                EnvironmentCommand::Apply { profiles } => (profiles, false, false),
                EnvironmentCommand::Resume { profiles } => (profiles, false, true),
            };
            let root = local.select_repository(cli.repo.as_deref())?;
            let preview = dort_core::environment::Preview::prepare(&host, &root, profiles, resume)?;
            if !cli.json {
                print!("{}", crate::services::presentation::safe(&preview.human()));
            }
            let blocked = preview.blocked();
            if preview_only || cli.dry_run || blocked {
                if cli.json {
                    print_git_json(&preview.review())?;
                }
                return Ok(ExitCode::from(if blocked { 4 } else { 0 }));
            }
            approve(cli)?;
            let run = preview.approve()?.execute(&host, |_| Ok(()))?;
            if cli.json {
                print_git_json(&run)?;
            } else {
                println!("{}", crate::services::presentation::safe(&run.human()));
            }
            return Ok(ExitCode::from(if run.complete() { 0 } else { 4 }));
        }
        Command::Profiles { command } => {
            let root = local.select_repository(cli.repo.as_deref())?;
            match command {
                ProfileCommand::List { profiles } => {
                    let (manifest, _) =
                        dort_core::observation::read_manifest(&root.join("dort.toml"))?;
                    let rows = local.profile_rows(
                        &manifest.profiles,
                        host.platform,
                        Some(&root),
                        profiles,
                    )?;
                    if cli.json {
                        print_git_json(&rows)?;
                    } else {
                        for row in rows {
                            println!("{}", row.label());
                        }
                    }
                }
                ProfileCommand::Set { name, choice } => {
                    use dort_core::profiles::Choice;
                    let choice = match choice {
                        ProfileChoice::Defaults => Choice::Defaults,
                        ProfileChoice::On => Choice::On,
                        ProfileChoice::Off => Choice::Off,
                    };
                    let preview =
                        dort_core::local::ProfilePreview::prepare(&host, &root, name, choice)?;
                    if !cli.json {
                        println!("{}", preview.human());
                    }
                    if cli.dry_run {
                        if cli.json {
                            print_git_json(&preview)?;
                        }
                    } else {
                        approve(cli)?;
                        preview.approve().execute(&host)?;
                        mutation_report("profiles", None)?;
                    }
                }
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
        Command::Recovery {
            command: RecoveryCommand::RecoverUpdate,
        } => return recover_generated_update(cli, &host),
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
            let (generated, generated_error) = match pending_update(&host) {
                Ok(location) => (location, None),
                Err(error) => (None, Some(error.to_string())),
            };
            let write_blocked = generated.is_some()
                || generated_error.is_some()
                || transactions.iter().any(journal::Diagnosis::blocks_writes);
            if cli.json {
                print_ascii_json(
                    &serde_json::json!({"schema": 1, "read_only": true, "write_blocked": write_blocked, "transactions": transactions, "generated_update": generated, "generated_update_error": generated_error}),
                    "could not encode recovery list",
                )?;
            } else {
                for transaction in &transactions {
                    println!(
                        "{} / {}: {:?} ({}; {} saved steps)",
                        transaction.repository_id,
                        transaction.transaction_id,
                        transaction.phase,
                        operation_type(transaction.journal_schema),
                        transaction.valid_checkpoints
                    );
                    if let Some(problem) = &transaction.problem {
                        println!("  BLOCKED: {problem}");
                    }
                }
                if let Some(location) = generated {
                    println!(
                        "Run `dort recovery recover-update --dry-run` to review the interrupted action.\n{}",
                        generated_update::recovery_review(&location)
                    );
                }
                if let Some(error) = generated_error {
                    println!(
                        "Generated update status unavailable: {}",
                        presentation::inline(&error)
                    );
                }
                println!(
                    "Read-only history. Writes blocked: {write_blocked}. Recovery needs a preview and approval."
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
            FilesCommand::LocalInputs { id } => {
                let root = local_input_repository(cli, &local)?;
                let rows = dort_core::generated::local_inputs::inspect(&host, &root, id)?;
                if cli.json {
                    let inputs: Vec<_> = rows
                        .iter()
                        .map(|row| serde_json::json!({"name": row.name, "value": row.value}))
                        .collect();
                    print_ascii_json(
                        &serde_json::json!({"schema": 1, "read_only": true, "repository": root, "id": id, "inputs": inputs}),
                        "could not encode local inputs",
                    )?;
                } else {
                    println!(
                        "Local inputs for {} on this machine",
                        presentation::inline(id)
                    );
                    if rows.is_empty() {
                        println!("No local inputs declared.");
                    }
                    for row in &rows {
                        println!(
                            "local.{}: {}",
                            presentation::inline(&row.name),
                            presentation::local_input_value(row.value.as_deref())
                        );
                    }
                }
            }
            FilesCommand::SetLocalInput { id, name, value } => {
                return change_local_input(cli, &host, &local, id, name, Some(value));
            }
            FilesCommand::ClearLocalInput { id, name } => {
                return change_local_input(cli, &host, &local, id, name, None);
            }
            FilesCommand::PreviewGenerated { id } => {
                let preview =
                    crate::services::preview_generated(&host, &local, cli.repo.as_deref(), id)?;
                if cli.json {
                    print_ascii_json(&preview, "could not encode generated preview")?;
                } else {
                    print!("{}", crate::services::presentation::generated(&preview));
                }
            }
            FilesCommand::DeployGenerated { id, profiles } => {
                if cli.confirm_remove.is_some() {
                    return Err(Error::Invalid(
                        "Generated deployment does not use --confirm-remove.".into(),
                    ));
                }
                let deployment = crate::services::review_generated_deployment(
                    &host,
                    cli.repo.as_deref(),
                    id,
                    profiles,
                )?;
                if !cli.json {
                    print!(
                        "{}",
                        crate::services::presentation::generated_deployment(deployment.review())
                    );
                }
                if cli.dry_run {
                    if cli.json {
                        print_ascii_json(
                            &deployment.review(),
                            "could not encode generated deployment review",
                        )?;
                    }
                } else {
                    approve(cli)?;
                    let result = deployment.approve().execute()?;
                    if cli.json {
                        print_ascii_json(&result, "could not encode generated deployment result")?;
                    } else {
                        println!(
                            "{}",
                            crate::services::presentation::generated_published(&result)
                        );
                    }
                }
            }
            FilesCommand::UpdateGenerated { id, profiles }
            | FilesCommand::ReplaceGenerated { id, profiles }
            | FilesCommand::RecreateGenerated { id, profiles } => {
                let action = match command {
                    FilesCommand::UpdateGenerated { .. } => UpdateAction::Update,
                    FilesCommand::ReplaceGenerated { .. } => UpdateAction::Replace,
                    _ => UpdateAction::Recreate,
                };
                return change_generated(cli, &host, id, profiles, action);
            }
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
                    for link in &snapshot.inactive_links {
                        println!(
                            "{}: No longer selected. Keep {:?} in place.",
                            link.id, link.target
                        );
                    }
                    for file in &snapshot.files {
                        println!(
                            "{}: {} -> {}",
                            file.mapping.id,
                            file.target_path.display(),
                            file.source_path.as_ref().map_or_else(
                                || "Inline template in dort.toml".into(),
                                |path| path.display().to_string()
                            )
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

fn operation_type(schema: Option<u32>) -> &'static str {
    match schema {
        Some(1) => "file adoption",
        Some(2) => "file change",
        Some(3) => "repository setup",
        Some(4) => "file batch",
        Some(5 | 6) => "tool declaration",
        _ => "unknown",
    }
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
fn local_input_repository(cli: &Cli, local: &LocalConfig) -> dort_core::Result<PathBuf> {
    if cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Local inputs do not use --confirm-remove.".into(),
        ));
    }
    local.select_repository(cli.repo.as_deref())
}

fn change_local_input(
    cli: &Cli,
    host: &HostContext,
    local: &LocalConfig,
    id: &str,
    name: &str,
    value: Option<&str>,
) -> dort_core::Result<ExitCode> {
    let root = local_input_repository(cli, local)?;
    let preview =
        dort_core::generated::local_inputs::Preview::prepare(host, &root, id, name, value)?;
    let action = if value.is_some() { "set" } else { "clear" };
    if cli.dry_run && cli.json {
        let review = preview.review();
        print_ascii_json(
            &serde_json::json!({"schema": 1, "read_only": true, "action": action, "repository": review.repository, "id": review.id, "name": review.name, "previous": review.previous, "proposed": review.proposed}),
            "could not encode local input review",
        )?;
    } else if cli.dry_run || (!cli.yes && interactive(cli)) {
        print_update_review(&presentation::local_input_review(preview.review()))?;
    }
    if cli.dry_run {
        return Ok(ExitCode::SUCCESS);
    }
    approve(cli)?;
    let changed = preview.approve().execute()?;
    if cli.json {
        print_ascii_json(
            &serde_json::json!({"schema": 1, "action": action, "id": id, "name": name, "changed": changed}),
            "could not encode local input result",
        )?;
    } else {
        println!(
            "{} Preview the generated file to see its output.",
            match (changed, value.is_some()) {
                (false, _) => "Local input unchanged.",
                (true, true) => "Local input saved.",
                (true, false) => "Local input cleared.",
            }
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn change_generated(
    cli: &Cli,
    host: &HostContext,
    id: &str,
    profiles: &[String],
    action: UpdateAction,
) -> dort_core::Result<ExitCode> {
    if cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Generated file changes do not use --confirm-remove.".into(),
        ));
    }
    let update =
        crate::services::review_generated_update(host, cli.repo.as_deref(), id, profiles, action)?;
    let change = update.review();
    let review = generated_update::review(&change)?;
    if cli.dry_run && cli.json {
        print_ascii_json(
            &serde_json::json!({"schema": 1, "read_only": true, "action": change.action, "detected": change.detected, "id": change.id, "repository": change.repository, "active_profiles": change.active_profiles, "destination": change.destination, "deployed_permissions": change.deployed_permissions, "recorded_permissions": change.recorded_permissions, "proposed_permissions": change.proposed_permissions, "resolved_inputs": change.resolved_inputs, "review": review}),
            "could not encode generated file review",
        )?;
    } else if !cli.json {
        print_update_review(&review)?;
    }
    if cli.dry_run {
        return Ok(ExitCode::SUCCESS);
    }
    approve(cli)?;
    generated_update_result(cli, update.approve().execute()?, false)
}

fn recover_generated_update(cli: &Cli, host: &HostContext) -> dort_core::Result<ExitCode> {
    if cli.repo.is_some() || cli.confirm_remove.is_some() {
        return Err(Error::Invalid(
            "Generated update recovery is host-wide. Omit --repo and --confirm-remove.".into(),
        ));
    }
    if let Some(location) = pending_update(host)? {
        let review = generated_update::recovery_review(&location);
        if cli.dry_run && cli.json {
            print_ascii_json(
                &serde_json::json!({"schema": 1, "read_only": true, "pending_update": location, "review": review}),
                "could not encode generated update recovery review",
            )?;
        } else if !cli.json {
            print_update_review(&review)?;
        }
        if cli.dry_run {
            return Ok(ExitCode::SUCCESS);
        }
        approve(cli)?;
        if let Some(outcome) = recover_reviewed_update(host, &location)? {
            return generated_update_result(cli, outcome, true);
        }
    }
    if cli.json {
        print_json(&serde_json::json!({"outcome": "no_pending_update", "read_only": cli.dry_run}))?;
    } else {
        println!("No generated update needs recovery.");
    }
    Ok(ExitCode::SUCCESS)
}
fn print_update_review(review: &str) -> dort_core::Result<()> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    out.write_all(review.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|source| Error::Io {
            operation: "display the full generated update review",
            path: "stdout".into(),
            source,
        })
}
fn generated_update_result(
    cli: &Cli,
    outcome: UpdateOutcome,
    recovering: bool,
) -> dort_core::Result<ExitCode> {
    if cli.json {
        print_ascii_json(&outcome, "could not encode generated update result")?;
    } else {
        println!("{}", generated_update::outcome(&outcome));
    }
    Ok(match outcome {
        UpdateOutcome::NeedsRecovery { .. } => ExitCode::from(4),
        UpdateOutcome::NotExchanged { .. } if !recovering => ExitCode::from(4),
        _ => ExitCode::SUCCESS,
    })
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

fn print_git_json(value: &impl Serialize) -> dort_core::Result<()> {
    print_ascii_json(value, "could not encode Git output")
}

fn print_ascii_json(value: &impl Serialize, encode_error: &str) -> dort_core::Result<()> {
    use std::fmt::Write;
    let json =
        serde_json::to_string_pretty(value).map_err(|_| Error::Invalid(encode_error.into()))?;
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
