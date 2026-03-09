mod config;
mod errors;
mod types;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use errors::GroveError;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "grove",
    version = VERSION,
    about = "Multi-agent orchestration for AI coding agents",
    long_about = None,
)]
struct Cli {
    /// Path to the project root (overrides automatic detection).
    #[arg(long, global = true, value_name = "PATH")]
    project: Option<PathBuf>,

    /// Output results as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show the resolved configuration for the current project.
    Config(ConfigArgs),
}

#[derive(clap::Args)]
struct ConfigArgs {
    /// Show only the specified field (e.g. project.root).
    #[arg(long, value_name = "FIELD")]
    field: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let cwd = std::env::current_dir()?;
    let override_path = cli.project.as_deref();

    match cli.command {
        Commands::Config(args) => {
            let cfg = config::load_config(&cwd, override_path)
                .map_err(|e: GroveError| anyhow::anyhow!("{e}"))?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&cfg)?);
            } else {
                match args.field.as_deref() {
                    Some("project.root") => println!("{}", cfg.project.root),
                    Some("project.name") => println!("{}", cfg.project.name),
                    Some("project.canonicalBranch") => {
                        println!("{}", cfg.project.canonical_branch)
                    }
                    Some(field) => {
                        eprintln!("[grove] Unknown field: {field}");
                        std::process::exit(1);
                    }
                    None => {
                        println!("{}", serde_json::to_string_pretty(&cfg)?);
                    }
                }
            }
        }
    }

    Ok(())
}
