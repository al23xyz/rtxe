use alloy::dyn_abi::{DecodedEvent, DynSolValue, EventExt, JsonAbiExt};
use alloy::json_abi::{Event, Function, JsonAbi};
use alloy::primitives::{FixedBytes, Log as PrimitiveLog};
use std::collections::HashMap;

use crate::error::RtxeError;

struct RegisteredEvent {
    name: String,
    abi_event: Event,
}

struct RegisteredFunction {
    name: String,
    abi_function: Function,
}

pub struct DecodedLog {
    pub event_name: String,
    pub params: Vec<(String, String)>,
}

pub struct DecodedCalldata {
    pub function_name: String,
    pub params: Vec<(String, String)>,
}

pub struct AbiRegistry {
    events: HashMap<FixedBytes<32>, Vec<RegisteredEvent>>,
    functions: HashMap<FixedBytes<4>, Vec<RegisteredFunction>>,
}

impl AbiRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            events: HashMap::new(),
            functions: HashMap::new(),
        };

        let abis: &[(&str, &str)] = &[
            ("ERC-20", include_str!("abis/erc20.json")),
            ("ERC-721", include_str!("abis/erc721.json")),
            ("ERC-1155", include_str!("abis/erc1155.json")),
            ("WETH", include_str!("abis/weth.json")),
            ("Uniswap V2 Pair", include_str!("abis/uniswap_v2_pair.json")),
            (
                "Uniswap V2 Router",
                include_str!("abis/uniswap_v2_router.json"),
            ),
            ("Uniswap V3 Pool", include_str!("abis/uniswap_v3_pool.json")),
            (
                "Uniswap V3 Router",
                include_str!("abis/uniswap_v3_router.json"),
            ),
        ];

        for (label, json) in abis {
            if let Err(e) = registry.load_abi(label, json) {
                eprintln!("Warning: failed to load {label} ABI: {e}");
            }
        }

        registry
    }

    fn load_abi(&mut self, label: &str, json: &str) -> Result<(), RtxeError> {
        let items: Vec<serde_json::Value> =
            serde_json::from_str(json).map_err(RtxeError::Serialization)?;

        let abi: JsonAbi = serde_json::from_value(serde_json::Value::Array(items))
            .map_err(RtxeError::Serialization)?;

        for event in abi.events() {
            let selector = event.selector();
            self.events
                .entry(selector)
                .or_default()
                .push(RegisteredEvent {
                    name: format!("{label}::{}", event.name),
                    abi_event: event.clone(),
                });
        }

        for function in abi.functions() {
            let selector = function.selector();
            self.functions
                .entry(selector)
                .or_default()
                .push(RegisteredFunction {
                    name: format!("{label}::{}", function.name),
                    abi_function: function.clone(),
                });
        }

        Ok(())
    }

    pub fn decode_log(&self, log: &PrimitiveLog) -> Option<DecodedLog> {
        let topic0 = log.topics().first()?;
        let registered = self.events.get(topic0)?;

        // Try each registered event with matching topic0
        // ERC-20 vs ERC-721 Transfer disambiguation: ERC-721 has 4 topics, ERC-20 has 3
        for reg in registered {
            let expected_topics = 1 + reg
                .abi_event
                .inputs
                .iter()
                .filter(|p| p.indexed)
                .count();

            if log.topics().len() != expected_topics {
                continue;
            }

            match reg.abi_event.decode_log(log) {
                Ok(decoded) => {
                    let params = Self::extract_event_params(&reg.abi_event, &decoded);
                    return Some(DecodedLog {
                        event_name: reg.name.clone(),
                        params,
                    });
                }
                Err(_) => continue,
            }
        }

        None
    }

    pub fn decode_calldata(&self, input: &[u8]) -> Option<DecodedCalldata> {
        if input.len() < 4 {
            return None;
        }

        let selector = FixedBytes::<4>::from_slice(&input[..4]);
        let registered = self.functions.get(&selector)?;

        for reg in registered {
            match reg.abi_function.abi_decode_input(&input[4..]) {
                Ok(decoded) => {
                    let params: Vec<(String, String)> = reg
                        .abi_function
                        .inputs
                        .iter()
                        .zip(decoded.iter())
                        .map(|(param, value)| {
                            (param.name.clone(), Self::format_dyn_value(value))
                        })
                        .collect();

                    return Some(DecodedCalldata {
                        function_name: reg.name.clone(),
                        params,
                    });
                }
                Err(_) => continue,
            }
        }

        None
    }

    fn extract_event_params(event: &Event, decoded: &DecodedEvent) -> Vec<(String, String)> {
        let mut params = Vec::new();
        let mut indexed_idx = 0;
        let mut body_idx = 0;

        for input in &event.inputs {
            if input.indexed {
                if let Some(val) = decoded.indexed.get(indexed_idx) {
                    params.push((input.name.clone(), Self::format_dyn_value(val)));
                }
                indexed_idx += 1;
            } else {
                if let Some(val) = decoded.body.get(body_idx) {
                    params.push((input.name.clone(), Self::format_dyn_value(val)));
                }
                body_idx += 1;
            }
        }

        params
    }

    fn format_dyn_value(val: &DynSolValue) -> String {
        match val {
            DynSolValue::Address(a) => format!("{a}"),
            DynSolValue::Uint(u, _) => u.to_string(),
            DynSolValue::Int(i, _) => i.to_string(),
            DynSolValue::Bool(b) => b.to_string(),
            DynSolValue::String(s) => s.clone(),
            DynSolValue::Bytes(b) => format!("0x{}", alloy::primitives::hex::encode(b)),
            DynSolValue::FixedBytes(b, _) => format!("{b}"),
            DynSolValue::Array(arr) | DynSolValue::FixedArray(arr) => {
                let inner: Vec<String> = arr.iter().map(Self::format_dyn_value).collect();
                format!("[{}]", inner.join(", "))
            }
            DynSolValue::Tuple(t) => {
                let inner: Vec<String> = t.iter().map(Self::format_dyn_value).collect();
                format!("({})", inner.join(", "))
            }
            _ => format!("{val:?}"),
        }
    }
}
