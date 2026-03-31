use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cargo-proofrun")]
#[command(about = "Deterministic proof-plan compiler for Cargo workspaces")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Plan {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = "ci")]
        profile: String,
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
    },
    Explain {
        #[arg(long, default_value = ".proofrun/plan.json")]
        plan: Utf8PathBuf,
    },
    Doctor {
        #[arg(long, default_value = ".")]
        repo: Utf8PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Plan {
            base,
            head,
            profile,
            repo,
        } => {
            let range = proofrun::git::GitRange { base, head };
            let _plan = proofrun::plan_repo(&repo, range, &profile)?;
        }
        Command::Explain { plan } => {
            let raw = std::fs::read_to_string(&plan)?;
            let plan: proofrun::Plan = serde_json::from_str(&raw)?;
            println!("{}", proofrun::render_explanation(&plan));
        }
        Command::Doctor { repo } => {
            let report = proofrun::doctor_repo(&repo);
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(())
}
