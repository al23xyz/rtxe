use thiserror::Error;

#[derive(Debug, Error)]
pub enum RtxeError {
    #[error("Unsupported chain type: {0}")]
    UnsupportedChain(String),

    #[error("Invalid transaction hash: {0}")]
    InvalidTxHash(String),

    #[error("Transaction not found: {0}")]
    TxNotFound(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("ABI decoding error: {0}")]
    AbiDecode(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
