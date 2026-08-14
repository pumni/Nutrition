# Escalation Policy

Stop only the affected work when a baseline is stale, a protected decision is missing, scope conflicts with the Task Spec, policy is inconsistent, context integrity is stale, or a required verification gate fails without an in-scope correction.

The report must identify:

- the classification;
- the observed fact and exact file, path, symbol, test, or error evidence;
- the existing policy constraint;
- why the current approved task cannot proceed;
- the implementation impact;
- the smallest human decision required.

Do not silently choose a product, domain, API, database, dependency, security, privacy, provider, infrastructure, behavior-version, publication, or release alternative. Safe unrelated verification may continue only when it does not expand scope.
