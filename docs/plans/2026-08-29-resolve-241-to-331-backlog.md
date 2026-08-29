# Resolve Issues #241–#331 Backlog

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Resolve the 11 remaining open issues in the #241–#331 range across parser, formatter, linter, MCP, and dependencies.

**Architecture:** Issues are grouped into 5 domain-focused batches, each isolated to a single subsystem (parser, formatter, linter, MCP tools, deps). Batches are independent and can be executed in parallel or sequentially. Each batch follows TDD cycle (Red → Green → Refactor) per AGENTS.md.

**Tech Stack:** Rust (single crate), tree-sitter, PL/pgSQL parser, MCP server (json2sql/tokenize/validate/parse)

**Pre-requisite check:** Issues #241 and #320 are already fixed in main (PRs #242 and #321) but GitHub issues were never closed. Before starting work, close those two issues with a note referencing the merged PRs.

---

## Batch 1: Parser Bug — CREATE AGGREGATE Function Syntax (#331)

**Goal:** Support `CREATE AGGREGATE schema.name(arg_type) ( SFUNC = schema.func, STYPE = type )` function-style syntax.

**Context:** #330 already fixed the `SFUNC = schema.func` dot issue for old-style syntax. #331 is about the **function-style** syntax (PG 9.4+) where the aggregate name itself has a schema qualifier: `CREATE AGGREGATE public.group_concat(text) (...)`.

### Task 1.1: Write failing test for CREATE AGGREGATE function-style (schema-qualified)

**Files:**
- Modify: `tests/sqlparser_common.rs` (or analogous test file for DDL statements)

**Step 1: Write the failing test**
Add a test that parses `CREATE AGGREGATE public.group_concat(text) ( SFUNC = public._group_concat, STYPE = text )` and expects a `CreateAggregateStatement` with:
- `name` = `ObjectName(vec![Ident("public"), Ident("group_concat")])`
- `args` = `vec![DataType::Text]`
- `sfunc` = `ObjectName(vec![Ident("public"), Ident("_group_concat")])`
- `stype` = `DataType::Text`

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_create_aggregate_function_style`
Expected: FAIL (parser error at `.`)

**Step 3: Fix parser to accept schema-qualified aggregate name in function-style syntax**

**Files:**
- Modify: `parser/` — likely `parse_create_aggregate` or equivalent function

Find the `CREATE AGGREGATE` parsing path. The issue is that the function-style syntax expects a simple identifier for the aggregate name but receives `public.group_concat`. The fix:
- In the function-style branch, parse the aggregate name as `ObjectName` (allowing schema-qualified) instead of a simple `Ident`
- Ensure the argument list `(text)` is parsed correctly after the qualified name

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_create_aggregate_function_style`
Expected: PASS

**Step 5: Run full gate**
```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

**Step 6: Close issue #331 on GitHub**

---

## Batch 2: Formatter Fixes (#311, #304, #305)

### Task 2.1: Fix keyword_case incorrectly uppercasing identifiers (#311)

**Bug:** `keyword_case=upper` converts `a.name` to `a.NAME` because `name` is treated as a keyword.

**Files:**
- Modify: `src/formatter/` — likely `keyword_case` implementation or token classification

**Step 1: Write failing test**
Test that `keyword_case=upper` formats:
- `SELECT a.id, a.name FROM users` → `SELECT a.id, a.name FROM users` (identifiers unchanged)
- `SELECT a.id, a.NAME FROM users` → `SELECT a.id, a.NAME FROM users` (already uppercase unchanged)
- `select a.id, a.name from users` → `SELECT a.id, a.name FROM users` (keywords uppercased, idents lowercase)

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_keyword_case_preserves_identifiers`
Expected: FAIL (identifiers incorrectly uppercased)

**Step 3: Fix implementation**
The root cause: `keyword_case` implementation operates on token level without distinguishing `Keyword` vs `Ident` token types. Fix:
- In the `keyword_case` formatting pass, check token type before applying case transformation
- Only apply case change to `Keyword` tokens, leave `Ident` tokens unchanged
- Handle edge cases: `name` is both a keyword and a common identifier — rely on token type from the lexer

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_keyword_case_preserves_identifiers`
Expected: PASS

**Step 5: Run full gate**
```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

### Task 2.2: Improve PL/SQL formatting for PACKAGE/PROCEDURE/FUNCTION bodies (#304)

**Problems:**
1. Output first line has leading whitespace
2. PACKAGE inner PROCEDURE/FUNCTION has no indentation
3. No blank line between PROCEDUREs
4. `RETURN` missing preceding space in `FUNCTION ... RETURN VARCHAR2`

**Files:**
- Modify: `src/formatter/` — PL/SQL formatting logic

**Step 1: Write failing test(s)**
Add test with a PACKAGE BODY containing multiple procedures, checking:
- First line no leading whitespace
- Procedures indented inside PACKAGE
- Blank lines between procedures
- `RETURN` has preceding space

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_plsql_package_formatting`

**Step 3: Fix implementation**
- Strip leading whitespace from output
- Track nesting depth in PL/SQL block structure (PACKAGE → PROCEDURE → BEGIN → END)
- Add indentation per nesting level
- Insert blank lines between PROCEDURE/FUNCTION definitions
- Add space before `RETURN` keyword

**Step 4: Run test to verify it passes**

**Step 5: Run full gate**

### Task 2.3: Add PL/SQL-specific formatting options (#305)

**Note:** This is a feature enhancement that builds on (#304). Implement Phase 1 only (default reasonable indentation) — the option parameters (Phase 2) can be deferred.

**Files:**
- Modify: `src/formatter/` — add PL/SQL formatting options

**Step 1: Write failing test**
Test that with default settings, PL/SQL blocks are formatted with proper indentation.

**Step 2: Run test to verify it fails**

**Step 3: Implement default PL/SQL formatting**
- Add `block_indent` option (default = indent setting)
- Add `procedure_newline` option (default = true)
- Ensure DECLARE/BEGIN/EXCEPTION/END block indentation works

**Step 4: Run test to verify it passes**

**Step 5: Run full gate**

---

## Batch 3: Linter Improvement (#314)

**Goal:** `P009` should suggest `COALESCE(a, b)` for `NVL(a, b)` instead of `CASE WHEN a IS NOT NULL THEN a ELSE b END`.

**Context:** Per comment on #314 (c2j), P009 currently only triggers in WHERE context, not in SELECT list. The fix should:
1. Check if P009 coverage gap (SELECT list) is a pre-existing issue to fix
2. For `NVL(a, b)`: suggest `COALESCE(a, b)` 
3. For `NVL2(a, b, c)`: suggest `CASE WHEN a IS NOT NULL THEN b ELSE c END`
4. For `DECODE`/`IIF`: keep existing CASE suggestion

### Task 3.1: Write failing test

**Files:**
- Modify: `src/linter/rules_performance.rs` tests

**Step 1: Write the failing test**
Add test for `NVL(a, b)` → suggestion contains `COALESCE`
Add test for `NVL2(a, b, c)` → suggestion contains `CASE WHEN`
Add test for `DECODE(...)` → suggestion contains `CASE` (unchanged)
Add test for `IIF(cond, a, b)` → suggestion contains `CASE` (unchanged)

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_p009_nvl_coalesce`
Expected: FAIL (suggestion still says CASE for NVL)

### Task 3.2: Fix P009 implementation

**Files:**
- Modify: `src/linter/rules_performance.rs:CASE_REPLACEABLE_FUNCTIONS` (around L1058)
- Modify: `check_p009` function

**Step 3: Implement fix**
- In `CASE_REPLACEABLE_FUNCTIONS`, add a field for the replacement suggestion per function
- For `nvl`: suggestion = `COALESCE({args})`
- For `nvl2`: suggestion = `CASE WHEN {arg1} IS NOT NULL THEN {arg2} ELSE {arg3} END`
- For `decode`/`iif`: keep existing suggestion

**Step 4: Run test to verify it passes**

**Step 5: Run full gate**

---

## Batch 4: MCP Tool Enhancements (#306, #308, #309, #312)

### Task 4.1: Unify lint output between validate and parse (#306)

**Problem:** `validate` and `parse` tools return different lint_warning structures. `parse` is missing `rule_id`, `rule_name`, `suggestion`, `gaussdb_ref`, `confidence`.

**Files:**
- Modify: `src/mcp/` or `src/validation/` — shared LintWarning serialization

**Step 1: Write failing test**
Test that `parse` with `lint=true` returns lint warnings with all fields that `validate` returns.

**Step 2: Implement fix**
Create a shared `LintWarning` struct with all fields, used by both `validate` and `parse`. Ensure `parse` output includes all fields.

**Step 3: Run test to verify it passes**

**Step 4: Run full gate**

### Task 4.2: Add lint option to parse_java and parse_xml (#309)

**Files:**
- Modify: `src/mcp/` — parse_java and parse_xml tool handlers

**Step 1: Write failing test**
Test that `parse_java` with `lint=true` returns lint warnings on extracted SQL.

**Step 2: Implement fix**
- Add `lint` parameter (bool, default=false) to `parse_java` and `parse_xml`
- When `lint=true`, run linter on each extracted SQL statement
- Attach lint warnings to the `parse_result` of each statement
- Return `lint_summary` in the top-level response

**Step 3: Run test to verify it passes**

**Step 4: Run full gate**

### Task 4.3: Add summary mode to tokenize (#308)

**Files:**
- Modify: `src/mcp/` — tokenize tool handler

**Step 1: Write failing test**
Test that `tokenize` with `summary=true` returns summary stats instead of full token list.

**Step 2: Implement fix**
- Add `summary` parameter (bool, default=false)
- When `summary=true`, aggregate tokens by type and return `total_tokens`, `by_type`, `tables`, `columns`, `functions`, `has_subquery`, `has_join`

**Step 3: Run test to verify it passes**

**Step 4: Run full gate**

### Task 4.4: Clarify json2sql input JSON schema (#312)

**Files:**
- Modify: `docs/mcp/` — json2sql tool documentation

**Step 1: Update documentation**
Add clear JSON schema description for json2sql input:
- Format: complete parse output `{"statements": [...]}`
- Each statement: `sql_text` (optional), `statement` (required AST), `location` (optional)

**Step 2: Close issue #312**

---

## Batch 5: Dependencies — Upgrade tree-sitter (#280)

**Goal:** Upgrade `tree-sitter` from 0.24 to 0.25 and `tree-sitter-java` from 0.23 to 0.23.5 to resolve `links = "tree-sitter"` conflict with astgrep.

**Files:**
- Modify: `Cargo.toml` (lines 44-45)

**Step 1: Update Cargo.toml**
```toml
tree-sitter = { version = "0.25", optional = true }
tree-sitter-java = { version = "0.23.5", optional = true }
```

**Step 2: Check for API changes**
Run `cargo check --features java` to see if any API changes in tree-sitter 0.25 break compilation.

**Step 3: Fix any API breakage**
Tree-sitter 0.25 may have changed:
- `Parser::set_language` API
- `Language` struct fields
- `Query` API changes
- `TreeCursor` API changes

**Step 4: Run tests**
```bash
cargo test --features java
cargo test --features ibatis,java,cli
```

**Step 5: Run full gate**
```bash
cargo fmt --all && cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

---

## Batch 6: Major Feature — Batch Processing Interface (#307)

**Note:** This is a large feature with 3 phases. Only Phase 1 is recommended for this batch.

**Phase 1: File path parameter support**
- Add `file_path` parameter to `validate`, `parse`, `format` tools
- Tool reads file content internally
- Support `.sql`/`.pck`/`.fnc`/`.prc` extensions

**Files:**
- Modify: `src/mcp/` — tool handler for each affected tool

**Step 1: Write failing test**
Test that `validate(file_path="/path/to/file.sql")` works same as `validate(sql="...")`.

**Step 2: Implement file path support**
- Add `file_path: Option<String>` parameter to each tool
- When `file_path` is set, read file content and use as `sql` input
- Validate file exists and has readable content

**Step 3: Run test to verify it passes**

**Step 4: Run full gate**

---

## Execution Order

```
Day 1:                              Day 2:                        Day 3:
┌─────────────────────┐            ┌─────────────────────┐       ┌─────────────────────┐
│ Batch 1: #331       │            │ Batch 2: #311,#304, │       │ Batch 4: #306,#308, │
│ Batch 2.1: #311     │  ──parallel─▶│ #305                │       │ #309,#312           │
│ Batch 3: #314       │            │                     │       │ ─────────────────── │
│ Close #241, #320    │            │                     │       │ Batch 5: #280       │
└─────────────────────┘            └─────────────────────┘       │ Batch 6: #307(P1)   │
                                                                  └─────────────────────┘
```

**Dependencies:**
- #304 (PL/SQL formatting bug) → #305 (PL/SQL formatting options) — sequential
- All other batches are independent — can be parallelized

---

## Verification Checklist (per batch)

- [ ] New behavior has a failing → passing test
- [ ] Modified legacy paths have characterization tests
- [ ] No existing tests deleted, skipped, or modified
- [ ] `cargo fmt` + clippy + test all pass
- [ ] No debug output, drafts, or lockfile mass changes in commits