# 11 — Operating Model After Installation

## Standard feature workflow

### Step 1 — Architect inspects current repository state

The architect determines:
- exact objective;
- current implementation facts;
- impacted module;
- whether a new ADR is necessary;
- behavior/version impact;
- database/API/dependency impact;
- verification oracle.

### Step 2 — Architect writes one task packet

The packet is the implementation contract.

For a large feature, the architect writes multiple ordered packets rather than a single broad prompt.

### Step 3 — Executor receives only

Give coding agent:
1. `prompts/AI_CODING_EXECUTOR_PROMPT.md` equivalent operational instruction;
2. exact task packet path/content;
3. repository checkout.

Do not ask:
"Study the repo and decide the best design."

Instead:
"Implement task packet NUTR-XXX exactly."

### Step 4 — Executor validates before writing

Required:
- baseline;
- ACL integrity;
- packet structure;
- profile;
- current repository state against assumptions.

Mismatch means a block report, not improvisation.

### Step 5 — Executor implements

The executor:
- opens required context;
- changes allowed paths only;
- follows ordered steps;
- does not widen scope.

### Step 6 — Deterministic verification

The executor runs:
- packet gates;
- ACL task scope check;
- repository verification.

### Step 7 — Executor reports evidence

The report is reviewed by the architect.

The architect decides:
- accept;
- request a fix with another packet;
- change architecture;
- supersede a decision;
- run additional evaluation.

The executor never self-approves an architecture change.

## Example: parser change

Bad delegation:

> Improve the hosted parser so it is more accurate and robust. Use the best model and approach.

Correct delegation:
- architect chooses exact parser behavior;
- architect chooses prompt/schema version change;
- architect decides provider/model if any;
- architect defines VietnameseMealBench slice and regression threshold;
- packet names `parser-hosted`;
- packet lists exact files;
- packet specifies retries/semantic checks unchanged or changed;
- executor implements and runs prescribed eval.

## Example: persistence change

Bad delegation:

> Add whatever tables are needed.

Correct delegation:
- architect provides migration filename, tables/columns/indexes/constraints;
- task packet says migration is new, not edit-in-place;
- transaction semantics are specified;
- required repository changes are named;
- packet requires `verify-postgres.ps1`.

## Example: simple bug

Even a simple bug should have:
- reproduction;
- expected behavior;
- allowed path;
- no-impact declarations;
- regression test;
- verification.

Small packets can be short. The contract structure stays stable.

## Context layer maintenance

When an authoritative source document changes:
1. source-lock fails;
2. architect reviews whether derived context changed;
3. architect writes an ACL maintenance packet;
4. executor updates only affected summaries/map/lock;
5. ACL self-test + foundation verification run.

This turns context drift into a visible build failure.
