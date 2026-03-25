use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rtxe", about = "Blockchain transaction explainer for AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Explain a blockchain transaction
    Explain {
        /// Transaction hash
        #[arg(long)]
        hash: String,

        /// RPC endpoint URL
        #[arg(long)]
        rpc: String,

        /// Chain type (evm, solana, bitcoin)
        #[arg(long, rename_all = "lower", default_value = "evm")]
        r#type: String,

        /// Output as JSON instead of plain text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
