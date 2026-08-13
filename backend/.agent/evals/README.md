# ACL Evaluation Cases

`context-layer-cases.json` contains deterministic verifier cases for valid and invalid ACL/task states. Each case has an expected pass or fail result.

`verify-agent-context.ps1 -SelfTest` copies the ACL fixtures and locked source inputs into uniquely named temporary directories, applies each case there, reports the expected failure reason for P09/P10A negatives, and removes temporary directories in `finally`. It exercises real staged, unstaged, untracked, committed, create, modify, delete, registry, provenance, and exact-conformance states. `run-agent-verification.ps1 -SelfTest` separately exercises isolated trusted-root, gate-kind, external-evidence, report, and cleanup cases. Neither suite modifies repository files, calls the network, installs modules, runs Cargo/Docker/database gates, or changes the working tree.

`verify-agent-context.ps1 -CiPolicy` validates the canonical P10C workflows as fixed governance artifacts without adding a YAML dependency. Its 36-case CI matrix copies both workflows and the policy into temporary fixtures, mutates one trust-boundary control per negative case, asserts the specific failure reason, and removes all fixtures in `finally`.
