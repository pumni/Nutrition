# ACL Evaluation Cases

`context-layer-cases.json` contains deterministic verifier cases for valid and invalid ACL/task states. Each case has an expected pass or fail result.

`verify-agent-context.ps1 -SelfTest` copies the ACL fixtures and locked source inputs into a uniquely named temporary directory, applies each case there, reports the observed result, and removes the temporary directory in `finally`. It does not modify repository files, call the network, install modules, or change the working tree.
