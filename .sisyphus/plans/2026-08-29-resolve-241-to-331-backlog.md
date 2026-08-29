# Resolve Issues #241–#331 Backlog

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Resolve the remaining open issues in the #241–#331 range across parser, formatter, linter, MCP tools, and dependencies.

**Pre-requisite — Close already-fixed issues on GitHub:**
- #241 — fixed by PR #242 (R009 downgrade + `docs/ecosystem-rule-mapping.md`)
- #306 — fixed (both `parse` and `validate` in `src/mcp/mod.rs` use the same `SqlWarning` struct from `src/linter/mod.rs:64-81` with all fields: `rule_id`, `rule_name`, `suggestion`, `gaussdb_ref`, `confidence`)
- #320 — fixed by PR #321 (inline DDL in PL bodies → structured AST)
- #331 — fixed by PR #330 (qualified CREATE AGGREGATE names; test `test_create_aggregate_qualified_name` in `src/parser/tests.rs:8625` passes)

**Architecture:** 6 remaining issues grouped into 4 domain-focused batches, each isolated to a single subsystem. Batches are independent and can be parallelized. Each batch follows TDD cycle (Red → Green → Refactor) per AGENTS.md.

**Tech Stack:** Rust (single crate), tree-sitter 0.24, PL/pgSQL parser, MCP server (json2sql/tokenize/validate/parse)

---

## Batch 1: Formatter Fixes (#311, #304, #305)

### Task 1.1: Fix keyword_case incorrectly uppercasing identifiers (#311)

**Bug:** `keyword_case=upper` converts `a.name` to `a.NAME` because `name` is treated as a keyword instead of an identifier.

**File:** `src/token_formatter.rs` — the `FormatConfig::format_statement` or equivalent function that applies `keyword_case`.

**QA verification:**
```bash
cargo test --all-features test_keyword_case_preserves_identifiers
# Expected: PASS
```

**Step 1: Write failing test (Red)**
Add test in `src/token_formatter.rs` (or `src/parser/tests.rs`) that:
- `keyword_case=upper` → `SELECT a.id, a.name FROM users` stays `SELECT a.id, a.name FROM users` (identifiers unchanged)
- `keyword_case=upper` → `select a.id, a.name from users` → `SELECT a.id, a.name FROM users` (keywords uppercased, idents lowercase)
- `keyword_case=lower` → `SELECT a.id, a.NAME FROM users` → `select a.id, a.NAME FROM users` (keywords lowercased, idents uppercase preserved)

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_keyword_case_preserves_identifiers`
Expected: FAIL (identifiers incorrectly uppercased)

**Step 3: Fix implementation (Green)**
The root cause: `keyword_case` implementation operates on token text without distinguishing `Keyword` vs `Ident` token types. Fix:
- In the `keyword_case` formatting pass, check `Token::token_type` before applying case transformation
- Only apply case change to tokens where `token_type` is a keyword
- Leave tokens where `token_type` is an identifier unchanged
- Edge case: some tokens are contextually ambiguous (e.g., `name` can be a keyword) — rely on the tokenizer's classification

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_keyword_case_preserves_identifiers`
Expected: PASS

**Step 5: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

### Task 1.2: Improve PL/SQL formatting for PACKAGE/PROCEDURE/FUNCTION bodies (#304)

**Problems:**
1. Output first line has leading whitespace
2. PACKAGE inner PROCEDURE/FUNCTION has no indentation
3. No blank line between PROCEDURE/FUNCTION definitions
4. `RETURN` missing preceding space in `FUNCTION ... RETURN VARCHAR2`

**File:** `src/formatter/` — PL/SQL formatting logic (likely `src/formatter/plpgsql.rs` or `src/token_formatter.rs`).

**QA verification:**
```bash
cargo test --all-features test_plsql_package_formatting
# Expected: PASS
```

**Step 1: Write failing test (Red)**
Add test(s) in `src/parser/tests.rs` or `src/formatter/` test module:
- Input: `CREATE OR REPLACE PACKAGE BODY test_pkg IS PROCEDURE do_something(p_id IN NUMBER) IS BEGIN INSERT INTO log_table VALUES(p_id, now()); COMMIT; END; END test_pkg;`
- Output first line has no leading whitespace
- PROCEDURE is indented inside PACKAGE BODY
- `RETURN` has preceding space in `FUNCTION get_name(p_id IN NUMBER) RETURN VARCHAR2`

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_plsql_package_formatting`
Expected: FAIL

**Step 3: Fix implementation (Green)**
- Strip leading whitespace from rendered output
- Track nesting depth in PL/SQL block structure (PACKAGE → PROCEDURE → BEGIN → END)
- Add indentation per nesting level
- Insert blank lines between PROCEDURE/FUNCTION definitions
- Add space before `RETURN` keyword in function signatures

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_plsql_package_formatting`
Expected: PASS

**Step 5: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

### Task 1.3: Add PL/SQL-specific formatting options (#305)

**Note:** This is a feature enhancement that builds on (#304). Implement Phase 1 only (default reasonable indentation) — the option parameters (Phase 2) can be deferred.

**File:** `src/token_formatter.rs` — `FormatConfig` struct + `format_statement`.

**QA verification:**
```bash
cargo test --all-features test_plsql_formatting_options
# Expected: PASS
```

**Step 1: Write failing test (Red)**
Test that with default settings, PL/SQL blocks are formatted with proper indentation (DECLARE/BEGIN/EXCEPTION/END nested correctly).

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_plsql_formatting_options`
Expected: FAIL

**Step 3: Implement default PL/SQL formatting (Green)**
- Add `block_indent: usize` field to `FormatConfig` (default = existing `indent_width`)
- Ensure DECLARE/BEGIN/EXCEPTION/END block indentation works
- Add `procedure_newline: bool` field (default = true)

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_plsql_formatting_options`
Expected: PASS

**Step 5: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

---

## Batch 2: Linter Improvement (#314)

**Goal:** `P009` should suggest `COALESCE(a, b)` for `NVL(a, b)` instead of `CASE WHEN a IS NOT NULL THEN a ELSE b END`.

**Context:** Per comment on #314 (c2j), P009 currently only triggers in WHERE context, not in SELECT list. The issue's "current behavior" claim is inaccurate. Fix should:
1. For `NVL(a, b)`: suggest `COALESCE(a, b)` 
2. For `NVL2(a, b, c)`: suggest `CASE WHEN a IS NOT NULL THEN b ELSE c END`
3. For `DECODE`/`IIF`: keep existing CASE suggestion

**Files:**
- Modify: `src/linter/rules_performance.rs` — `CASE_REPLACEABLE_FUNCTIONS` constant (around L1058) and `check_p009` function

**QA verification:**
```bash
cargo test --all-features test_p009_nvl_coalesce
# Expected: PASS
```

**Step 1: Write failing test (Red)**
Add test in `src/linter/tests.rs` or `src/linter/rules_performance.rs`:
- `NVL(a, b)` → suggestion contains `COALESCE`
- `NVL2(a, b, c)` → suggestion contains `CASE WHEN`
- `DECODE(...)` → suggestion contains `CASE` (unchanged)
- `IIF(cond, a, b)` → suggestion contains `CASE` (unchanged)

**Step 2: Run test to verify it fails**
Run: `cargo test --all-features test_p009_nvl_coalesce`
Expected: FAIL (suggestion still says CASE for NVL)

**Step 3: Implement fix (Green)**
- In `CASE_REPLACEABLE_FUNCTIONS` (or equivalent), add per-function replacement suggestion logic
- For `nvl`: suggestion = `COALESCE({args})`
- For `nvl2`: suggestion = `CASE WHEN {arg1} IS NOT NULL THEN {arg2} ELSE {arg3} END`
- For `decode`/`iif`: keep existing suggestion

**Step 4: Run test to verify it passes**
Run: `cargo test --all-features test_p009_nvl_coalesce`
Expected: PASS

**Step 5: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

---

## Batch 3: MCP Tool Enhancements (#308, #309, #312)

### Task 3.1: Add lint option to parse_java and parse_xml (#309)

**Files:**
- Modify: `src/mcp/` — parse_java and parse_xml tool handlers

**QA verification:**
```bash
cargo test --all-features test_parse_java_lint_option
# Expected: PASS
```

**Step 1: Write failing test**
Test that `parse_java` with `lint=true` returns lint warnings on extracted SQL.

**Step 2: Implement fix**
- Add `lint: bool` parameter (default=false) to `parse_java` and `parse_xml` MCP tool definitions
- When `lint=true`, run linter on each extracted SQL statement
- Attach lint warnings to the `parse_result` of each statement
- Return `lint_summary` in the top-level response

**Step 3: Run test to verify it passes**

**Step 4: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

### Task 3.2: Add summary mode to tokenize (#308)

**Files:**
- Modify: `src/mcp/` — tokenize tool handler

**QA verification:**
```bash
cargo test --all-features test_tokenize_summary_mode
# Expected: PASS
```

**Step 1: Write failing test**
Test that `tokenize` with `summary=true` returns summary stats instead of full token list.

**Step 2: Implement fix**
- Add `summary: bool` parameter (default=false) to tokenize tool
- When `summary=true`, aggregate tokens by type and return:
  - `total_tokens`, `by_type` (count per token type), `tables`, `columns`, `functions`, `has_subquery`, `has_join`

**Step 3: Run test to verify it passes**

**Step 4: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

### Task 3.3: Clarify json2sql input JSON schema (#312)

**File:** `docs/user-guide.md` — find the `json2sql` section and clarify the input schema.

**QA verification:**
```bash
grep -A 20 "json2sql" docs/user-guide.md | head -30
# Expected: Output contains the JSON schema description with:
# 1. "Input format: complete parse output `{ \"statements\": [...] }`"
# 2. "sql_text: string (optional, for round-trip verification)"
# 3. "statement: object (required, AST node)"
# 4. "location: object (optional, line/column info)"
# 5. A JSON example showing the expected structure
```

**Step 1: Add JSON schema documentation to `docs/user-guide.md`**
- Find the `json2sql` section in `docs/user-guide.md`
- Add a "Input JSON Schema" subsection with:
  - Clear description of the expected format: complete parse output `{"statements": [...]}`
  - Per-statement field documentation:
    - `sql_text`: string (optional, for round-trip verification)
    - `statement`: object (required, AST node)
    - `location`: object (optional, line/column info)
  - A JSON example showing the structure

**Step 2: Verify the change**
Run: `grep -A 20 "json2sql" docs/user-guide.md` and confirm the schema section appears with the expected fields.

**Step 3: Close issue #312**

---

## Batch 4: Dependencies — Upgrade tree-sitter (#280)

**Goal:** Upgrade `tree-sitter` from 0.24 to 0.25 and `tree-sitter-java` from 0.23 to 0.23.5 to resolve `links = "tree-sitter"` conflict with astgrep (c2j/astgrep uses tree-sitter 0.25).

**Current versions (from Cargo.toml):**
```
tree-sitter = { version = "0.24", optional = true }
tree-sitter-java = { version = "0.23", optional = true }
```

**File:** `Cargo.toml` (lines 44-45)

**QA verification:**
```bash
cargo test --features java
cargo test --features ibatis,java,cli
cargo test --all-features
# All expected: PASS
```

**Step 1: Update Cargo.toml**
```toml
tree-sitter = { version = "0.25", optional = true }
tree-sitter-java = { version = "0.23.5", optional = true }
```

**Step 2: Check for API changes**
Run: `cargo check --features java`
Expected: Either PASS or compile errors from API changes

**Step 3: Fix any API breakage**
Tree-sitter 0.25 may have changed:
- `Parser::set_language` API signature
- `Language` struct fields
- `Query` API
- `TreeCursor` API

Search for tree-sitter API usage in `src/java/` to find affected code.

**Step 4: Run tests**
```bash
cargo test --features java
cargo test --features ibatis,java,cli
```

**Step 5: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

---

## Batch 5: Major Feature — Batch Processing Interface (#307, Phase 1)

**Note:** This is a large feature with 3 phases. Only Phase 1 is recommended for this batch.

**Phase 1: File path parameter support**
- Add `file_path` parameter to `validate`, `parse`, `format` MCP tools
- Tool reads file content internally
- Support `.sql`/`.pck`/`.fnc`/`.prc` extensions

**Files:**
- Modify: `src/mcp/` — tool handler for each affected tool (validate, parse, format)

**QA verification:**
```bash
cargo test --all-features test_mcp_file_path_parameter
# Expected: PASS
```

**Step 1: Write failing test**
Test that `validate(file_path="/path/to/file.sql")` works same as `validate(sql="...")`.

**Step 2: Implement file path support**
- Add `file_path: Option<String>` parameter to each tool's input struct
- When `file_path` is set (and `sql` is not), read file content and use as `sql` input
- Validate file exists and has readable content
- Error if both `file_path` and `sql` are provided, or neither

**Step 3: Run test to verify it passes**

**Step 4: Full gate verification**
```bash
cargo test --all-features
# Expected: All tests pass
```

---

## Execution Order

```
Day 1 (parallel):
┌──────────────────────┐  ┌──────────────────────┐  ┌──────────────────────┐
│ Batch 1: Formatter   │  │ Batch 2: Linter       │  │ Batch 4: Dependencies│
│   #311, #304, #305   │  │   #314                │  │   #280               │
│ (sequential: 1→2→3)  │  │ (standalone)          │  │ (standalone)         │
└──────────────────────┘  └──────────────────────┘  └──────────────────────┘

Day 2 (parallel):
┌──────────────────────┐  ┌──────────────────────┐
│ Batch 3: MCP Tools   │  │ Batch 5: Batch MCP   │
│   #308, #309, #312   │  │   #307 Phase 1       │
│ (all independent)    │  │ (standalone)         │
└──────────────────────┘  └──────────────────────┘

Close already-fixed issues:
  #241, #306, #320, #331 — now, before any implementation (just close on GitHub, no code changes)
```

**Dependencies:**
- #304 → #305 sequential (formatting bug fix before options feature)
- All other batches are independent — can be parallelized

---

## Verification Checklist (per batch)

- [ ] New behavior has a failing → passing test
- [ ] Modified legacy paths have characterization tests
- [ ] No existing tests deleted, skipped, or modified
- [ ] `cargo fmt --all && cargo fmt --all -- --check` — clean
- [ ] `cargo clippy --all-features -- -D warnings` — 0 warnings
- [ ] `cargo test --all-features` — all pass
- [ ] No debug output, drafts, or lockfile mass changes in commits