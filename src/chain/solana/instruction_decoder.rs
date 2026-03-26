use solana_sdk::bs58;
use solana_sdk::hash::hash;
use solana_transaction_status::{
    EncodedTransaction, UiInnerInstructions, UiInstruction, UiMessage, UiParsedInstruction,
};

use super::SolanaAction;

pub fn decode_instructions(
    encoded_tx: &EncodedTransaction,
    account_keys: &[String],
) -> Vec<SolanaAction> {
    let mut actions = Vec::new();

    let instructions = match encoded_tx {
        EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
            UiMessage::Parsed(parsed_msg) => &parsed_msg.instructions,
            UiMessage::Raw(raw_msg) => {
                for (idx, ix) in raw_msg.instructions.iter().enumerate() {
                    let program_id = account_keys
                        .get(ix.program_id_index as usize)
                        .cloned()
                        .unwrap_or_else(|| "unknown".to_string());
                    actions.push(SolanaAction {
                        index: idx + 1,
                        action_type: "ProgramInvocation".to_string(),
                        description: format!("Instruction to program {program_id}"),
                        program: program_id,
                    });
                }
                return actions;
            }
        },
        _ => return actions,
    };

    for (idx, ix) in instructions.iter().enumerate() {
        let action = decode_instruction(ix, idx + 1, account_keys);
        actions.push(action);
    }

    actions
}

pub fn decode_inner_instructions(
    inner_instructions: &[UiInnerInstructions],
    account_keys: &[String],
    start_index: usize,
) -> Vec<SolanaAction> {
    let mut actions = Vec::new();
    let mut idx = start_index;

    for inner in inner_instructions {
        for ix in &inner.instructions {
            let action = decode_instruction(ix, idx, account_keys);
            if action.action_type != "ProgramInvocation"
                || !action.description.starts_with("spl-token::")
                    && !action.description.starts_with("system::")
            {
                idx += 1;
                actions.push(action);
            }
        }
    }

    actions
}

fn decode_instruction(
    ix: &UiInstruction,
    index: usize,
    account_keys: &[String],
) -> SolanaAction {
    match ix {
        UiInstruction::Parsed(parsed) => match parsed {
            UiParsedInstruction::Parsed(parsed_ix) => {
                let program = parsed_ix.program.clone();
                let ix_type = parsed_ix
                    .parsed
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let info = parsed_ix.parsed.get("info");

                decode_parsed_instruction(&program, ix_type, info, index)
            }
            UiParsedInstruction::PartiallyDecoded(partial) => {
                // Decode discriminator from raw instruction data
                let ix_data = bs58::decode(&partial.data).into_vec().ok();
                classify_by_program_and_data(&partial.program_id, ix_data.as_deref(), index)
            }
        },
        UiInstruction::Compiled(compiled) => {
            let program_id = account_keys
                .get(compiled.program_id_index as usize)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            let ix_data = bs58::decode(&compiled.data).into_vec().ok();
            classify_by_program_and_data(&program_id, ix_data.as_deref(), index)
        }
    }
}

// --- Known program + discriminator matching ---

/// Compute Anchor-style discriminator: first 8 bytes of SHA256("global:<instruction_name>")
fn anchor_disc(instruction_name: &str) -> [u8; 8] {
    let h = hash(format!("global:{instruction_name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&h.to_bytes()[..8]);
    disc
}

struct DexInfo {
    name: &'static str,
    swap_discriminators: Vec<[u8; 8]>,
}

fn get_dex_info(program_id: &str) -> Option<DexInfo> {
    match program_id {
        // Jupiter v6 — uses custom discriminator scheme, hardcoded from observed txs + IDL
        "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4" => Some(DexInfo {
            name: "Jupiter v6",
            swap_discriminators: vec![
                anchor_disc("route"),
                anchor_disc("routeWithTokenLedger"),
                anchor_disc("sharedAccountsRoute"),
                anchor_disc("sharedAccountsRouteWithTokenLedger"),
                anchor_disc("sharedAccountsExactOutRoute"),
                anchor_disc("exactOutRoute"),
                // Observed discriminators from real swap transactions
                // (Jupiter may use a non-standard discriminator scheme)
                [0xbb, 0x64, 0xfa, 0xcc, 0x31, 0xc4, 0xaf, 0x14],
                [0xe4, 0x45, 0xa5, 0x2e, 0x51, 0xcb, 0x9a, 0x1d],
            ],
        }),
        "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB" => Some(DexInfo {
            name: "Jupiter v4",
            swap_discriminators: vec![anchor_disc("route")],
        }),
        "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc" => Some(DexInfo {
            name: "Orca Whirlpool",
            swap_discriminators: vec![
                anchor_disc("swap"),
                anchor_disc("swap_v2"),
                anchor_disc("two_hop_swap"),
                anchor_disc("two_hop_swap_v2"),
            ],
        }),
        "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8" => Some(DexInfo {
            name: "Raydium AMM",
            swap_discriminators: vec![
                [0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // swapBaseIn
                [0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // swapBaseOut
            ],
        }),
        "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK" => Some(DexInfo {
            name: "Raydium CLMM",
            swap_discriminators: vec![anchor_disc("swap"), anchor_disc("swap_v2")],
        }),
        "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo" => Some(DexInfo {
            name: "Meteora DLMM",
            swap_discriminators: vec![anchor_disc("swap")],
        }),
        "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB" => Some(DexInfo {
            name: "Meteora Pools",
            swap_discriminators: vec![anchor_disc("swap")],
        }),
        _ => None,
    }
}

fn is_swap_discriminator(dex: &DexInfo, data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let disc: [u8; 8] = data[..8].try_into().unwrap();
    dex.swap_discriminators.contains(&disc)
}

fn classify_by_program_and_data(
    program_id: &str,
    data: Option<&[u8]>,
    index: usize,
) -> SolanaAction {
    // Check ComputeBudget first
    if program_id == "ComputeBudget111111111111111111111111111111" {
        return SolanaAction {
            index,
            action_type: "ComputeBudget".to_string(),
            description: format!("Compute budget ({program_id})"),
            program: program_id.to_string(),
        };
    }

    // Check known DEX programs with discriminator matching
    if let Some(dex) = get_dex_info(program_id) {
        let is_swap = data.map(|d| is_swap_discriminator(&dex, d)).unwrap_or(false);

        if is_swap {
            return SolanaAction {
                index,
                action_type: "Swap".to_string(),
                description: format!("{} swap ({})", dex.name, program_id),
                program: program_id.to_string(),
            };
        } else {
            return SolanaAction {
                index,
                action_type: "ProgramInvocation".to_string(),
                description: format!("{} instruction ({})", dex.name, program_id),
                program: program_id.to_string(),
            };
        }
    }

    SolanaAction {
        index,
        action_type: "ProgramInvocation".to_string(),
        description: format!("Instruction to program {program_id}"),
        program: program_id.to_string(),
    }
}

// --- Parsed instruction decoders ---

fn decode_parsed_instruction(
    program: &str,
    ix_type: &str,
    info: Option<&serde_json::Value>,
    index: usize,
) -> SolanaAction {
    let program_id = info
        .and_then(|i| i.get("programId"))
        .and_then(|v| v.as_str())
        .unwrap_or(program);

    match program {
        "system" => decode_system_instruction(ix_type, info, index),
        "spl-token" | "spl-token-2022" => {
            decode_spl_token_instruction(ix_type, info, index, program)
        }
        "spl-associated-token-account" => decode_ata_instruction(ix_type, info, index),
        _ => SolanaAction {
            index,
            action_type: "ProgramInvocation".to_string(),
            description: format!("{program}::{ix_type} ({program_id})"),
            program: program.to_string(),
        },
    }
}

fn decode_system_instruction(
    ix_type: &str,
    info: Option<&serde_json::Value>,
    index: usize,
) -> SolanaAction {
    match ix_type {
        "transfer" => {
            let source = get_str(info, "source").unwrap_or("?");
            let destination = get_str(info, "destination").unwrap_or("?");
            let lamports = get_u64(info, "lamports").unwrap_or(0);
            let sol = lamports as f64 / 1_000_000_000.0;
            let formatted = format!("{sol:.9}");
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            SolanaAction {
                index,
                action_type: "Transfer".to_string(),
                description: format!("{trimmed} SOL from {source} to {destination}"),
                program: "system".to_string(),
            }
        }
        "createAccount" => {
            let source = get_str(info, "source").unwrap_or("?");
            let new_account = get_str(info, "newAccount").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "AccountCreation".to_string(),
                description: format!("Create account {new_account} funded by {source}"),
                program: "system".to_string(),
            }
        }
        _ => SolanaAction {
            index,
            action_type: "ProgramInvocation".to_string(),
            description: format!("system::{ix_type}"),
            program: "system".to_string(),
        },
    }
}

fn decode_spl_token_instruction(
    ix_type: &str,
    info: Option<&serde_json::Value>,
    index: usize,
    program: &str,
) -> SolanaAction {
    match ix_type {
        "transfer" | "transferChecked" => {
            let source = get_str(info, "source").unwrap_or("?");
            let destination = get_str(info, "destination").unwrap_or("?");
            let amount = get_str(info, "amount")
                .or_else(|| {
                    info.and_then(|i| i.get("tokenAmount"))
                        .and_then(|ta| ta.get("uiAmountString"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("?");
            let mint = get_str(info, "mint").unwrap_or("");
            let authority = get_str(info, "authority").unwrap_or("");
            let desc = if !mint.is_empty() {
                format!("{amount} (mint: {mint}) from {source} to {destination}")
            } else {
                format!(
                    "{amount} tokens from {source} to {destination} (authority: {authority})"
                )
            };
            SolanaAction {
                index,
                action_type: "Transfer".to_string(),
                description: desc,
                program: program.to_string(),
            }
        }
        "approve" | "approveChecked" => {
            let source = get_str(info, "source").unwrap_or("?");
            let delegate = get_str(info, "delegate").unwrap_or("?");
            let amount = get_str(info, "amount").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "Approval".to_string(),
                description: format!("{source} approved {delegate} for {amount} tokens"),
                program: program.to_string(),
            }
        }
        "mintTo" | "mintToChecked" => {
            let account = get_str(info, "account").unwrap_or("?");
            let amount = get_str(info, "amount").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "Mint".to_string(),
                description: format!("Minted {amount} tokens to {account}"),
                program: program.to_string(),
            }
        }
        "burn" | "burnChecked" => {
            let account = get_str(info, "account").unwrap_or("?");
            let amount = get_str(info, "amount").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "Burn".to_string(),
                description: format!("Burned {amount} tokens from {account}"),
                program: program.to_string(),
            }
        }
        "initializeAccount" | "initializeAccount2" | "initializeAccount3" => {
            let account = get_str(info, "account").unwrap_or("?");
            let mint = get_str(info, "mint").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "AccountCreation".to_string(),
                description: format!("Initialize token account {account} for mint {mint}"),
                program: program.to_string(),
            }
        }
        _ => SolanaAction {
            index,
            action_type: "ProgramInvocation".to_string(),
            description: format!("{program}::{ix_type}"),
            program: program.to_string(),
        },
    }
}

fn decode_ata_instruction(
    ix_type: &str,
    info: Option<&serde_json::Value>,
    index: usize,
) -> SolanaAction {
    match ix_type {
        "create" | "createIdempotent" => {
            let account = get_str(info, "account").unwrap_or("?");
            let wallet = get_str(info, "wallet").unwrap_or("?");
            let mint = get_str(info, "mint").unwrap_or("?");
            SolanaAction {
                index,
                action_type: "AccountCreation".to_string(),
                description: format!(
                    "Create associated token account {account} for wallet {wallet} (mint: {mint})"
                ),
                program: "spl-associated-token-account".to_string(),
            }
        }
        _ => SolanaAction {
            index,
            action_type: "ProgramInvocation".to_string(),
            description: format!("spl-associated-token-account::{ix_type}"),
            program: "spl-associated-token-account".to_string(),
        },
    }
}

fn get_str<'a>(info: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    info.and_then(|i| i.get(key)).and_then(|v| v.as_str())
}

fn get_u64(info: Option<&serde_json::Value>, key: &str) -> Option<u64> {
    info.and_then(|i| i.get(key)).and_then(|v| v.as_u64())
}
