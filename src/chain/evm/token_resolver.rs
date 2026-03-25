use alloy::primitives::Address;
use alloy::providers::RootProvider;
use alloy::sol;
use std::collections::HashMap;

sol! {
    #[sol(rpc)]
    contract ERC20 {
        function name() public view returns (string);
        function symbol() public view returns (string);
        function decimals() public view returns (uint8);
    }
}

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub symbol: String,
    pub decimals: u8,
}

pub struct TokenResolver {
    provider: RootProvider,
    cache: HashMap<Address, Option<TokenInfo>>,
}

impl TokenResolver {
    pub fn new(provider: RootProvider) -> Self {
        Self {
            provider,
            cache: HashMap::new(),
        }
    }

    pub async fn resolve(&mut self, address: Address) -> Option<TokenInfo> {
        if let Some(cached) = self.cache.get(&address) {
            return cached.clone();
        }

        let info = self.fetch_token_info(address).await;
        self.cache.insert(address, info.clone());
        info
    }

    async fn fetch_token_info(&self, address: Address) -> Option<TokenInfo> {
        let contract = ERC20::new(address, &self.provider);

        // Build call builders (must outlive the futures)
        let symbol_builder = contract.symbol();
        let decimals_builder = contract.decimals();

        // Call in parallel
        let ( symbol_result, decimals_result) = tokio::join!(
            symbol_builder.call(),
            decimals_builder.call(),
        );

        let decimals = decimals_result.unwrap_or(18);
        let symbol = symbol_result.unwrap_or_default();

        if symbol.is_empty()  {
            return None;
        }

        Some(TokenInfo {
            symbol,
            decimals,
        })
    }
}
