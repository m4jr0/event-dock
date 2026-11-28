# Backend contracts (milestone 02)

## Ownership and lifecycle

The engine owns retained events. A storage worker exclusively owns each registered store. Network collection and query transport submit work through bounded channels. Local clients use the typed request interface without serialization; remote clients use newline-delimited JSON over TCP.

Stop ingestion to stop intake while retaining events. Explicit backend shutdown stops intake, closes query submission and joins storage workers. Events admitted to the ingestion queue are drained; events still in sockets or waiting for admission may be lost. Drop pending queries to abandon work. Cancellation is cooperative, not instantaneous. Stop and join the query server before unwrapping and shutting down a shared backend.

## Resource accounting

Retention is bounded by event count and logical live payload bytes. Circular payload allocation can exceed the logical budget because of growth and wraparound. Reported storage allocation includes payload, metadata and encoding scratch capacities, not process RSS or allocator overhead. Growth may temporarily require both old and new buffers.

Ingestion admission accounts for wire-frame bytes, not decoded heap size. Socket buffers, decoder buffers and frames waiting for admission are outside that budget. Query admission bounds outstanding requests, including completed unread replies. Query text limits are soft: a first oversized event may be returned alone to preserve pagination progress. Wire response limits are separate. These contracts do not establish a hard process-wide memory ceiling.

## Communication

Query requests have a bounded event limit. Results are newest first, with an exclusive before-ID cursor. Incremental queries do not provide snapshot isolation; concurrent eviction can remove events before they are visited. Event IDs are scoped to a store, not globally unique across sessions or restarts.

Remote requests use bounded frames, bounded connection queues and deadlines. Overload can return a busy error; the best-effort busy response can be truncated. Local and remote requests share the typed query contract. The current TCP protocol is intended for trusted environments; authentication and encryption are not implemented.

## Validation

Run from the repository root:

```sh
cargo fmt --all -- --check
python3 scripts/check_crate_boundaries.py
cargo test --locked --workspace
cargo test --locked -p eventdock-engine --features storage-validation
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo bench --locked -p eventdock-engine --no-run
```

Run performance measurements separately from CI:

```sh
cargo bench -p eventdock-engine --bench storage
cargo bench -p eventdock-engine --bench end_to_end
```

Performance results depend on hardware and scheduling. Do not treat individual timings as portable guarantees. The current end-to-end benchmark measures loopback transport and reported storage allocations, not physical network latency or process RSS.

## Next milestone

Introduce authoritative session identity, catalog and lifecycle, explicit store removal and aggregate resource admission. Keep frontend views separate from engine ownership. Defer session-aware protocol evolution, GUI integration, process memory monitoring and untrusted network deployment to their respective milestones.
