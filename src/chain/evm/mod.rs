pub mod abi_registry;
pub mod token_resolver;

use alloy::consensus::Transaction as TransactionTrait;
use alloy::network::{ReceiptResponse, TransactionResponse};
use alloy::primitives::utils::format_ether;
use alloy::primitives::utils::format_units;
use alloy::primitives::{Address, TxHash, U256};
use alloy::providers::ext::DebugApi;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::types::trace::geth::{
    CallFrame, GethDebugBuiltInTracerType, GethDebugTracerType, GethDebugTracingOptions,
    GethTrace,
};

use crate::chain::ChainExplainer;
use crate::error::RtxeError;
use crate::model::{Action, ActionType, TxExplanation, TxStatus};

use abi_registry::AbiRegistry;
use token_resolver::{TokenInfo, TokenResolver};

pub struct EvmExplainer {
    provider: RootProvider,
    abi_registry: AbiRegistry,
}

impl EvmExplainer {
    pub fn new(rpc_url: &str) -> Result<Self, RtxeError> {
        let url = rpc_url
            .parse()
            .map_err(|e| RtxeError::Rpc(format!("Invalid RPC URL: {e}")))?;
        let provider = ProviderBuilder::new()
            .disable_recommended_fillers()
            .connect_http(url);
        Ok(Self {
            provider,
            abi_registry: AbiRegistry::new(),
        })
    }

    fn format_eth(wei: U256) -> String {
        let formatted = format_ether(wei);
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed} ETH")
    }

    fn format_gwei(wei: u128) -> String {
        let formatted = format_units(U256::from(wei), 9).unwrap_or_else(|_| wei.to_string());
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed} gwei")
    }

    fn classify_event(event_name: &str) -> ActionType {
        let name_part = event_name.split("::").last().unwrap_or(event_name);
        match name_part {
            "Transfer" | "TransferSingle" | "TransferBatch" => ActionType::Transfer,
            "Approval" | "ApprovalForAll" => ActionType::Approval,
            "Swap" => ActionType::Swap,
            "Mint" => ActionType::Mint,
            "Burn" => ActionType::Burn,
            "Deposit" | "Withdrawal" => ActionType::Transfer,
            _ => ActionType::Unknown,
        }
    }

    fn format_event_description(
        event_name: &str,
        params: &[(String, String)],
        token_info: Option<&TokenInfo>,
        contract_addr: Address,
    ) -> String {
        let name_part = event_name.split("::").last().unwrap_or(event_name);
        let symbol = token_info.map(|t| t.symbol.as_str()).unwrap_or("???");
        let decimals = token_info.map(|t| t.decimals).unwrap_or(18);

        match name_part {
            "Transfer" => {
                let from = Self::get_param(params, "from").unwrap_or("?".into());
                let to = Self::get_param(params, "to").unwrap_or("?".into());
                let value = Self::get_param(params, "value").unwrap_or("?".into());
                let formatted = Self::format_param_value(&value, decimals);
                format!("{formatted} {symbol} from {from} to {to}")
            }
            "Approval" => {
                let owner = Self::get_param(params, "owner").unwrap_or("?".into());
                let spender = Self::get_param(params, "spender").unwrap_or("?".into());
                let value = Self::get_param(params, "value").unwrap_or("?".into());
                let formatted = Self::format_param_value(&value, decimals);
                format!("{owner} approved {spender} for {formatted} {symbol}")
            }
            "Swap" => {
                if let Some(amount0_in) = Self::get_param(params, "amount0In") {
                    let amount1_in = Self::get_param(params, "amount1In").unwrap_or("0".into());
                    let amount0_out =
                        Self::get_param(params, "amount0Out").unwrap_or("0".into());
                    let amount1_out =
                        Self::get_param(params, "amount1Out").unwrap_or("0".into());
                    format!("Swap on pool {contract_addr}: amount0In={amount0_in} amount1In={amount1_in} amount0Out={amount0_out} amount1Out={amount1_out}")
                } else {
                    let amount0 = Self::get_param(params, "amount0").unwrap_or("0".into());
                    let amount1 = Self::get_param(params, "amount1").unwrap_or("0".into());
                    format!("Swap on pool {contract_addr}: amount0={amount0} amount1={amount1}")
                }
            }
            "Deposit" => {
                let dst = Self::get_param(params, "dst").unwrap_or("?".into());
                let wad = Self::get_param(params, "wad").unwrap_or("?".into());
                let formatted = Self::format_param_value(&wad, decimals);
                format!("{dst} deposited {formatted} {symbol}")
            }
            "Withdrawal" => {
                let src = Self::get_param(params, "src").unwrap_or("?".into());
                let wad = Self::get_param(params, "wad").unwrap_or("?".into());
                let formatted = Self::format_param_value(&wad, decimals);
                format!("{src} withdrew {formatted} {symbol}")
            }
            "TransferSingle" => {
                let from = Self::get_param(params, "from").unwrap_or("?".into());
                let to = Self::get_param(params, "to").unwrap_or("?".into());
                let id = Self::get_param(params, "id").unwrap_or("?".into());
                let value = Self::get_param(params, "value").unwrap_or("?".into());
                format!("{value}x token#{id} from {from} to {to}")
            }
            "TransferBatch" => {
                let from = Self::get_param(params, "from").unwrap_or("?".into());
                let to = Self::get_param(params, "to").unwrap_or("?".into());
                format!("Batch transfer from {from} to {to}")
            }
            _ => {
                let param_str: String = params
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name_part}({param_str})")
            }
        }
    }

    fn get_param(params: &[(String, String)], name: &str) -> Option<String> {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    fn extract_internal_transfers(trace: &GethTrace, actions: &mut Vec<Action>) {
        if let GethTrace::CallTracer(frame) = trace {
            // Skip the top-level call (it's the tx itself), only process sub-calls
            for sub in &frame.calls {
                Self::walk_call_frame(sub, actions);
            }
        }
    }

    fn walk_call_frame(frame: &CallFrame, actions: &mut Vec<Action>) {
        // Record CALL frames with non-zero value (internal ETH transfers)
        let value = frame.value.unwrap_or(U256::ZERO);
        if !value.is_zero() {
            let next_idx = actions.len() + 1;
            let formatted = Self::format_eth(value);
            actions.push(Action {
                index: next_idx,
                action_type: ActionType::InternalTransfer,
                description: format!(
                    "{formatted} internal transfer from {} to {}",
                    frame.from,
                    frame.to.unwrap_or(Address::ZERO)
                ),
                contract: format!("{}", frame.from),
            });
        }

        for sub in &frame.calls {
            Self::walk_call_frame(sub, actions);
        }
    }

    fn generate_summary(actions: &[Action]) -> Option<String> {
        let has_swap = actions
            .iter()
            .any(|a| matches!(a.action_type, ActionType::Swap));
        let transfers: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a.action_type, ActionType::Transfer))
            .collect();
        let approvals: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a.action_type, ActionType::Approval))
            .collect();

        if has_swap && transfers.len() >= 2 {
            // Summarize as a swap using the first and last transfer descriptions
            Some(format!(
                "Token swap involving {} transfers.",
                transfers.len()
            ))
        } else if has_swap {
            Some("Token swap.".to_string())
        } else if !approvals.is_empty() && transfers.is_empty() {
            Some(format!(
                "Token approval ({} approval(s)).",
                approvals.len()
            ))
        } else if transfers.len() == 1 {
            Some(format!("Token transfer: {}", transfers[0].description))
        } else if transfers.len() > 1 {
            Some(format!(
                "Multiple token transfers ({} transfers).",
                transfers.len()
            ))
        } else if actions.is_empty() {
            Some("Simple ETH transfer or contract call with no decoded events.".to_string())
        } else {
            None
        }
    }

    fn format_param_value(raw_value: &str, decimals: u8) -> String {
        let Ok(val) = raw_value.parse::<U256>() else {
            return raw_value.to_string();
        };
        let formatted = format_units(val, decimals).unwrap_or_else(|_| val.to_string());
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

impl ChainExplainer for EvmExplainer {
    async fn explain(&self, tx_hash: &str) -> Result<TxExplanation, RtxeError> {
        let hash: TxHash = tx_hash
            .parse()
            .map_err(|_| RtxeError::InvalidTxHash(tx_hash.to_string()))?;

        // Fetch transaction and receipt in parallel
        let (tx_result, receipt_result) = tokio::join!(
            self.provider.get_transaction_by_hash(hash),
            self.provider.get_transaction_receipt(hash),
        );

        let tx = tx_result
            .map_err(|e| RtxeError::Rpc(format!("Failed to fetch transaction: {e}")))?
            .ok_or_else(|| RtxeError::TxNotFound(tx_hash.to_string()))?;

        let receipt = receipt_result
            .map_err(|e| RtxeError::Rpc(format!("Failed to fetch receipt: {e}")))?
            .ok_or_else(|| RtxeError::TxNotFound(format!("Receipt not found for {tx_hash}")))?;

        let status = if receipt.status() {
            TxStatus::Success
        } else {
            TxStatus::Failure {
                revert_reason: None,
            }
        };

        let gas_used = Some(receipt.gas_used());
        let gas_price_wei = receipt.effective_gas_price();
        let gas_price = Some(Self::format_gwei(gas_price_wei));
        let fee = gas_used.map(|gu| {
            let fee_wei = U256::from(gu) * U256::from(gas_price_wei);
            Self::format_eth(fee_wei)
        });

        let value = Self::format_eth(tx.value());

        // Decode calldata
        let function_called = {
            let input = tx.input();
            if input.len() >= 4 {
                self.abi_registry.decode_calldata(input).map(|decoded| {
                    let params_str: String = decoded
                        .params
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let fn_name = decoded
                        .function_name
                        .split("::")
                        .last()
                        .unwrap_or(&decoded.function_name);
                    format!("{fn_name}({params_str})")
                })
            } else {
                None
            }
        };

        // Decode logs using ABI registry + resolve token metadata
        let mut token_resolver = TokenResolver::new(self.provider.clone());
        let mut actions = Vec::new();

        for (idx, log) in receipt.inner.logs().iter().enumerate() {
            let contract_addr: Address = log.address();
            let decoded = self.abi_registry.decode_log(&log.inner);

            let (action_type, description) = if let Some(decoded) = &decoded {
                let token_info = token_resolver.resolve(contract_addr).await;
                let desc = Self::format_event_description(
                    &decoded.event_name,
                    &decoded.params,
                    token_info.as_ref(),
                    contract_addr,
                );
                (Self::classify_event(&decoded.event_name), desc)
            } else {
                let topic0 = log
                    .inner
                    .topics()
                    .first()
                    .map(|t| format!("{t}"))
                    .unwrap_or_else(|| "no topics".to_string());
                (
                    ActionType::Unknown,
                    format!("Unknown event (topic0: {topic0})"),
                )
            };

            actions.push(Action {
                index: idx + 1,
                action_type,
                description,
                contract: format!("{contract_addr}"),
            });
        }

        // Attempt trace for internal ETH transfers (gracefully ignore failures)
        let trace_opts = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::BuiltInTracer(
                GethDebugBuiltInTracerType::CallTracer,
            )),
            ..Default::default()
        };
        if let Ok(trace) = self.provider.debug_trace_transaction(hash, trace_opts).await {
            Self::extract_internal_transfers(&trace, &mut actions);
        }

        let summary = Self::generate_summary(&actions);

        Ok(TxExplanation {
            chain_type: "evm".to_string(),
            tx_hash: tx_hash.to_string(),
            status,
            block_number: receipt.block_number(),
            from: format!("{}", tx.from()),
            to: tx.to().map(|a| format!("{a}")),
            value,
            gas_used,
            gas_price,
            fee,
            function_called,
            actions,
            summary,
        })
    }
}
