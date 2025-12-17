//! Sherpack CLI - The Kubernetes package manager with Jinja2 templates

use clap::{Parser, Subcommand};
use miette::Result;
use std::path::PathBuf;

mod commands;

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
}

fn main() -> Result<()> {
    // Setup miette for nice error display
    miette::set_panic_hook();

    let cli = Cli::parse();

    // Set debug level
    if cli.debug {
        // SAFETY: We're the only thread at this point (start of main)
        unsafe { std::env::set_var("RUST_BACKTRACE", "1") };
    }

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
        } => commands::template::run(
            &name,
            &pack,
            &values,
            &set,
            &namespace,
            output_dir.as_deref(),
            show_only.as_deref(),
            show_values,
            cli.debug,
        ),

        Commands::Create { name, output } => commands::create::run(&name, &output),

        Commands::Lint { path, strict } => commands::lint::run(&path, strict),

        Commands::Show { path, all } => commands::show::run(&path, all),
    }
}
