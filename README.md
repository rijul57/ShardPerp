# ShardPerp: Entropy Funding CRDT

A highly parallelized Solana smart contract that calculates dynamic, entropy-based funding rates for decentralized exchanges. 

This program leverages **Conflict-free Replicated Data Types (CRDTs)** and **Account Sharding** to eliminate write-lock contention. Traders are deterministically routed to separate shard accounts based on their pubkey, allowing massive parallel throughput. The system asynchronously reconciles these isolated states into a global accumulator using custom 18-decimal fixed-point math and Shannon entropy calculations.


## Features

* **Parallelized Execution:** Trader updates hit isolated shard PDAs, avoiding global state write-locks.
* **Deterministic Routing:** `u64` conversion of trader pubkeys ensures consistent shard assignment.
* **Order-Agnostic Merging:** CRDT addition guarantees commutative global state reconstruction regardless of the order shards are passed to the reconcile instruction.
* **Dynamic Entropy Adjustments:** Implements a Shannon Entropy (H(t)) model to increase funding sensitivity when market volume becomes one-sided.
* **Integer-Only Fixed-Point Math:** Custom bitwise `fp_log2` and 18-decimal scaling (`1e18`) guarantees cross-validator determinism with zero floating-point operations.


## Architecture

Both state accounts are tightly packed and exactly **50 bytes** to minimize rent exemption costs.

* **GlobalState** (`seeds = [b"global-state"]`): The central hub. Holds the `total_shards` count, the aggregated `global_funding_accumulator`, and the current `funding_sensitivity`. Read-only during trading; write-locked only during reconciliation.
* **Shard** (`seeds = [b"shard", shard_id]`): The edge nodes. Stores isolated `local_funding_delta` and directional volumes (`long_volume`, `short_volume`). Only the shard assigned to a specific trader is write-locked during an `update_position` transaction.


## Prerequisites

To build and test this program, you need the standard Solana development stack:

* [Rust](https://rustup.rs/) (latest stable)
* [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) (`~1.18.0`)


## Setup & Build

1. **Clone the repository and navigate to the directory:**
   ```bash
   git clone <your-repo-url>
   cd shard_perp

2. **Format and Lint the Code**

    Before building, ensure the code is properly formatted and passes all lint checks:

    ```bash
    cargo fmt
    cargo clippy --all-targets -- -D warnings
    ```

3. **Build the Solana Program**

    Compile the Solana program into a deployable shared object (`.so`) file:

    ```bash
    cargo build-sbf
    ```

    The compiled program will be generated in:

    ```text
    target/deploy/
    ```

## Testing

The project includes a comprehensive integration test suite powered by `solana-program-test`, which spins up local validator environments to verify correctness and safety.

Run all tests with:

```bash
cargo test-sbf
```

### Test Coverage

#### 1. Parallel Isolation (`test_1_parallel_isolation.rs`)

Verifies that transactions originating from different traders are routed to their designated shards and remain completely isolated, preventing any cross-shard contamination.

#### 2. CRDT Commutativity (`test_2_crdt_commutativity.rs`)

Ensures shard reconciliation is order-independent by applying merges in randomized sequences and proving the resulting global state remains mathematically identical.

#### 3. Entropy Funding (`test_3_entropy_funding.rs`)

Simulates various market conditions to validate that the entropy-based funding mechanism correctly increases the funding sensitivity multiplier during periods of directional imbalance.

#### 4. Overflow Safety (`test_4_overflow_safety.rs`)

Stress-tests the system using extreme volume values (`u64::MAX / 2`) to demonstrate that all computations remain safe from integer overflows, panics, and unexpected behavior.
