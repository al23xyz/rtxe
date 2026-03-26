mod cli;

use rtxe::chain;

use clap::Parser;
use cli::{Cli, Commands};

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
                Ok(output) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&output.json).unwrap());
                    } else {
                        print!("{}", output.text);
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
