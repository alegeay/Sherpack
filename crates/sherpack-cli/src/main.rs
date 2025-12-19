//! Sherpack CLI - The Kubernetes package manager with Jinja2 templates

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod commands;
mod display;
mod error;
mod exit_codes;
mod util;

use error::CliError;

#[derive(Parser)]
#[command(name = "sherpack")]
#[command(author = "Sherpack Contributors")]
#[command(version)]
#[command(about = "The Kubernetes package manager with Jinja2 templates", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable debug output
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Render pack templates locally
    Template {
        /// Release name (for template context)
        name: String,

        /// Pack path
        pack: PathBuf,

        /// Values file(s) to merge
        #[arg(short = 'f', long = "values")]
        values: Vec<PathBuf>,

        /// Set values on command line (key=value)
        #[arg(long = "set")]
        set: Vec<String>,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Output directory (if not set, outputs to stdout)
        #[arg(long)]
        output_dir: Option<PathBuf>,

        /// Show only specific template
        #[arg(short = 's', long)]
        show_only: Option<String>,

        /// Show rendered values
        #[arg(long)]
        show_values: bool,

        /// Skip schema validation before rendering
        #[arg(long)]
        skip_schema: bool,
    },

    /// Create a new pack
    Create {
        /// Pack name
        name: String,

        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
    },

    /// Lint a pack
    Lint {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Strict mode
        #[arg(long)]
        strict: bool,

        /// Skip schema validation even if schema exists
        #[arg(long)]
        skip_schema: bool,
    },

    /// Show pack information
    Show {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Show all information
        #[arg(long)]
        all: bool,
    },

    /// Validate values against schema
    Validate {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,

        /// External schema file to use (overrides pack schema)
        #[arg(short = 's', long)]
        schema: Option<PathBuf>,

        /// Values file to validate (default: pack's values.yaml)
        #[arg(short = 'f', long = "values")]
        values: Option<PathBuf>,

        /// Additional values files to merge before validation
        #[arg(long = "values-file")]
        values_files: Vec<PathBuf>,

        /// Set values on command line (key=value)
        #[arg(long = "set")]
        set: Vec<String>,

        /// Show verbose output with all validated properties
        #[arg(short, long)]
        verbose: bool,

        /// Output validation results as JSON
        #[arg(long)]
        json: bool,

        /// Strict mode - treat warnings as errors
        #[arg(long)]
        strict: bool,
    },

    /// Package a pack into a distributable archive
    Package {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Output file (default: {name}-{version}.tar.gz)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Sign with key after packaging
        #[arg(long)]
        sign: Option<PathBuf>,
    },

    /// Inspect a packaged archive
    Inspect {
        /// Archive path (.tar.gz)
        archive: PathBuf,

        /// Show only the MANIFEST file
        #[arg(long)]
        manifest: bool,

        /// Show file checksums
        #[arg(long)]
        checksums: bool,
    },

    /// Generate signing keys
    Keygen {
        /// Output directory (default: ~/.sherpack/)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing keys
        #[arg(long)]
        force: bool,

        /// Don't password-protect the secret key
        #[arg(long)]
        no_password: bool,
    },

    /// Sign an archive
    Sign {
        /// Archive to sign
        archive: PathBuf,

        /// Secret key path (default: ~/.sherpack/sherpack.key)
        #[arg(short, long)]
        key: Option<PathBuf>,

        /// Custom trusted comment
        #[arg(long)]
        comment: Option<String>,
    },

    /// Verify archive integrity and signature
    Verify {
        /// Archive to verify
        archive: PathBuf,

        /// Public key for signature verification
        #[arg(short, long)]
        key: Option<PathBuf>,

        /// Fail if no signature present
        #[arg(long)]
        require_signature: bool,
    },

    /// Convert a Helm chart to a Sherpack pack
    Convert {
        /// Path to Helm chart
        chart: PathBuf,

        /// Output directory (default: <chartname>-sherpack)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Overwrite existing output directory
        #[arg(long)]
        force: bool,

        /// Show what would be converted without writing files
        #[arg(long)]
        dry_run: bool,

        /// Show detailed output
        #[arg(short, long)]
        verbose: bool,
    },

    // ========== Phase 4: Kubernetes Deployment ==========

    /// Install a pack to Kubernetes
    Install {
        /// Release name
        name: String,

        /// Pack path or archive
        pack: PathBuf,

        /// Values file(s) to merge
        #[arg(short = 'f', long = "values")]
        values: Vec<PathBuf>,

        /// Set values on command line (key=value)
        #[arg(long = "set")]
        set: Vec<String>,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Wait for resources to be ready
        #[arg(long)]
        wait: bool,

        /// Timeout in seconds (for --wait)
        #[arg(long)]
        timeout: Option<u64>,

        /// Rollback on failure
        #[arg(long)]
        atomic: bool,

        /// Create namespace if not exists
        #[arg(long)]
        create_namespace: bool,

        /// Simulate without applying
        #[arg(long)]
        dry_run: bool,

        /// Show diff before applying
        #[arg(long)]
        diff: bool,

        /// Skip CRD installation (assume CRDs are managed externally)
        #[arg(long)]
        skip_crds: bool,
    },

    /// Upgrade an existing release
    Upgrade {
        /// Release name
        name: String,

        /// Pack path or archive
        pack: PathBuf,

        /// Values file(s) to merge
        #[arg(short = 'f', long = "values")]
        values: Vec<PathBuf>,

        /// Set values on command line (key=value)
        #[arg(long = "set")]
        set: Vec<String>,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Wait for resources to be ready
        #[arg(long)]
        wait: bool,

        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Rollback on failure
        #[arg(long)]
        atomic: bool,

        /// Install if not exists
        #[arg(short, long)]
        install: bool,

        /// Force recreate resources
        #[arg(long)]
        force: bool,

        /// Reset values to defaults
        #[arg(long)]
        reset_values: bool,

        /// Reuse values from previous release
        #[arg(long)]
        reuse_values: bool,

        /// Skip hooks
        #[arg(long)]
        no_hooks: bool,

        /// Simulate without applying
        #[arg(long)]
        dry_run: bool,

        /// Show diff before applying
        #[arg(long)]
        diff: bool,

        /// Strategy for immutable fields (fail|recreate|skip)
        #[arg(long)]
        immutable_strategy: Option<String>,

        /// Max history revisions to keep
        #[arg(long)]
        max_history: Option<u32>,

        /// Skip CRD updates (assume CRDs are managed externally)
        #[arg(long)]
        skip_crd_update: bool,

        /// Force CRD updates even for breaking changes
        #[arg(long)]
        force_crd_update: bool,

        /// Show CRD diff before applying
        #[arg(long)]
        show_crd_diff: bool,
    },

    /// Uninstall a release
    Uninstall {
        /// Release name
        name: String,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Wait for deletion
        #[arg(long)]
        wait: bool,

        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Keep release history
        #[arg(long)]
        keep_history: bool,

        /// Skip hooks
        #[arg(long)]
        no_hooks: bool,

        /// Simulate without deleting
        #[arg(long)]
        dry_run: bool,

        /// Delete CRDs (WARNING: deletes all CustomResources of those types)
        #[arg(long)]
        delete_crds: bool,

        /// Confirm CRD deletion (required with --delete-crds)
        #[arg(long)]
        confirm_crd_deletion: bool,
    },

    /// Rollback to a previous revision
    Rollback {
        /// Release name
        name: String,

        /// Target revision (0 = previous)
        #[arg(default_value = "0")]
        revision: u32,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Wait for resources to be ready
        #[arg(long)]
        wait: bool,

        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,

        /// Force recreate resources
        #[arg(long)]
        force: bool,

        /// Skip hooks
        #[arg(long)]
        no_hooks: bool,

        /// Simulate without applying
        #[arg(long)]
        dry_run: bool,

        /// Show diff before applying
        #[arg(long)]
        diff: bool,

        /// Strategy for immutable fields
        #[arg(long)]
        immutable_strategy: Option<String>,

        /// Max history revisions to keep
        #[arg(long)]
        max_history: Option<u32>,
    },

    /// List installed releases
    #[command(name = "list", alias = "ls")]
    List {
        /// Filter by namespace
        #[arg(short, long)]
        namespace: Option<String>,

        /// List across all namespaces
        #[arg(short = 'A', long)]
        all_namespaces: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show release history
    History {
        /// Release name
        name: String,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Max revisions to show
        #[arg(long)]
        max: Option<usize>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show release status
    Status {
        /// Release name
        name: String,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,

        /// Show resource status
        #[arg(long)]
        resources: bool,

        /// Show values
        #[arg(long)]
        show_values: bool,

        /// Show rendered manifest
        #[arg(long)]
        manifest: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Recover a stuck release
    Recover {
        /// Release name
        name: String,

        /// Target namespace
        #[arg(short, long, default_value = "default")]
        namespace: String,
    },

    // ========== Phase 5: Repository Management ==========

    /// Manage pack repositories
    #[command(subcommand)]
    Repo(RepoCommands),

    /// Search for packs across repositories
    Search {
        /// Search query
        query: String,

        /// Search in specific repository
        #[arg(short, long)]
        repo: Option<String>,

        /// Show all versions
        #[arg(long)]
        versions: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Pull a pack from a repository
    Pull {
        /// Pack reference (repo/name:version or oci://...)
        pack: String,

        /// Specific version to pull (or use pack:version format)
        #[arg(long = "ver")]
        pack_version: Option<String>,

        /// Output file or directory
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Extract to directory instead of saving archive
        #[arg(long)]
        untar: bool,
    },

    /// Push a pack to an OCI registry
    Push {
        /// Archive to push
        archive: PathBuf,

        /// OCI destination (oci://registry/repo:tag)
        destination: String,
    },

    /// Manage pack dependencies
    #[command(subcommand, name = "dependency", alias = "dep")]
    Dependency(DependencyCommands),
}

/// Repository subcommands
#[derive(Subcommand)]
enum RepoCommands {
    /// Add a repository
    Add {
        /// Repository name
        name: String,

        /// Repository URL
        url: String,

        /// Username for authentication
        #[arg(short, long)]
        username: Option<String>,

        /// Password for authentication
        #[arg(short, long)]
        password: Option<String>,

        /// Token for authentication (alternative to username/password)
        #[arg(long)]
        token: Option<String>,
    },

    /// List configured repositories
    #[command(alias = "ls")]
    List {
        /// Show authentication status
        #[arg(long)]
        auth: bool,
    },

    /// Update repository index
    Update {
        /// Repository name (updates all if not specified)
        name: Option<String>,
    },

    /// Remove a repository
    #[command(alias = "rm")]
    Remove {
        /// Repository name
        name: String,
    },
}

/// Dependency subcommands
#[derive(Subcommand)]
enum DependencyCommands {
    /// List dependencies
    #[command(alias = "ls")]
    List {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Resolve and lock dependencies
    Update {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Download dependencies
    Build {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Verify integrity
        #[arg(long)]
        verify: bool,
    },

    /// Show dependency tree
    Tree {
        /// Pack path
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> ExitCode {
    // Setup miette for nice error display
    miette::set_panic_hook();

    let cli = Cli::parse();

    // Set debug level
    if cli.debug {
        // SAFETY: We're the only thread at this point (start of main)
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }

    let result = run_command(cli);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Don't print LintFailed errors - lint command already printed details
            if !matches!(err, CliError::LintFailed { .. }) {
                eprintln!("{:?}", miette::Report::from(err.clone()));
            }
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

fn run_command(cli: Cli) -> error::Result<()> {
    match cli.command {
        Commands::Template {
            name,
            pack,
            values,
            set,
            namespace,
            output_dir,
            show_only,
            show_values,
            skip_schema,
        } => commands::template::run(
            &name,
            &pack,
            &values,
            &set,
            &namespace,
            output_dir.as_deref(),
            show_only.as_deref(),
            show_values,
            skip_schema,
            cli.debug,
        )
        .map_err(CliError::from),

        Commands::Create { name, output } => {
            commands::create::run(&name, &output).map_err(CliError::from)
        }

        Commands::Lint {
            path,
            strict,
            skip_schema,
        } => commands::lint::run(&path, strict, skip_schema),

        Commands::Show { path, all } => commands::show::run(&path, all).map_err(CliError::from),

        Commands::Validate {
            path,
            schema,
            values,
            values_files,
            set,
            verbose,
            json,
            strict,
        } => commands::validate::run(
            &path,
            schema.as_deref(),
            values.as_deref(),
            &values_files,
            &set,
            verbose,
            json,
            strict,
        ),

        Commands::Package { path, output, sign } => {
            commands::package::run(&path, output.as_deref(), sign.as_deref())
                .map_err(CliError::from)
        }

        Commands::Inspect {
            archive,
            manifest,
            checksums,
        } => commands::inspect::run(&archive, manifest, checksums).map_err(CliError::from),

        Commands::Keygen {
            output,
            force,
            no_password,
        } => commands::keygen::run(output.as_deref(), force, no_password).map_err(CliError::from),

        Commands::Sign {
            archive,
            key,
            comment,
        } => commands::sign::run(&archive, key.as_deref(), comment.as_deref())
            .map_err(CliError::from),

        Commands::Verify {
            archive,
            key,
            require_signature,
        } => commands::verify::run(&archive, key.as_deref(), require_signature)
            .map_err(CliError::from),

        Commands::Convert {
            chart,
            output,
            force,
            dry_run,
            verbose,
        } => commands::convert::run(&chart, output.as_deref(), force, dry_run, verbose)
            .map_err(CliError::from),

        // Phase 4: Kubernetes deployment commands (async)
        Commands::Install {
            name,
            pack,
            values,
            set,
            namespace,
            wait,
            timeout,
            atomic,
            create_namespace,
            dry_run,
            diff,
            skip_crds,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::install::run(
                &name,
                &pack,
                &values,
                &set,
                &namespace,
                wait,
                timeout,
                atomic,
                create_namespace,
                dry_run,
                diff,
                skip_crds,
            ))}

        Commands::Upgrade {
            name,
            pack,
            values,
            set,
            namespace,
            wait,
            timeout,
            atomic,
            install,
            force,
            reset_values,
            reuse_values,
            no_hooks,
            dry_run,
            diff,
            immutable_strategy,
            max_history,
            skip_crd_update,
            force_crd_update,
            show_crd_diff,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::upgrade::run(
                &name,
                &pack,
                &values,
                &set,
                &namespace,
                wait,
                timeout,
                atomic,
                install,
                force,
                reset_values,
                reuse_values,
                no_hooks,
                dry_run,
                diff,
                immutable_strategy.as_deref(),
                max_history,
                skip_crd_update,
                force_crd_update,
                show_crd_diff,
            ))}

        Commands::Uninstall {
            name,
            namespace,
            wait,
            timeout,
            keep_history,
            no_hooks,
            dry_run,
            delete_crds,
            confirm_crd_deletion,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::uninstall::run(
                &name,
                &namespace,
                wait,
                timeout,
                keep_history,
                no_hooks,
                dry_run,
                delete_crds,
                confirm_crd_deletion,
            ))}

        Commands::Rollback {
            name,
            revision,
            namespace,
            wait,
            timeout,
            force,
            no_hooks,
            dry_run,
            diff,
            immutable_strategy,
            max_history,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::rollback::run(
                &name,
                revision,
                &namespace,
                wait,
                timeout,
                force,
                no_hooks,
                dry_run,
                diff,
                immutable_strategy.as_deref(),
                max_history,
            ))}

        Commands::List {
            namespace,
            all_namespaces,
            json,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::list::run(
                namespace.as_deref(),
                all_namespaces,
                json,
            ))}

        Commands::History {
            name,
            namespace,
            max,
            json,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::history::run(&name, &namespace, max, json))}

        Commands::Status {
            name,
            namespace,
            resources,
            show_values,
            manifest,
            json,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::status::run(
                &name,
                &namespace,
                resources,
                show_values,
                manifest,
                json,
            ))}

        Commands::Recover { name, namespace } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::recover::run(&name, &namespace))}

        // Phase 5: Repository management commands
        Commands::Repo(subcmd) => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            match subcmd {
                RepoCommands::Add {
                    name,
                    url,
                    username,
                    password,
                    token,
                } => rt.block_on(commands::repo::add(
                    &name,
                    &url,
                    username.as_deref(),
                    password.as_deref(),
                    token.as_deref(),
                )),
                RepoCommands::List { auth } => rt.block_on(commands::repo::list(auth)),
                RepoCommands::Update { name } => {
                    rt.block_on(commands::repo::update(name.as_deref()))
                }
                RepoCommands::Remove { name } => rt.block_on(commands::repo::remove(&name)),
            }
        }

        Commands::Search {
            query,
            repo,
            versions,
            json,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::search::run(&query, repo.as_deref(), versions, json))
        }

        Commands::Pull {
            pack,
            pack_version,
            output,
            untar,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::pull::run(
                &pack,
                pack_version.as_deref(),
                output.as_ref(),
                untar,
            ))
        }

        Commands::Push {
            archive,
            destination,
        } => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            rt.block_on(commands::push::run(&archive, &destination))
        }

        Commands::Dependency(subcmd) => {
            let rt = tokio::runtime::Runtime::new().map_err(|e| CliError::internal(e.to_string()))?;
            match subcmd {
                DependencyCommands::List { path } => rt.block_on(commands::dep::list(&path)),
                DependencyCommands::Update { path } => rt.block_on(commands::dep::update(&path)),
                DependencyCommands::Build { path, verify } => {
                    rt.block_on(commands::dep::build(&path, verify))
                }
                DependencyCommands::Tree { path } => rt.block_on(commands::dep::tree(&path)),
            }
        }
    }
}
