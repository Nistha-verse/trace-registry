# Trace live demo script — under 3 minutes

This is a **recording script**, not evidence that every step has been run.
The UI lookup and local tests are verified. The mutation/publisher/devnet
sequence is marked untested because P1 was skipped while less than 24 hours
remained before the official deadline.

## 0:00–0:35 — Mainnet readout

1. Open the static web app with `cd web && npm run dev`.
2. Paste
   `TokenzQdBNLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` and click **CHECK**.
3. Point out the labelled result: **ProgramControlled**. The known authority
   is `AeLmXCbPaQHGWRLr2saFsEVfmMNuKnxRAbWCT9P5twgz`, an off-curve PDA.
4. Point out **Verified on-chain — read-only scan · not attested**. The
   off-chain panel remains separate and says controller inference was not run.

The same offline-verified facts are available for Jupiter v6 and Raydium AMM
v4 in `web/public/scan-results.json`; neither is a single-key program.

## 0:35–1:35 — Throwaway program state changes

**UNTESTED IN THIS CHECKOUT — do not present these as recorded evidence.**

On a local validator or devnet, deploy a disposable upgradeable program, then
use the P1 TypeScript publisher and SetAuthority helper (when implemented) to:

1. Call `attest`: a generated keypair authority displays **SingleKey**.
2. Send the upgradeable-loader `SetAuthority` instruction (enum index `4`)
   with a PDA as the new authority. Call `attest` again: the account changes
   to **ProgramControlled** and the transaction logs `AuthorityChanged`.
3. Send the same instruction without a new authority. Call `attest` again:
   the account changes to **Immutable**.

The on-chain behavior behind this sequence is covered by the 10 LiteSVM tests
in `program/tests/test_attest.rs`; a live RPC transaction has not been run.

## 1:35–2:20 — Gate behavior

**The exact live transaction sequence is also untested.** Show the intended
`assert_safe` calls in the test harness or a transaction inspector:

```text
SingleKey + max_risk=ProgramControlled  -> RiskTooHigh
SingleKey + max_risk=SingleKey          -> pass when fresh
ProgramControlled + max_risk=ProgramControlled -> pass when fresh
Immutable + max_risk=Immutable           -> pass when fresh
stale attestation + max_age_slots=0     -> AttestationStale
```

Close on the rule: for current-state guarantees, put `attest` and
`assert_safe(max_age_slots = 0)` in the same transaction.
