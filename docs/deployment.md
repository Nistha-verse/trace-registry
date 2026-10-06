# Trace deployment

Trace uses the program ID declared in `Anchor.toml`:

```text
H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w
```

The same program keypair may be used on multiple clusters, but a program must
be deployed and independently verified on each cluster before it is considered
live there. The repository does not contain wallet keys or deployment claims.

## Prerequisites

- Rust `1.89.0` from `rust-toolchain.toml`.
- Anchor CLI `1.2.0`.
- Solana CLI with a funded deployer wallet.
- An RPC URL appropriate for the target cluster.

Set the wallet explicitly in the shell or pass the provider override. Never
commit the wallet file or private key.

## Build and test

The current LiteSVM test runtime accepts the SBPF v2 artifact, so every local
build and test uses the explicit architecture flag:

```bash
make build
make test
make lint
```

## Devnet

Configure the deployer and RPC endpoint, then run:

```bash
solana config set --url "$SOLANA_RPC_URL"
anchor deploy --provider.cluster devnet
anchor idl fetch H2xqJNtm8nwHu6UxY41NhmP9tC42q6nwQM8PY3PJpt6w \
  --provider.cluster devnet
```

Verify the deployed address and executable account with the Solana explorer or
RPC before publishing the endpoint to users. The public verifier can then be
run against the same cluster:

```bash
cargo run -p trace-cli -- check PROGRAM_ADDRESS \
  --rpc-url "$SOLANA_RPC_URL"
```

## Mainnet promotion

Mainnet deployment is intentionally a separate operator action:

```bash
make deploy-mainnet
```

Before running it:

1. Build with `anchor build --arch v2 --verifiable`.
2. Confirm the program keypair matches the declared ID.
3. Confirm the deployer wallet, RPC URL, and fee budget.
4. Review the generated binary and IDL.
5. Perform a devnet deployment and smoke test first.
6. Record the final deployment signature and cluster in release notes.

After deployment, use the CLI against a mainnet RPC and verify that the
program account is executable and owned by the expected loader.
