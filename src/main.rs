//! alberto — binario. Toda la lógica vive en la lib (cli/client/commands/tui).

use clap::Parser;

use alberto_cli::cli::{Cli, Cmd};
use alberto_cli::{commands, tui};

#[tokio::main]
async fn main() {
    let result = run().await;
    if let Err(e) = result {
        eprintln!("Error: {:#}", alberto_cli::client::friendly(e));
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Upload(args) => commands::upload::run(args).await,
        Cmd::Node { cmd } => commands::node::run(cmd).await,
        Cmd::Tenant { cmd } => commands::tenant::run(cmd).await,
        Cmd::Admin { cmd } => commands::admin::run(cmd).await,
        Cmd::Config { cmd } => commands::config_cmd::run(cmd),
        Cmd::Tui {
            tenant,
            download_dir,
            grpc,
        } => tui::run(tenant, download_dir, grpc),
        Cmd::Download { id, dest, grpc } => commands::download::run(id, dest, grpc).await,
        Cmd::Completions { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(shell, &mut cmd, "alberto", &mut std::io::stdout());
            Ok(())
        }
    }
}
