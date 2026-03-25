use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TxExplanation {
    pub chain_type: String,
    pub tx_hash: String,
    pub status: TxStatus,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_used: Option<u64>,
    pub gas_price: Option<String>,
    pub fee: Option<String>,
    pub function_called: Option<String>,
    pub actions: Vec<Action>,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub enum TxStatus {
    Success,
    Failure { revert_reason: Option<String> },
    Pending,
}

#[derive(Debug, Serialize)]
pub struct Action {
    pub index: usize,
    pub action_type: ActionType,
    pub description: String,
    pub contract: String,
}

#[derive(Debug, Serialize)]
pub enum ActionType {
    Transfer,
    Approval,
    Swap,
    Mint,
    Burn,
    InternalTransfer,
    Unknown,
}

impl std::fmt::Display for TxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxStatus::Success => write!(f, "Success"),
            TxStatus::Failure { revert_reason } => match revert_reason {
                Some(reason) => write!(f, "Failed: {reason}"),
                None => write!(f, "Failed"),
            },
            TxStatus::Pending => write!(f, "Pending"),
        }
    }
}

impl std::fmt::Display for ActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionType::Transfer => write!(f, "Transfer"),
            ActionType::Approval => write!(f, "Approval"),
            ActionType::Swap => write!(f, "Swap"),
            ActionType::Mint => write!(f, "Mint"),
            ActionType::Burn => write!(f, "Burn"),
            ActionType::InternalTransfer => write!(f, "InternalTransfer"),
            ActionType::Unknown => write!(f, "Unknown"),
        }
    }
}
