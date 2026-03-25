use crate::model::TxExplanation;

pub struct OutputFormatter;

impl OutputFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format_text(&self, explanation: &TxExplanation) -> String {
        let mut out = String::new();

        out.push_str(&format!("Transaction: {}\n", explanation.tx_hash));
        out.push_str(&format!("Chain: {}\n", explanation.chain_type.to_uppercase()));
        out.push_str(&format!("Status: {}\n", explanation.status));

        if let Some(block) = explanation.block_number {
            out.push_str(&format!("Block: {block}\n"));
        }

        out.push('\n');
        out.push_str(&format!("From: {}\n", explanation.from));

        match &explanation.to {
            Some(to) => out.push_str(&format!("To: {to}\n")),
            None => out.push_str("To: (Contract Creation)\n"),
        }

        out.push_str(&format!("Value: {}\n", explanation.value));

        if let Some(fee) = &explanation.fee {
            out.push_str(&format!("Fee: {fee}\n"));
        }

        if let Some(func) = &explanation.function_called {
            out.push_str(&format!("\nFunction Called: {func}\n"));
        }

        if !explanation.actions.is_empty() {
            out.push_str(&format!("\nActions ({} events):\n", explanation.actions.len()));
            for action in &explanation.actions {
                out.push_str(&format!(
                    "  {}. [{}] {}\n",
                    action.index, action.action_type, action.description
                ));
            }
        }

        if let Some(summary) = &explanation.summary {
            out.push_str(&format!("\nSummary: {summary}\n"));
        }

        out
    }

    pub fn format_json(&self, explanation: &TxExplanation) -> String {
        serde_json::to_string_pretty(explanation).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    }
}
