# rtxe — Rust Tx Explainer

A CLI tool that makes blockchain transactions readable for AI agents. Think block explorer, but outputting structured text that LLMs can reason about.

```
rtxe explain --hash <TX_HASH> --rpc <RPC_URL> --type <evm|solana>
```

## Why?

AI agents working with blockchain data spend too much effort parsing raw transaction receipts, logs, and traces. **rtxe** does the heavy lifting — fetching, decoding, and formatting transaction data into a clean, structured output that agents can immediately understand and act on.

## Quick Start

```bash
# Build
cargo build --release

# Explain an EVM transaction
./target/release/rtxe explain \
    --hash 0xbfc187a25273a50a893367e64a5637235de3ffe3de0491be820b35418b36a5fe \
    --rpc https://ethereum-rpc.publicnode.com \
    --type evm

# Explain a Solana transaction
./target/release/rtxe explain \
    --hash 5qL1Gh186s6dLdp3i9c2uUbA5xgFriZR3so44jfDERF2jzd4D7sCEdB5ewNUQCbgVV9AUxfA3CorPPusbk1m65AZ \
    --rpc https://api.mainnet-beta.solana.com \
    --type solana
```

### EVM Output

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

### Solana Output

```
Transaction: 5qL1Gh186s6dLdp3i9c2uUbA5xgFriZR3so44jfDERF2jzd4D7sCEdB5ewNUQCbgVV9AUxfA3CorPPusbk1m65AZ
Chain: SOLANA
Status: Success
Slot: 408794296

Fee Payer: 4cLvukZZjVi7drrtDUQCQukdYVirmhnFMpFtvpTGCdpy
Fee: 0.000057466 SOL
Compute Units: 83624

Actions (7 instructions):
  1. [ProgramInvocation] Instruction to program ComputeBudget111111111111111111111111111111
  2. [ProgramInvocation] Instruction to program ComputeBudget111111111111111111111111111111
  3. [AccountCreation] Create associated token account for wallet 4cLvukZZjVi7drrtDUQCQukdYVirmhnFMpFtvpTGCdpy (mint: So11111111111111111111111111111111111111112)
  4. [Transfer] 0.01 SOL from 4cLvukZZjVi7drrtDUQCQukdYVirmhnFMpFtvpTGCdpy to GF5DDFTTuwi4cLLUrQ2xqnc2hrKAYBCaYhXPGfPznKs7
  5. [ProgramInvocation] spl-token::syncNative
  6. [ProgramInvocation] Instruction to program JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4
  7. [ProgramInvocation] spl-token::closeAccount

Summary: 1 transfer(s) with 1 account creation(s).
```

## CLI Reference

| Flag | Description | Default |
|------|-------------|---------|
| `--hash` | Transaction hash or signature | required |
| `--rpc` | RPC endpoint URL | required |
| `--type` | Chain type (`evm`, `solana`) | `evm` |
| `--json` | Output as JSON | `false` |

## What It Does

Each chain has its own explainer that fetches, decodes, and formats transaction data independently.

### EVM

- Fetches transaction + receipt (and optionally trace via `debug_traceTransaction`)
- Decodes events using built-in ABI registry: ERC-20, ERC-721, ERC-1155, WETH, Uniswap V2/V3
- Resolves token metadata (symbol, decimals) on-chain via ERC-20 calls
- Extracts internal ETH transfers from call traces

### Solana

- Fetches transaction with `jsonParsed` encoding for pre-decoded instructions
- Decodes System Program (SOL transfers, account creation), SPL Token (transfers, approvals, mints, burns), and Associated Token Account (ATA creation)
- Shows fee payer, signers, compute units consumed
- Unknown programs shown with their program ID

### Common

- Full addresses in output for AI agent cross-referencing
- Plain text (default) or JSON (`--json`) output
- Graceful error handling

## Supported Chains

| Chain | Status |
|-------|--------|
| EVM (Ethereum, Polygon, Arbitrum, Base, etc.) | Supported |
| Solana | Supported |
| Bitcoin | Planned |

## Architecture

```
src/
├── main.rs                        # Entry point
├── cli.rs                         # clap CLI definitions
├── error.rs                       # Error types
└── chain/
    ├── mod.rs                     # ChainExplainer trait + factory
    ├── evm/
    │   ├── mod.rs                 # EvmExplainer (model, formatting, RPC)
    │   ├── abi_registry.rs        # ABI registry with event/function decoding
    │   ├── token_resolver.rs      # On-chain ERC-20 metadata resolution
    │   └── abis/                  # Embedded JSON ABI files
    └── solana/
        ├── mod.rs                 # SolanaExplainer (model, formatting, RPC)
        ├── instruction_decoder.rs # Decode known Solana program instructions
        └── token_resolver.rs      # SPL token metadata (placeholder)
```

Each chain explainer owns its model types and output formatting — no shared model across chains.

Built with [alloy-rs](https://alloy.rs/) for EVM and [solana-client](https://crates.io/crates/solana-client) for Solana.

## License

MIT
