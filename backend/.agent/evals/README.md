# ACL Evaluation Cases

`context-layer-cases.json` contains deterministic verifier cases for valid and invalid ACL/task states. Each case has an expected pass or fail result.

`verify-agent-context.ps1 -SelfTest` copies the ACL fixtures and locked source inputs into uniquely named temporary directories, applies each case there, reports the expected failure reason for P09/P10A negatives, and removes temporary directories in `finally`. It exercises real staged, unstaged, untracked, committed, create, modify, delete, registry, and exact-conformance states. It does not modify repository files, call the network, install modules, or change the working tree.
