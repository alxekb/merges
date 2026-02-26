mod commands;
mod config;
mod doctor;
mod git;
mod github;
mod mcp;
mod split;
mod state;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;

#[derive(Parser)]
#[command(
    name = "merges",
    version,
    about = "Break down large PRs into smaller reviewable chunks",
    long_about = "merges helps you split a large feature branch into small, independently\n\
                  mergeable PRs. It keeps chunk branches rebased on main, creates GitHub PRs\n\
                  automatically, and exposes everything as an MCP server for LLM clients."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise merges for the current repository
    Init {
        /// Base branch PRs will target (default: main)
        #[arg(short, long)]
        base: Option<String>,

        /// Use git worktrees — each chunk gets its own directory so your
        /// working tree never changes during push/sync operations
        #[arg(long)]
        worktrees: bool,

        /// Explicit commit/PR message prefix to use instead of auto-detected ticket
        /// (e.g. --commit-prefix JCLARK-97246 for repos with strict hook formats)
        #[arg(long, value_name = "PREFIX")]
        commit_prefix: Option<String>,
    },

    /// Assign changed files to named chunks and create branches.
    /// Pass --plan to run non-interactively (useful for scripting and MCP/LLM clients).
    /// Pass --auto to group files by directory structure automatically.
    Split {
        /// JSON chunk plan: '[{"name":"models","files":["src/models/user.rs"]}]'
        #[arg(long, value_name = "JSON", conflicts_with = "auto")]
        plan: Option<String>,

        /// Automatically group files by top-level directory structure
        #[arg(long, conflicts_with = "plan")]
        auto: bool,
    },

    /// Push chunk branches and create/update GitHub PRs
    Push {
        /// Use stacked PR strategy (each PR targets the previous chunk's branch)
        #[arg(long, conflicts_with = "independent")]
        stacked: bool,

        /// Use independent PR strategy (all PRs target the base branch)
        #[arg(long, conflicts_with = "stacked")]
        independent: bool,
    },

    /// Rebase all chunk branches onto the latest base branch
    Sync,

    /// Show chunk and PR status table
    Status,

    /// Start the MCP stdio server (for LLM clients like Claude or GitHub Copilot)
    Mcp,

    /// Delete local chunk branches (optionally only those whose PRs are merged)
    Clean {
        /// Only delete branches for chunks whose GitHub PRs are merged/closed
        #[arg(long)]
        merged: bool,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Add files to an existing chunk
    Add {
        /// Name of the chunk to add files to
        chunk: String,

        /// Files to add (relative paths)
        #[arg(required = true)]
        files: Vec<String>,
    },

    /// Move a file from one chunk to another
    Move {
        /// File to move (relative path)
        file: String,

        /// Source chunk name
        #[arg(long = "from")]
        from: String,

        /// Destination chunk name
        #[arg(long = "to")]
        to: String,
    },

    /// Validate state consistency (branch existence, worktrees, gitignore)
    Doctor {
        /// Attempt to repair detected issues
        #[arg(long)]
        repair: bool,
    },

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { base, worktrees, commit_prefix } => commands::init::run(base, worktrees, commit_prefix)?,
        Commands::Split { plan, auto } => commands::split::run(plan, auto)?,
        Commands::Push { stacked, independent } => commands::push::run(stacked, independent).await?,
        Commands::Sync => commands::sync::run()?,
        Commands::Status => commands::status::run().await?,
        Commands::Mcp => mcp::run().await?,
        Commands::Clean { merged, yes } => commands::clean::run(merged, yes).await?,
        Commands::Add { chunk, files } => {
            let root = git::repo_root()?;
            commands::add::run(&root, &chunk, &files)?;
        }
        Commands::Move { file, from, to } => {
            let root = git::repo_root()?;
            commands::r#move::run(&root, &file, &from, &to)?;
        }
        Commands::Doctor { repair } => {
            let root = git::repo_root()?;
            let report = doctor::run(&root, repair)?;
            if report.all_ok() {
                println!("✓ All checks passed — state is healthy.");
            } else {
                for issue in &report.issues {
                    println!("✗ {}", issue);
                }
                if !repair {
                    println!("\nRun `merges doctor --repair` to attempt automatic fixes.");
                }
                anyhow::bail!("{} issue(s) found", report.issues.len());
            }
        }
        Commands::Completions { shell } => {
            generate(shell, &mut Cli::command(), "merges", &mut std::io::stdout());
        }
    }

    Ok(())
}
