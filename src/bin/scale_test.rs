use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;

use rtxe::chain::create_explainer;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse flags
    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    let positional: Vec<&String> = args.iter().skip(1).filter(|a| !a.starts_with('-')).collect();

    if positional.len() < 2 {
        eprintln!("Usage: scale_test [-v|--verbose] <chain_type> <rpc_url> [concurrency]");
        std::process::exit(1);
    }
    let chain_type = positional[0];
    let rpc_url = positional[1];
    let concurrency: usize = positional.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);

    let hashes = match chain_type.as_str() {
        "evm" => fetch_evm_hashes(rpc_url).await,
        "solana" => fetch_solana_hashes(rpc_url).await,
        _ => {
            eprintln!("Unknown chain type: {chain_type}");
            std::process::exit(1);
        }
    };

    let total = hashes.len();
    println!("Testing {total} {chain_type} transactions with concurrency={concurrency} verbose={verbose}...\n");

    let errors = Arc::new(AtomicU32::new(0));
    let completed = Arc::new(AtomicU32::new(0));
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let error_messages: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let mut handles = vec![];

    for hash in hashes {
        let chain = chain_type.to_string();
        let rpc = rpc_url.to_string();
        let sem = semaphore.clone();
        let errs = errors.clone();
        let comp = completed.clone();
        let err_msgs = error_messages.clone();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let explainer = match create_explainer(&chain, &rpc) {
                Ok(e) => e,
                Err(e) => {
                    errs.fetch_add(1, Ordering::Relaxed);
                    err_msgs.lock().await.push(format!("create: {e}"));
                    return;
                }
            };

            match explainer.explain_dyn(&hash).await {
                Ok(output) => {
                    if verbose {
                        println!("--- {} ---\n{}", hash, output.text);
                    }
                }
                Err(e) => {
                    errs.fetch_add(1, Ordering::Relaxed);
                    let msg = format!("{e}");
                    eprintln!("  ERROR {}: {}", &hash[..20.min(hash.len())], msg);
                    err_msgs.lock().await.push(msg);
                }
            }

            let done = comp.fetch_add(1, Ordering::Relaxed) + 1;
            if !verbose && done % 50 == 0 {
                let err_count = errs.load(Ordering::Relaxed);
                println!("  [{done}/{total}] errors so far: {err_count}");
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let err_count = errors.load(Ordering::Relaxed);
    let success = total as u32 - err_count;

    println!("\n=== {chain_type} Results ===");
    println!("Total: {total}");
    println!("Success: {success}");
    println!("Errors: {err_count}");
    if total > 0 {
        println!("Success rate: {}%", success * 100 / total as u32);
    }

    if err_count > 0 {
        let msgs = error_messages.lock().await;
        let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for msg in msgs.iter() {
            let key = if msg.len() > 80 {
                format!("{}...", &msg[..80])
            } else {
                msg.clone()
            };
            *counts.entry(key).or_default() += 1;
        }
        let mut sorted: Vec<_> = counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        println!("\nError breakdown:");
        for (msg, count) in sorted.iter().take(10) {
            println!("  {count}x {msg}");
        }
    }
}

async fn fetch_evm_hashes(rpc_url: &str) -> Vec<String> {
    use serde_json::json;

    let client = reqwest::Client::new();

    let resp: serde_json::Value = client
        .post(rpc_url)
        .json(&json!({"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let latest = u64::from_str_radix(
        resp["result"].as_str().unwrap().trim_start_matches("0x"),
        16,
    )
    .unwrap();

    let mut all_hashes = Vec::new();
    println!("Fetching EVM txs from blocks {}..{}", latest - 9, latest);

    for block in (latest - 9)..=latest {
        let hex = format!("0x{block:x}");
        let resp: serde_json::Value = client
            .post(rpc_url)
            .json(&json!({"jsonrpc":"2.0","method":"eth_getBlockByNumber","params":[hex, false],"id":1}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        if let Some(txs) = resp["result"]["transactions"].as_array() {
            println!("  Block {block}: {} txs", txs.len());
            for tx in txs {
                if let Some(h) = tx.as_str() {
                    all_hashes.push(h.to_string());
                }
            }
        }
    }
    all_hashes
}

async fn fetch_solana_hashes(rpc_url: &str) -> Vec<String> {
    use serde_json::json;

    let client = reqwest::Client::new();
    let mut all_sigs = Vec::new();

    let addresses = [
        ("Jupiter", "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"),
        ("SPL Token", "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
        ("System", "11111111111111111111111111111111"),
    ];

    for (label, addr) in &addresses {
        let resp: serde_json::Value = client
            .post(rpc_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "getSignaturesForAddress",
                "params": [addr, {"limit": 70, "commitment": "confirmed"}],
                "id": 1
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        if let Some(results) = resp["result"].as_array() {
            let sigs: Vec<String> = results
                .iter()
                .filter(|r| r["err"].is_null())
                .filter_map(|r| r["signature"].as_str().map(|s| s.to_string()))
                .collect();
            println!("  {label}: {} successful txs", sigs.len());
            all_sigs.extend(sigs);
        }
    }

    let mut seen = std::collections::HashSet::new();
    all_sigs.retain(|s| seen.insert(s.clone()));
    all_sigs.truncate(200);
    all_sigs
}
