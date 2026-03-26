pub mod evm;
pub mod solana;

use crate::error::RtxeError;

pub struct ExplainOutput {
    pub text: String,
    pub json: serde_json::Value,
}

pub trait ChainExplainer: Send + Sync {
    fn explain(
        &self,
        tx_hash: &str,
    ) -> impl std::future::Future<Output = Result<ExplainOutput, RtxeError>> + Send;
}

pub fn create_explainer(
    chain_type: &str,
    rpc_url: &str,
) -> Result<Box<dyn ChainExplainerDyn>, RtxeError> {
    match chain_type {
        "evm" => Ok(Box::new(evm::EvmExplainer::new(rpc_url)?)),
        "solana" => Ok(Box::new(solana::SolanaExplainer::new(rpc_url)?)),
        _ => Err(RtxeError::UnsupportedChain(chain_type.to_string())),
    }
}

/// Object-safe wrapper for dynamic dispatch
pub trait ChainExplainerDyn: Send + Sync {
    fn explain_dyn<'a>(
        &'a self,
        tx_hash: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ExplainOutput, RtxeError>> + Send + 'a>,
    >;
}

impl<T: ChainExplainer> ChainExplainerDyn for T {
    fn explain_dyn<'a>(
        &'a self,
        tx_hash: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ExplainOutput, RtxeError>> + Send + 'a>,
    > {
        Box::pin(self.explain(tx_hash))
    }
}
