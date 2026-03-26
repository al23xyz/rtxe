pub mod instruction_decoder;
pub mod token_resolver;

use serde::Serialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use solana_transaction_status::{
    EncodedTransaction, UiMessage, UiTransactionEncoding, UiTransactionStatusMeta,
};

use crate::chain::{ChainExplainer, ExplainOutput};
use crate::error::RtxeError;

// --- Solana-specific model types ---

#[derive(Debug, Serialize)]
struct SolanaExplanation {
    chain_type: String,
    tx_signature: String,
    status: String,
    slot: u64,
    fee_payer: String,
    signers: Vec<String>,
    fee: String,
    compute_units: Option<u64>,
    actions: Vec<SolanaAction>,
    token_balance_changes: Vec<TokenBalanceChange>,
    summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SolanaAction {
    index: usize,
    action_type: String,
    description: String,
    program: String,
}

#[derive(Debug, Clone, Serialize)]
struct TokenBalanceChange {
    owner: String,
    mint: String,
    change: String,
}

// --- Formatting ---

fn format_text(explanation: &SolanaExplanation) -> String {
    let mut out = String::new();

    out.push_str(&format!("Transaction: {}\n", explanation.tx_signature));
    out.push_str(&format!(
        "Chain: {}\n",
        explanation.chain_type.to_uppercase()
    ));
    out.push_str(&format!("Status: {}\n", explanation.status));
    out.push_str(&format!("Slot: {}\n", explanation.slot));

    out.push('\n');
    out.push_str(&format!("Fee Payer: {}\n", explanation.fee_payer));
    if explanation.signers.len() > 1 {
        out.push_str(&format!("Signers: {}\n", explanation.signers.join(", ")));
    }
    out.push_str(&format!("Fee: {}\n", explanation.fee));
    if let Some(cu) = explanation.compute_units {
        out.push_str(&format!("Compute Units: {cu}\n"));
    }

    if !explanation.actions.is_empty() {
        out.push_str(&format!(
            "\nActions ({} instructions):\n",
            explanation.actions.len()
        ));
        for action in &explanation.actions {
            out.push_str(&format!(
                "  {}. [{}] {}\n",
                action.index, action.action_type, action.description
            ));
        }
    }

    if !explanation.token_balance_changes.is_empty() {
        out.push_str("\nToken Balance Changes:\n");
        for change in &explanation.token_balance_changes {
            out.push_str(&format!(
                "  {} {} (mint: {})\n",
                change.owner, change.change, change.mint
            ));
        }
    }

    if let Some(summary) = &explanation.summary {
        out.push_str(&format!("\nSummary: {summary}\n"));
    }

    out
}

fn format_sol(lamports: u64) -> String {
    let sol = lamports as f64 / 1_000_000_000.0;
    if sol == 0.0 {
        "0 SOL".to_string()
    } else {
        let formatted = format!("{sol:.9}");
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed} SOL")
    }
}

// --- SolanaExplainer ---

pub struct SolanaExplainer {
    rpc_client: RpcClient,
}

impl SolanaExplainer {
    pub fn new(rpc_url: &str) -> Result<Self, RtxeError> {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        );
        Ok(Self { rpc_client })
    }
}

impl ChainExplainer for SolanaExplainer {
    async fn explain(&self, tx_hash: &str) -> Result<ExplainOutput, RtxeError> {
        let signature: Signature = tx_hash
            .parse()
            .map_err(|_| RtxeError::InvalidTxHash(tx_hash.to_string()))?;

        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        };

        let tx_response = self
            .rpc_client
            .get_transaction_with_config(&signature, config)
            .await
            .map_err(|e| RtxeError::Rpc(format!("Failed to fetch Solana transaction: {e}")))?;

        let slot = tx_response.slot;

        // Extract meta
        let meta = tx_response
            .transaction
            .meta
            .as_ref()
            .ok_or_else(|| RtxeError::Rpc("Transaction has no metadata".to_string()))?;

        let status = if meta.err.is_none() {
            "Success".to_string()
        } else {
            format!("Failed: {:?}", meta.err.as_ref().unwrap())
        };

        let fee = format_sol(meta.fee);
        let compute_units = meta.compute_units_consumed.clone().map(|cu| cu);

        // Extract account keys and signers from the jsonParsed transaction
        let encoded_tx = &tx_response.transaction.transaction;
        let (account_keys, signers, fee_payer) = match encoded_tx {
            EncodedTransaction::Json(ui_tx) => {
                match &ui_tx.message {
                    UiMessage::Parsed(parsed_msg) => {
                        let keys: Vec<String> = parsed_msg
                            .account_keys
                            .iter()
                            .map(|k| k.pubkey.clone())
                            .collect();
                        let sigs: Vec<String> = parsed_msg
                            .account_keys
                            .iter()
                            .filter(|k| k.signer)
                            .map(|k| k.pubkey.clone())
                            .collect();
                        let payer = keys.first().cloned().unwrap_or("unknown".to_string());
                        (keys, sigs, payer)
                    }
                    UiMessage::Raw(raw_msg) => {
                        let keys = raw_msg.account_keys.clone();
                        let num_signers = raw_msg.header.num_required_signatures as usize;
                        let sigs: Vec<String> = keys.iter().take(num_signers).cloned().collect();
                        let payer = keys.first().cloned().unwrap_or("unknown".to_string());
                        (keys, sigs, payer)
                    }
                }
            }
            _ => {
                return Err(RtxeError::Rpc("Unexpected transaction encoding".to_string()));
            }
        };

        // Decode top-level instructions
        let mut actions = instruction_decoder::decode_instructions(encoded_tx, &account_keys);

        // Decode inner instructions (CPI calls — this is where swap transfers happen)
        {
            use solana_transaction_status::option_serializer::OptionSerializer;
            if let OptionSerializer::Some(inner_ixs) = &meta.inner_instructions {
                let inner_actions = instruction_decoder::decode_inner_instructions(
                    inner_ixs,
                    &account_keys,
                    actions.len() + 1,
                );
                actions.extend(inner_actions);
            }
        }

        // Compute token balance changes from pre/post balances
        let mut token_balance_changes = compute_token_balance_changes(meta);
        // Also include native SOL balance changes
        let sol_changes = compute_sol_balance_changes(meta, &account_keys, meta.fee);
        token_balance_changes.extend(sol_changes);

        let summary = generate_summary(&actions, &token_balance_changes, &fee_payer);

        let explanation = SolanaExplanation {
            chain_type: "solana".to_string(),
            tx_signature: tx_hash.to_string(),
            status,
            slot,
            fee_payer,
            signers,
            fee,
            compute_units,
            actions,
            token_balance_changes,
            summary,
        };

        let text = format_text(&explanation);
        let json = serde_json::to_value(&explanation).map_err(RtxeError::Serialization)?;

        Ok(ExplainOutput { text, json })
    }
}

fn compute_sol_balance_changes(
    meta: &UiTransactionStatusMeta,
    account_keys: &[String],
    fee: u64,
) -> Vec<TokenBalanceChange> {
    let mut changes = Vec::new();
    let pre = &meta.pre_balances;
    let post = &meta.post_balances;

    for (i, (pre_bal, post_bal)) in pre.iter().zip(post.iter()).enumerate() {
        let mut diff = *post_bal as i128 - *pre_bal as i128;
        // Add back the fee for fee payer (index 0) so it reflects the actual transfer
        if i == 0 {
            diff += fee as i128;
        }
        if diff != 0 {
            let sol = diff.unsigned_abs() as f64 / 1_000_000_000.0;
            let formatted = format!("{sol:.9}");
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            let sign = if diff > 0 { "+" } else { "-" };
            let owner = account_keys
                .get(i)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            changes.push(TokenBalanceChange {
                owner,
                mint: "SOL (native)".to_string(),
                change: format!("{sign}{trimmed} SOL"),
            });
        }
    }

    changes
}

fn compute_token_balance_changes(meta: &UiTransactionStatusMeta) -> Vec<TokenBalanceChange> {
    use std::collections::HashMap;
    use solana_transaction_status::option_serializer::OptionSerializer;

    let pre_balances = match &meta.pre_token_balances {
        OptionSerializer::Some(b) => b,
        _ => return vec![],
    };
    let post_balances = match &meta.post_token_balances {
        OptionSerializer::Some(b) => b,
        _ => return vec![],
    };

    // Build map: (account_index, mint) -> (pre_amount, post_amount, decimals, owner)
    #[derive(Default)]
    struct BalanceEntry {
        pre: f64,
        post: f64,
        decimals: u8,
        owner: String,
        mint: String,
    }

    let mut entries: HashMap<(u8, String), BalanceEntry> = HashMap::new();

    for bal in pre_balances {
        let mint = bal.mint.clone();
        let owner = match &bal.owner {
            OptionSerializer::Some(o) => o.clone(),
            _ => "unknown".to_string(),
        };
        let ui_amount = bal
            .ui_token_amount
            .ui_amount
            .unwrap_or(0.0);
        let entry = entries
            .entry((bal.account_index, mint.clone()))
            .or_default();
        entry.pre = ui_amount;
        entry.decimals = bal.ui_token_amount.decimals;
        entry.owner = owner;
        entry.mint = mint;
    }

    for bal in post_balances {
        let mint = bal.mint.clone();
        let owner = match &bal.owner {
            OptionSerializer::Some(o) => o.clone(),
            _ => "unknown".to_string(),
        };
        let ui_amount = bal
            .ui_token_amount
            .ui_amount
            .unwrap_or(0.0);
        let entry = entries
            .entry((bal.account_index, mint.clone()))
            .or_default();
        entry.post = ui_amount;
        entry.decimals = bal.ui_token_amount.decimals;
        if entry.owner.is_empty() || entry.owner == "unknown" {
            entry.owner = owner;
        }
        entry.mint = mint;
    }

    let mut changes: Vec<TokenBalanceChange> = entries
        .values()
        .filter_map(|e| {
            let diff = e.post - e.pre;
            if diff.abs() < 1e-12 {
                return None;
            }
            let sign = if diff > 0.0 { "+" } else { "" };
            let formatted = format!("{sign}{diff}");
            Some(TokenBalanceChange {
                owner: e.owner.clone(),
                mint: e.mint.clone(),
                change: formatted,
            })
        })
        .collect();

    changes.sort_by(|a, b| a.owner.cmp(&b.owner));
    changes
}

fn known_token_symbol(mint: &str) -> Option<&'static str> {
    match mint {
        "So11111111111111111111111111111111111111112" => Some("SOL"),
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" => Some("USDC"),
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB" => Some("USDT"),
        "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263" => Some("BONK"),
        "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN" => Some("JUP"),
        "7vfCXTUXx5WJV5JADk17DUJ4ksgau7utNKj4b963voxs" => Some("WETH"),
        "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So" => Some("mSOL"),
        "7dHbWXmci3dT8UFYWYZweBLXgycu7Y3iL6trKn1Y7ARj" => Some("stSOL"),
        "rndrizKT3MK1iimdxRdWabcF7Zg7AR5T4nud4EkHBof" => Some("RNDR"),
        _ => None,
    }
}

fn shorten_mint(mint: &str) -> String {
    known_token_symbol(mint)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if mint.len() > 12 {
                format!("{}...{}", &mint[..4], &mint[mint.len()-4..])
            } else {
                mint.to_string()
            }
        })
}

fn generate_summary(actions: &[SolanaAction], token_changes: &[TokenBalanceChange], fee_payer: &str) -> Option<String> {
    let transfers: Vec<&SolanaAction> = actions
        .iter()
        .filter(|a| a.action_type == "Transfer")
        .collect();

    // Find the DEX name from actions
    let dex_name = actions
        .iter()
        .find(|a| a.action_type == "Swap")
        .map(|a| {
            a.description
                .split(" (")
                .next()
                .unwrap_or("DEX")
                .to_string()
        });

    if let Some(dex) = &dex_name {
        // Look at fee payer's balance changes to describe the swap
        let payer_changes: Vec<&TokenBalanceChange> = token_changes
            .iter()
            .filter(|c| c.owner == fee_payer)
            .collect();

        let sent: Vec<&&TokenBalanceChange> = payer_changes
            .iter()
            .filter(|c| c.change.starts_with('-'))
            .collect();
        let received: Vec<&&TokenBalanceChange> = payer_changes
            .iter()
            .filter(|c| c.change.starts_with('+'))
            .collect();

        if !sent.is_empty() && !received.is_empty() {
            let s = sent[0];
            let r = received[0];
            Some(format!(
                "Swapped {} ({}) for {} ({}) via {dex}.",
                s.change.trim_start_matches('-'), shorten_mint(&s.mint),
                r.change.trim_start_matches('+'), shorten_mint(&r.mint),
            ))
        } else {
            Some(format!("Token swap via {dex}."))
        }
    } else if transfers.len() == 1 {
        Some(format!("Transfer: {}", transfers[0].description))
    } else if transfers.len() > 1 {
        Some(format!("{} transfers.", transfers.len()))
    } else if actions.is_empty() {
        None
    } else {
        Some(format!("{} instruction(s).", actions.len()))
    }
}
