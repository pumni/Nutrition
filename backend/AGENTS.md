# Coding Agent Entry Point

This repository uses `.agent/manifest.json` as the machine-readable context-layer manifest.

The coding agent is an implementation executor. An architect-authored task packet and its named context profile are required before any write. The architect owns product, domain, architecture, API, database, dependency, security, privacy, provider, and release decisions. The executor does not select a context profile, design alternatives, widen scope, or add unrequested behavior.

Read the authority contract at `.agent/authority/executor-contract.md`, then read only the context files named by the current packet's profile. Change only the packet's `allowed_paths`; never change `forbidden_paths`. Run the packet's required verification and report gates, then perform changed-path verification and produce the required implementation report.

If a packet, baseline, context profile, contract, scope, or verification precondition is missing or inconsistent, stop and report the most specific exact block code from the authority contract. Do not work around an unresolved architect decision.

The ACL is repository governance only. It does not integrate with nutrition runtime behavior, dependencies, database schema, or migrations.
