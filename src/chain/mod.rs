pub mod evm;

use crate::error::RtxeError;
use crate::model::TxExplanation;

pub trait ChainExplainer: Send + Sync {
    fn explain(
        &self,
        tx_hash: &str,
    ) -> impl std::future::Future<Output = Result<TxExplanation, RtxeError>> + Send;
}

pub fn create_explainer(
    chain_type: &str,
    rpc_url: &str,
) -> Result<Box<dyn ChainExplainerDyn>, RtxeError> {
    match chain_type {
        "evm" => Ok(Box::new(evm::EvmExplainer::new(rpc_url)?)),
        _ => Err(RtxeError::UnsupportedChain(chain_type.to_string())),
    }
}

/// Object-safe wrapper for dynamic dispatch
pub trait ChainExplainerDyn: Send + Sync {
    fn explain_dyn<'a>(
        &'a self,
        tx_hash: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TxExplanation, RtxeError>> + Send + 'a>,
    >;
}

impl<T: ChainExplainer> ChainExplainerDyn for T {
    fn explain_dyn<'a>(
        &'a self,
        tx_hash: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TxExplanation, RtxeError>> + Send + 'a>,
    > {
        Box::pin(self.explain(tx_hash))
    }
}
