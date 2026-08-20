# Worker architecture

The `worker` process is the runtime edge for migrations, the test-only foundation seed, staged FDC
import, privacy retention, bounded job batches, leases, retries, and outbox delivery.

Supported modes are `idle`, `run-once`, and `loop`. Production configuration is explicit and
fail-closed; development fixture behavior is limited to local/CI environments.

Use `cargo xtask postgres` for worker and persistence integration verification.
