mod chain;
mod cli;
mod error;
mod model;
mod output_formatter;

use clap::Parser;
use cli::{Cli, Commands};
use output_formatter::OutputFormatter;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Explain {
            hash,
            rpc,
            r#type,
            json,
        } => {
            let explainer = match chain::create_explainer(&r#type, &rpc) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            };

            match explainer.explain_dyn(&hash).await {
                Ok(explanation) => {
                    let formatter = OutputFormatter::new();
                    if json {
                        print!("{}", formatter.format_json(&explanation));
                    } else {
                        print!("{}", formatter.format_text(&explanation));
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
