# rtxe — Rust Tx Explainer

A CLI tool that makes blockchain transactions readable for AI agents. Think block explorer, but outputting structured text that LLMs can reason about.

```
rtxe explain --hash <TX_HASH> --rpc <RPC_URL> --type evm
```

## Why?

AI agents working with blockchain data spend too much effort parsing raw transaction receipts, logs, and traces. **rtxe** does the heavy lifting — fetching, decoding, and formatting transaction data into a clean, structured output that agents can immediately understand and act on.

## Quick Start

```bash
# Build
cargo build --release

# Explain a transaction
./target/release/rtxe explain \
    --hash 0xbfc187a25273a50a893367e64a5637235de3ffe3de0491be820b35418b36a5fe \
    --rpc https://ethereum-rpc.publicnode.com \
    --type evm
```

### Output

```
Transaction: 0xbfc187a25273a50a893367e64a5637235de3ffe3de0491be820b35418b36a5fe
Chain: EVM
Status: Success
Block: 24734747

From: 0xdF36521532b90D5D3448FaCEA5d6617Da1F114b6
To: 0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af
Value: 0 ETH
Fee: 0.00006712814660598 ETH

Actions (5 events):
  1. [Transfer] 5000 FIAS from 0x21e88F4482CD48DEAb1Ca5eDDABDfb14Cb8AD76F to 0xdF36521532b90D5D3448FaCEA5d6617Da1F114b6
  2. [Transfer] 0.02047350697055789 WETH from 0x18Bbe20F81bdcB340325E28a6eE6BB426B7cCbc1 to 0x21e88F4482CD48DEAb1Ca5eDDABDfb14Cb8AD76F
  3. [Transfer] 1470.814322670630531902 DEVVE from 0xdF36521532b90D5D3448FaCEA5d6617Da1F114b6 to 0x18Bbe20F81bdcB340325E28a6eE6BB426B7cCbc1
  4. [Swap] Swap on pool 0x18Bbe20F81bdcB340325E28a6eE6BB426B7cCbc1: amount0=1470814322670630531902 amount1=-20473506970557890
  5. [Swap] Swap on pool 0x21e88F4482CD48DEAb1Ca5eDDABDfb14Cb8AD76F: amount0=-5000000000000000000000 amount1=20473506970557890

Summary: Token swap involving 3 transfers.
```

## CLI Reference

| Flag | Description | Default |
|------|-------------|---------|
| `--hash` | Transaction hash | required |
| `--rpc` | RPC endpoint URL | required |
| `--type` | Chain type (`evm`) | `evm` |
| `--json` | Output as JSON | `false` |

## What It Does

**Fetches** transaction data, receipt, and trace (when available) from any EVM-compatible RPC.

**Decodes** events and calldata using a built-in ABI registry:
- Token standards — ERC-20, ERC-721, ERC-1155
- WETH — Deposit, Withdrawal
- Uniswap V2 — Swap, Mint, Burn, Sync (Pair + Router)
- Uniswap V3 — Swap, Mint, Burn (Pool + Router)

**Resolves** token metadata (symbol, decimals) on-chain via ERC-20 calls, cached per-run.

**Traces** internal ETH transfers via `debug_traceTransaction` using the `callTracer`. Gracefully skipped if the RPC doesn't support it.

**Formats** everything into plain text or JSON (`--json`), with full unshortened addresses for reliable cross-referencing by AI agents.

## Supported Chains

| Chain | Status |
|-------|--------|
| EVM (Ethereum, Polygon, Arbitrum, Base, etc.) | Supported |
| Solana | Planned |
| Bitcoin | Planned |

## Architecture

```
src/
├── main.rs                    # Entry point
├── cli.rs                     # clap CLI definitions
├── error.rs                   # Error types
├── model.rs                   # TxExplanation, Action, ActionType
├── output_formatter.rs        # Text and JSON rendering
└── chain/
    ├── mod.rs                 # ChainExplainer trait + factory
    └── evm/
        ├── mod.rs             # EvmExplainer
        ├── abi_registry.rs    # ABI registry with event/function decoding
        ├── token_resolver.rs  # On-chain ERC-20 metadata resolution
        └── abis/              # Embedded JSON ABI files
```

Built with [alloy-rs](https://alloy.rs/) for all EVM interactions.

## License

MIT
