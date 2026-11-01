use clap::{Parser, Subcommand, ValueEnum};
use dort_core::{
    Error, Manifest,
    context::HostContext,
    discovery, journal,
    local::LocalConfig,
    model::{FileMode, ManagedFile},
    planner::{self, Request, UnmanageMode},
    profiles::selected,
};
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Git-native developer environment manager (read-only observation and planning)",
    arg_required_else_help = true
)]
struct Cli {
    /// Emit structured output; diagnostics remain on stderr.
    #[arg(long, global = true)]
    json: bool,
    /// Override the locally registered active repository.
    #[arg(long, global = true)]
    repo: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show supported host and local state locations without creating them.
    Doctor,
    /// Read-only discovery, inventory and previews. No file mutation commands exist yet.
    Files {
        #[command(subcommand)]
        command: FilesCommand,
    },
    /// Read-only transaction diagnosis. Never replays or rolls back a transaction.
    Recovery {
        #[command(subcommand)]
        command: RecoveryCommand,
    },
    /// Read-only manifest diagnostics.
    Manifest {
        #[command(subcommand)]
        command: ManifestCommand,
    },
}

#[derive(Subcommand)]
enum RecoveryCommand {
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

#[derive(Clone, Copy, ValueEnum)]
enum RemovalMode {
    Keep,
    Remove,
}

#[derive(Serialize)]
struct DoctorReport {
    host: HostContext,
    registered_repositories: usize,
    active_repository: Option<String>,
    mode: &'static str,
}

#[derive(Serialize)]
struct CheckReport {
    manifest: PathBuf,
    schema: u32,
    active_profiles: BTreeSet<String>,
    selected_tools: usize,
    selected_files: usize,
    tool_dependency_order: Vec<String>,
    unsupported_capabilities: Vec<String>,
    filesystem_and_tools_inspected: bool,
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

fn execute(cli: &Cli) -> dort_core::Result<ExitCode> {
    let host = HostContext::from_env()?;
    // Read only; missing configuration uses defaults without creating directories.
    let local = LocalConfig::load(&host)?;
    match &cli.command {
        Command::Doctor => {
            let report = DoctorReport {
                host,
                registered_repositories: local.repositories.len(),
                active_repository: local.active_repository,
                mode: "read-only foundation",
            };
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
        Command::Recovery {
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
                        "{} / {}: {:?} ({} valid checkpoints)",
                        transaction.repository_id,
                        transaction.transaction_id,
                        transaction.phase,
                        transaction.valid_checkpoints
                    );
                    if let Some(problem) = &transaction.problem {
                        println!("  BLOCKED: {problem}");
                    }
                }
                println!(
                    "Read-only diagnosis; writes blocked: {write_blocked}. Recovery is explicit and currently core-API only."
                );
            }
            if write_blocked {
                return Ok(ExitCode::from(4));
            }
        }
        Command::Files { command } => {
            match command {
                FilesCommand::Discover => {
                    // Discovery does not require an active repository.
                    let root = if cli.repo.is_some() || local.active_repository.is_some() {
                        Some(local.select_repository(cli.repo.as_deref())?)
                    } else {
                        None
                    };
                    let report = discovery::discover(&host, root.as_deref())?;
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
                    let root = local.select_repository(cli.repo.as_deref())?;
                    let requested = requested_profiles(&local, profiles);
                    let snapshot = planner::observe(&host, &root, &requested, Request::Apply)?;
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
                    let root = local.select_repository(cli.repo.as_deref())?;
                    let requested = requested_profiles(&local, profiles);
                    let request = match command {
                        PlanCommand::Apply => Request::Apply,
                        PlanCommand::Adopt { target, id, source } => Request::Adopt {
                            mapping: ManagedFile {
                                id: id.clone(),
                                source: source.clone(),
                                target: target.clone(),
                                mode: FileMode::Link,
                                profiles: vec!["base".into()],
                                requires: vec![],
                                overrides: Default::default(),
                                permissions: None,
                            },
                        },
                        PlanCommand::Unmanage { id, mode } => Request::Unmanage {
                            id: id.clone(),
                            mode: match mode {
                                RemovalMode::Keep => UnmanageMode::Keep,
                                RemovalMode::Remove => UnmanageMode::Remove,
                            },
                        },
                    };
                    let plan = planner::plan(planner::observe(&host, &root, &requested, request)?);
                    if cli.json {
                        print_json(&plan)?;
                    } else {
                        print!("{}", plan.human());
                    }
                    if plan.has_blockers() {
                        return Ok(ExitCode::from(4));
                    }
                }
            }
        }
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
            let text = std::fs::read_to_string(&path).map_err(|source| Error::Io {
                operation: "read manifest",
                path: path.clone(),
                source,
            })?;
            tracing::debug!("validating environment manifest");
            let manifest = Manifest::parse(&text)?;
            let requested: Vec<_> = local
                .active_profiles
                .iter()
                .chain(profiles)
                .cloned()
                .collect();
            // Resolve the repository root for lexical target overlap protection.
            let root = path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .canonicalize()
                .map_err(|source| Error::Io {
                    operation: "resolve manifest directory",
                    path: path.clone(),
                    source,
                })?;
            let active = manifest.validate_for_host(&host, &requested, Some(&root))?;
            let tools: Vec<_> = manifest
                .tools
                .iter()
                .filter(|t| selected(&t.profiles, &active))
                .collect();
            let files: Vec<_> = manifest
                .files
                .iter()
                .filter(|f| selected(&f.profiles, &active))
                .collect();
            let mut unsupported = vec![];
            if !tools.is_empty() {
                unsupported.push("tool inventory and installation".into());
            }
            for file in &files {
                if file.mode != FileMode::Link {
                    unsupported.push(format!("{}: {:?} deployment", file.id, file.mode));
                }
                if !file.requires.is_empty() {
                    unsupported.push(format!("{}: tool prerequisite verification", file.id));
                }
            }
            let report = CheckReport {
                manifest: path,
                schema: manifest.schema,
                selected_tools: tools.len(),
                selected_files: files.len(),
                tool_dependency_order: manifest.dependency_order(host.platform, &active)?,
                active_profiles: active,
                unsupported_capabilities: unsupported,
                filesystem_and_tools_inspected: false,
            };
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

fn requested_profiles(local: &LocalConfig, requested: &[String]) -> Vec<String> {
    local
        .active_profiles
        .iter()
        .chain(requested)
        .cloned()
        .collect()
}

fn print_json(value: &impl Serialize) -> dort_core::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|_| Error::Invalid("could not encode output".into()))?;
    println!("{json}");
    Ok(())
}
