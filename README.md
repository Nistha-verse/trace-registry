# Trace

Trace is an on-chain Solana program-risk registry. It reads loader state
directly from account data, stores a public attestation, and exposes gates that
other programs and clients can verify without trusting an off-chain attester.

## What Trace verifies

`attest` accepts executable programs owned by a supported loader:

- Upgradeable BPF: reads the linked `ProgramData` account and its authority.
- Legacy BPF loaders: treats the program as immutable because there is no
  upgrade-authority account.
- Loader v4: reads the management authority and deployment status.

Risk classes are ordered from safest to riskiest:

1. `Immutable`
2. `MultisigProtected`
3. `ProgramControlled`
4. `SingleKey`

An arbitrary PDA is only `ProgramControlled`. It is not called a multisig
unless it matches an immutable Trace multisig configuration with a sorted
member set and threshold. This avoids treating every PDA as a safe controller.

Every attestation is stored at:

```text
PDA(["attestation", target_program], TRACE_PROGRAM_ID)
```

It includes the loader, authority, risk, deploy slot, attestation slot,
attester, refresh count, and multisig metadata. `Attested`, `AuthorityChanged`,
and `DependencyObserved` events provide a queryable history.

## On-chain interfaces

- `attest`: permissionlessly refresh a target’s loader-derived state.
- `assert_safe`: require a fresh attestation below a caller-selected risk.
- `initialize_multisig`: create a Trace-native threshold configuration.
- `initialize_safe_registry`: initialize the administrator-owned allowlist.
- `register_safe_program`: register a target only when its current attestation
  meets the administrator’s risk and freshness policy.
- `assert_known_safe`: require both a live attestation and an enabled allowlist
  record.
- `record_cpi_dependency`: record a dependency observed while the declared
  parent program is the transaction caller, using the instructions sysvar.
- `assert_dependencies_safe`: gate a recorded dependency manifest by freshness
  and risk.

The known-safe registry is an explicit policy layer. Its authority is trusted
to choose the policy, while the attestation facts it references are recomputed
from on-chain accounts by Trace.

## Public verifier

The `trace-cli` crate provides an RPC client, JSON command-line interface, and
small HTTP server:

```bash
cargo run -p trace-cli -- check PROGRAM_ADDRESS \
  --rpc-url https://api.devnet.solana.com

cargo run -p trace-cli -- history PROGRAM_ADDRESS \
  --rpc-url https://api.devnet.solana.com \
  --file .trace/history.jsonl

cargo run -p trace-cli -- serve \
  --rpc-url https://api.devnet.solana.com \
  --bind 127.0.0.1:8787
```

HTTP endpoints:

- `GET /health`
- `GET /v1/programs/{program_address}`
- `GET /v1/programs/{program_address}/history`

The client derives PDAs locally and decodes raw RPC account/event data. It does
not depend on a Trace-hosted database.

## Development

The repository is pinned to Rust `1.89.0`. LiteSVM requires the test artifact
to be built as SBPF v2, so use the provided targets:

```bash
make build
make test
make lint
make cli-help
```

Equivalent commands:

```bash
anchor build --arch v2
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Deployment

The declared program ID is:

```text
H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w
```

Cluster mappings and operator commands are in [`docs/deployment.md`](docs/deployment.md).
Deployment requires an externally managed Solana wallet and funded RPC
environment; no keys are stored in this repository.

## Trust boundaries

- An attestation is a current snapshot and can become stale; use the age gate.
- `MultisigProtected` currently describes Trace-native multisig configurations.
  External multisig protocols remain `ProgramControlled` until an adapter is
  added for their verified account layout.
- Runtime dependency manifests record observed parent-to-dependency checks. A
  manifest is not a static proof that every historical code path contains no
  other CPI target.
- The administrator-controlled known-safe registry is policy metadata, not a
  replacement for the raw loader-derived attestation.
