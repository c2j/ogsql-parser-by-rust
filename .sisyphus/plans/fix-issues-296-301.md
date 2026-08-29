# Plan: Fix linter issues #296–#301

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the linter PL/SQL traversal gap (#296) and add five new lint rules: P024 ROWNUM (#297), C019 commit-inside-loop (#298), R013 implicit-join (#299), P025 positive-IN-subquery (#300), S009 tautological-condition (#301).

**Architecture:** The linter (`src/linter/`) registers rules as `LintRuleEntry { id, name, level, stmt_kind, check_fn }` in four files (`rules_prohibition.rs`, `rules_performance.rs`, `rules_caution.rs`, `rules_suggestion.rs`). Dispatch is `SqlLinter::lint()` → `classify_statement()` → `StatementKind::matches()` → `check_fn`. PL-body traversal is split between `collect_selects_from_stmt` (extracts embedded SELECTs for Dml rules) and the `walk_pl_for_*` entry gates (7 sites) that feed the `check_pl_stmts_for_*` recursive walkers. #296 is fixed by (a) reclassifying the 3 CREATE variants to `PlBlock`, (b) introducing a shared `pl_blocks_from_stmt()` helper, (c) routing all 7 walk gates + `collect_selects_from_stmt` through it, and (d) widening R001/R005/R006/R007 dispatch + traversal. The 5 new rules clone existing reference implementations (P002→P025, P013→R013, C014+P021→C019, P015→S009, walk_expr→P024).

**Tech Stack:** Rust, hand-written recursive-descent parser, serde AST, `cargo test --all-features` (CI: fmt + clippy + test + audit per AGENTS.md).

**Branch:** `feat/issue-296_to_301` (current worktree, HEAD `c85e6cc`)
**Base:** v0.10.1

---

## Verified Current State (empirical, against HEAD)

| Issue | Reproduction | HEAD result | Verdict |
|---|---|---|---|
| #296 | `validate(lint=true, sql="CREATE FUNCTION bad_func ... SELECT * INTO ... EXECUTE IMMEDIATE ... WHEN OTHERS ... END")` | 0 warnings | ❌ needs fix |
| #297 | `validate(lint=true, sql="SELECT id FROM users WHERE ROWNUM <= 10")` | 0 warnings | ❌ needs rule |
| #298 | `validate(lint=true, sql="DO $$ BEGIN LOOP COMMIT; END LOOP; END $$")` | C014 fires (any-commit), no loop-specific | ❌ needs rule |
| #299 | `validate(lint=true, sql="SELECT a.id, b.name FROM a, b WHERE a.id = b.id")` | 0 warnings | ❌ needs rule |
| #300 | `validate(lint=true, sql="SELECT * FROM a WHERE id IN (SELECT id FROM b)")` | 0 warnings | ❌ needs rule |
| #301 | `validate(lint=true, sql="SELECT * FROM users WHERE 1=1 AND status = 'A'")` | 0 warnings | ❌ needs rule |

Key facts established (all verified by reading source at HEAD):
- **#296 root cause is two disjoint gaps:** (1) `classify_statement` maps `CreateFunction`(mod.rs:421)/`CreateProcedure`(mod.rs:422)/`CreatePackageBody`(mod.rs:536) → `Ddl`, so Dml-gated rules (R001/R005/R006/R007) AND PlBlock-gated rules (C012/C013/C014/C016/C017/P021/S005) never dispatch on them (`StatementKind::matches` at mod.rs:99-114: `(Dml,Ddl)` and `(PlBlock,Ddl)` are both `false`). (2) Even if dispatched, `collect_selects_from_stmt` (mod.rs:783-837) has no arms for the 3 CREATE variants (`_ => {}` at L835), and all 7 `walk_pl_for_*` gates match only `AnonyBlock`/`Do`.
- **Only ONE rule gates on `stmt_kind: StatementKind::Ddl`: C010 `unlogged-table`** (rules_caution.rs:64). Its check function `check_c010` (rules_caution.rs:451-488) matches ONLY `Statement::CreateTable(s) if s.unlogged` and `Statement::CreateTableAs(s) if s.unlogged` — **it never matches CreateFunction/CreateProcedure/CreatePackageBody**, so reclassifying the 3 CREATE variants to `PlBlock` cannot affect C010. All other rules use Dml/Select/Update/Delete/Merge/Insert/All/PlBlock. → Reclassification is safe; C010 is a known unrelated exception.
- **R010 is the working precedent:** `stmt_kind: All` + manual `func.block`/`pkg_body.items[*].block` unwrapping (rules_prohibition.rs:685-716, 770-814). The parser already populates `block: Option<PlBlock>` for all 3 CREATE variants (ast/mod.rs:2052-2059, 2062-2068, 2085-2128). No parser/AST changes needed.
- **Existing R010 tests already parse CREATE FUNCTION/PROCEDURE bodies** (tests.rs:1395-1540) — a ready-made harness; none assert R001/R005/R006/R007 stay silent inside bodies.
- **AST readiness:** ROWNUM → `Expr::ColumnRef(["rownum"])` (parser/expr.rs:1116-1130; parser/tests.rs:13208). Commit/Rollback → `PlStatement::Commit { and_chain }`/`Rollback { to_savepoint, and_chain }` (ast/plpgsql.rs:176-184); loops → `PlStatement::{Loop,While,For,ForEach}` with `body: Vec<PlStatement>` (plpgsql.rs:126-129, 264-311). Comma-join → `SelectStatement.from: Vec<TableRef>` with >1 top-level elements (ast/mod.rs:1016; parser/select.rs:440-457); explicit JOIN folds into single `TableRef::Join` (parser/select.rs:586-682). IN-subquery → `Expr::InSubquery { expr, subquery, negated }` (ast/mod.rs:1252-1256). Tautology → `Expr::BinaryOp { op: String, left, right }` (ast/mod.rs:1199-1203) + `literals_equal` (rules_performance.rs:900-905).
- **Docs counts are stale:** README/user-guide say 53/54 rules; code registers 56 entries. After this plan: 61. Recompute from `all_rules_metadata().len()` rather than trusting docs.

---

## Task 1 — #296A: Shared `pl_blocks_from_stmt` helper (foundation)

**Files:**
- Modify: `src/linter/mod.rs` (add helper near `collect_selects_from_pl_block` at L1093)

**Step 1: Write the helper** (after `collect_selects_from_pl_block`, ~L1110):

```rust
/// Return all PL/pgSQL blocks reachable from a statement's PL body:
/// AnonyBlock/Do (as today), plus CREATE FUNCTION / PROCEDURE / PACKAGE BODY.
pub(crate) fn pl_blocks_from_stmt<'a>(stmt: &'a Statement) -> Vec<&'a crate::ast::plpgsql::PlBlock> {
    use crate::ast::PackageItem;
    match stmt {
        Statement::AnonyBlock(b) => vec![&b.block],
        Statement::Do(d) => d.block.iter().collect(),
        Statement::CreateFunction(f) => f.block.iter().collect(),
        Statement::CreateProcedure(p) => p.block.iter().collect(),
        Statement::CreatePackageBody(pkg) => pkg
            .items
            .iter()
            .filter_map(|item| match item {
                PackageItem::Function(f) => f.block.as_ref(),
                PackageItem::Procedure(p) => p.block.as_ref(),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: PASS (helper is `pub(crate)`, dead-code warning may appear until Task 1C wires it — add `#[allow(dead_code)]` temporarily or wire in same task; prefer wiring immediately).

**Step 3: Commit**

```bash
git add src/linter/mod.rs
git commit -m "feat(linter): add pl_blocks_from_stmt helper for PL body traversal (#296)"
```

> Note: if the plan executor prefers atomicity, combine Tasks 1A–1C into one commit; the helper is useless until wired.

---

## Task 2 — #296B: Reclassify CREATE variants to `PlBlock`

**Files:**
- Modify: `src/linter/mod.rs:421-422, 536` (in `classify_statement`)

**Step 1: Remove the 3 variants from the Ddl arm** — delete `Statement::CreateFunction(_)` (L421) and `Statement::CreateProcedure(_)` (L422) from the big `|`-chain, and `Statement::CreatePackageBody(_)` (L536) (also delete the now-orphaned `Statement::RemovePackage(_)`? — NO: `RemovePackage` stays Ddl; only the 3 named variants move).

**Step 2: Add a dedicated arm** before `Statement::Truncate(_)`:

```rust
Statement::CreateFunction(_) | Statement::CreateProcedure(_) | Statement::CreatePackageBody(_) => {
    StatementKind::PlBlock
}
```

**Step 3: Verify no OTHER rule gates on `Ddl` for the reclassified variants**

Run: `grep -rn "StatementKind::Ddl" src/linter/`
Expected: EXACTLY ONE match in rule entries — C010 at `rules_caution.rs:64`. That is the known, safe exception: `check_c010` (rules_caution.rs:451-488) only matches `CreateTable`/`CreateTableAs` with `unlogged`, never the 3 reclassified variants. If any OTHER rule entry uses `Ddl`, stop and reassess.

**Step 4: Run full linter tests**

Run: `cargo test --lib linter`
Expected: PASS — existing tests must not regress (R010 tests at tests.rs:1395-1540 are the sensitive ones: they lint CREATE FUNCTION statements and assert R010 behavior; reclassification must not change R010 results since R010 is `stmt_kind: All`).

**Step 5: Commit**

```bash
git add src/linter/mod.rs
git commit -m "fix(linter): classify CREATE FUNCTION/PROCEDURE/PACKAGE BODY as PlBlock (#296)"
```

---

## Task 3 — #296C: Route all 7 `walk_pl_for_*` gates through the helper

**Files:**
- Modify: `src/linter/rules_caution.rs` — C012 (L609), C013 (L709), C014 (L805), C016 (L922), C017 (L996)
- Modify: `src/linter/rules_performance.rs` — P021 (L1123)
- Modify: `src/linter/rules_suggestion.rs` — S005 (L151)
- Modify: `src/linter/mod.rs` — export helper (already `pub(crate)`; add to `use` lists in the 3 rule files)

**Step 1: Rewrite one gate as the template** — C014 (rules_caution.rs:805-818), current shape:

```rust
fn walk_pl_for_commit_rollback(stmt: &Statement, found: &mut bool) {
    let block = match stmt {
        Statement::AnonyBlock(b) => &b.block,
        Statement::Do(d) => {
            if let Some(ref block) = d.block { block } else { return; }
        }
        _ => return,
    };
    check_pl_stmts_for_commit_rollback(&block.body, found);
}
```

Replace with:

```rust
fn walk_pl_for_commit_rollback(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_stmts_for_commit_rollback(&block.body, found);
        if *found {
            return;
        }
    }
}
```

**Step 2: Apply the same transformation to the other 6 gates:**
- `walk_pl_for_execute_concat` (rules_caution.rs:609-622) → body `check_pl_stmts_for_execute_concat(&block.body, found)`
- `walk_pl_for_exception_swallow` (rules_caution.rs:709-722) → body `check_block_for_swallow(block, found)` — NOTE: this one takes `&PlBlock`, pass `block` directly
- `walk_pl_for_autonomous` (rules_caution.rs:922-938) → body `check_block_for_autonomous(block, found)` (verify exact helper name at implementation time)
- `walk_pl_for_raise_in_exception` (rules_caution.rs:996-1009) → body `check_pl_stmts_for_raise_in_exception(&block.body, found)` (verify name)
- `walk_pl_for_loop_insert` (rules_performance.rs:1123-1136) → body `check_pl_stmts_for_loop_insert(&block.body, found, false)`
- `walk_pl_for_type_name` (rules_suggestion.rs:151-167) → body checks declarations + body: `check_decls_for_type_name(&block.declarations, found); if !*found { check_pl_stmts_for_type_name(&block.body, found); }`

Each gate becomes a `for block in crate::linter::pl_blocks_from_stmt(stmt) { ...; if *found { return; } }` loop. Keep the exact `check_pl_stmts_for_*`/`check_block_for_*` call signatures from the current bodies — only the entry `match` is replaced.

**Step 3: Import the helper** — add `pl_blocks_from_stmt` to the `use crate::linter::{...}` lists in the 3 rule files (or use the fully-qualified path in Step 1/2 — pick one style, prefer adding to the existing `use` block).

**Step 4: Grep for any remaining PL gates**

Run: `grep -rn "Statement::AnonyBlock(b) => &b.block" src/linter/`
Expected: NO remaining matches (all 7 converted). Also grep `fn walk_pl_for` to confirm exactly 7 gates, all converted.

**Step 5: Tests**

Add tests in `src/linter/tests.rs` (C-series section, near existing C014 tests ~L774):

```rust
#[test]
fn c014_commit_in_function_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN COMMIT; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C014"), "COMMIT inside CREATE FUNCTION body should trigger C014");
}

#[test]
fn c012_concat_in_procedure_warns() {
    let stmts = parse(
        "CREATE PROCEDURE p() LANGUAGE plpgsql AS $$ BEGIN EXECUTE IMMEDIATE 'SELECT * FROM ' || v_t; END; $$",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "C012"), "EXECUTE IMMEDIATE concat inside procedure should trigger C012");
}

#[test]
fn c013_swallow_in_package_body_warns() {
    let stmts = parse(
        "CREATE OR REPLACE PACKAGE BODY pkg AS
            FUNCTION f RETURN VARCHAR2 IS v VARCHAR2;
            BEGIN
                NULL;
            EXCEPTION WHEN OTHERS THEN RETURN NULL;
            END;
        END pkg;",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "C013"), "WHEN OTHERS swallow inside package body function should trigger C013");
}
```

Run: `cargo test --lib linter`
Expected: new tests PASS (also existing `p021_*`/`c012_*`/`c013_*`/`c014_*`/`s005_*` tests on DO blocks still PASS).

**Step 6: Commit**

```bash
git add src/linter/rules_caution.rs src/linter/rules_performance.rs src/linter/rules_suggestion.rs src/linter/tests.rs
git commit -m "fix(linter): traverse CREATE FUNCTION/PROCEDURE/PACKAGE BODY in all PL walk rules (#296)"
```

---

## Task 4 — #296D: Extend `collect_selects_from_stmt` for the 3 CREATE variants

**Files:**
- Modify: `src/linter/mod.rs:783-837`

**Step 1: Add arms** before `_ => {}` (L835):

```rust
Statement::CreateFunction(f) => {
    if let Some(ref block) = f.block {
        collect_selects_from_pl_block(block, out);
    }
}
Statement::CreateProcedure(p) => {
    if let Some(ref block) = p.block {
        collect_selects_from_pl_block(block, out);
    }
}
Statement::CreatePackageBody(pkg) => {
    for block in pl_blocks_from_stmt(stmt) {
        collect_selects_from_pl_block(block, out);
    }
}
```

(`pl_blocks_from_stmt` returns the per-item blocks for package bodies; the first two arms could also use the helper for uniformity — use `for block in pl_blocks_from_stmt(stmt)` for all three.)

**Step 2: Test — R001 now fires inside bodies**

Add in `src/linter/tests.rs` (R001 section, ~L60):

```rust
#[test]
fn r001_select_star_inside_function_warns() {
    // Regression for #296: SELECT * inside CREATE FUNCTION body must warn.
    let stmts = parse(
        "CREATE OR REPLACE FUNCTION bad_func RETURN VARCHAR2 IS
            v_name VARCHAR2;
         BEGIN
            SELECT * INTO v_name FROM users WHERE id = 1;
            RETURN v_name;
         END;",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "SELECT * inside function body should trigger R001");
}
```

Run: `cargo test --lib linter`
Expected: FAIL — R001 is still `stmt_kind: Dml` and `CreateFunction` now classifies as `PlBlock`, so R001 does NOT dispatch on it yet. **This failing test is the TDD anchor for Task 5.** (Alternatively combine this test with Task 5.)

**Step 3: Commit**

```bash
git add src/linter/mod.rs src/linter/tests.rs
git commit -m "feat(linter): collect embedded SELECTs from CREATE FUNCTION/PROCEDURE/PACKAGE BODY (#296)"
```

> Note: after this commit the test from Step 2 fails (red). That is intentional and temporary — Task 5 turns it green. If the executor prefers never-committing-red, fold Step 2's test into Task 5's commit.

---

## Task 5 — #296E: Widen R001/R005/R006/R007 to run on PL-block statements

**Files:**
- Modify: `src/linter/rules_prohibition.rs`
  - R001 registration L22: `stmt_kind: StatementKind::Dml` → `StatementKind::All`
  - R005 registration L54, R006 registration L62, R007 registration L70: same change
  - `check_r001` L122-177: unchanged logic — `collect_selects_from_stmt` now reaches embedded SELECTs (Task 4), and `has_outermost` (L138-146) is `false` for `CreateFunction`/`CreateProcedure`/`CreatePackageBody` → all embedded SELECT * warn as inner. Already correct.
  - `check_r005` L280-351: keep the non-S/U/D fallback path (L300-303). `extract_where_clause` (L1181) currently returns `None` for CREATE/PL statements. **Extend it** to walk embedded SELECTs:

**Step 1a: Export `collect_selects_from_pl_block`** — it is currently a private `fn` at `src/linter/mod.rs:1093`. Change `fn collect_selects_from_pl_block<'a>(` → `pub(crate) fn collect_selects_from_pl_block<'a>(`. (It is only used within mod.rs today; exporting is safe and required for cross-module reuse in Task 5's helper.)

```rust
fn extract_where_clause(stmt: &Statement) -> Option<&Expr> {
    match stmt {
        Statement::Select(s) => s.where_clause.as_ref(),
        Statement::Update(s) => s.where_clause.as_ref(),
        Statement::Delete(s) => s.where_clause.as_ref(),
        // #296: check WHERE of embedded SELECTs inside PL bodies / CREATE bodies
        Statement::AnonyBlock(b) => first_embedded_where(&b.block),
        Statement::Do(d) => d.block.as_ref().and_then(first_embedded_where),
        Statement::CreateFunction(f) => f.block.as_ref().and_then(first_embedded_where),
        Statement::CreateProcedure(p) => p.block.as_ref().and_then(first_embedded_where),
        Statement::CreatePackageBody(pkg) => crate::linter::pl_blocks_from_stmt(stmt)
            .into_iter()
            .find_map(first_embedded_where),
        _ => None,
    }
}
```

with a tiny helper (place next to it):

```rust
fn first_embedded_where(block: &PlBlock) -> Option<&Expr> {
    let mut selects: Vec<(&SelectStatement, crate::token::SourceLocation)> = Vec::new();
    crate::linter::collect_selects_from_pl_block(block, &mut selects);
    selects.iter().find_map(|(s, _)| s.where_clause.as_ref())
}
```

  - `where_and_tables` (L520-527) used by R006/R007: mirror the same extension — return `(first_embedded_where(block), &[])` for the PL/CREATE arms (tables empty: embedded SELECTs' FROM lists are not aggregated; acceptable limitation for R006/R007 which primarily need the WHERE expr; document in a comment).

**Step 2: Verify R001 semantics preserved**

Run: `cargo test --lib linter`
Expected:
- The Task 4 red test `r001_select_star_inside_function_warns` now PASSES.
- All existing `r001_*` tests (tests.rs:33-60) still PASS — especially `r001_outer_with_inner_allowed` (L41: `SELECT * FROM (SELECT id,name ...)` must remain allowed) and `r001_from_subquery_star_flagged` (L49).
- Existing `r005_*`/`r006_*`/`r007_*` tests still PASS (schema-aware tests may need `lint` with schema — see tests.rs for the schema-carrying helper; verify the `lint` helper at tests.rs:9 passes `None` schema, so R005 returns early without schema: fine, no regression).

**Step 3: Add regression tests for the issue's exact scenarios**

```rust
#[test]
fn r005_r006_r007_inside_function_warn() {
    let stmts = parse(
        "CREATE FUNCTION bad_func RETURN VARCHAR2 IS v_name VARCHAR2;
         BEGIN
            SELECT v_name INTO v_name FROM users WHERE LEFT(name, 3) = 'abc' AND name LIKE '%abc' AND status = 1;
            RETURN v_name;
         END;",
    );
    let w = lint(&stmts);
    // R007: LIKE with leading wildcard '%abc'
    // R006: LEFT(name,3) function on column
    assert!(has_rule(&w, "R006"), "function on WHERE column inside function body should trigger R006");
    assert!(has_rule(&w, "R007"), "LIKE leading wildcard inside function body should trigger R007");
}
```

Run: `cargo test --lib linter`
Expected: PASS (R006/R007 fire via the extended `where_and_tables`; R005 requires schema → skip if no schema, that's existing behavior).

**Step 4: Commit**

```bash
git add src/linter/rules_prohibition.rs src/linter/tests.rs
git commit -m "fix(linter): run R001/R005/R006/R007 on embedded SQL inside PL bodies (#296)"
```

---

## Task 6 — #296F: Full-issue acceptance test

**Files:**
- Modify: `src/linter/tests.rs`

**Step 1: Add the issue's exact Test B scenario**

```rust
#[test]
fn issue296_full_function_body_lints() {
    // Exact scenario from issue #296 Test B.
    let stmts = parse(
        "CREATE OR REPLACE FUNCTION bad_func RETURN VARCHAR2 IS
            v_name VARCHAR2;
         BEGIN
            SELECT * INTO v_name FROM users WHERE id = 1;
            EXECUTE IMMEDIATE 'DELETE FROM log WHERE id=' || v_name;
            RETURN v_name;
         EXCEPTION
            WHEN OTHERS THEN RETURN NULL;
         END;",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "SELECT * inside function should trigger R001");
    assert!(has_rule(&w, "C012"), "EXECUTE IMMEDIATE concat should trigger C012");
    assert!(has_rule(&w, "C013"), "WHEN OTHERS swallow should trigger C013");
}
```

**Step 2: Verify against the issue's Test A (unchanged behavior)**

```rust
#[test]
fn issue296_plain_dml_still_lints() {
    let stmts = parse("SELECT * FROM users WHERE DECODE(status, A, 1, 0) = 1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "plain SELECT * still triggers R001");
    assert!(has_rule(&w, "P009"), "DECODE still triggers P009");
}
```

Run: `cargo test --lib linter`
Expected: all PASS.

**Step 3: Commit**

```bash
git add src/linter/tests.rs
git commit -m "test(linter): add issue #296 acceptance tests"
```

---

## Task 7 — #297: P024 `rownum-pagination`

**Files:**
- Modify: `src/linter/rules_performance.rs` (register at L9-202; new check fn after P023 at L1224)
- Modify: `src/linter/tests.rs`

**Step 1: Register** (append to the `rules` vec in `register()`):

```rust
LintRuleEntry {
    id: "P024",
    name: "rownum-pagination",
    description: "ROWNUM is an Oracle-only pagination pseudo-column; use LIMIT/OFFSET or FETCH FIRST",
    level: WarningLevel::Performance,
    // All: dispatches on CreateFunction/CreateProcedure/CreatePackageBody (PlBlock
    // after Task 2) AND plain DML, so ROWNUM inside PL bodies is also detected.
    stmt_kind: StatementKind::All,
    check_fn: check_p024,
},
```

**Step 2: Implement** (after `check_p023`, end of file):

```rust
// P024: ROWNUM pseudo-column (Oracle pagination) → LIMIT/OFFSET or FETCH FIRST
fn check_p024(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let mut selects: Vec<(&SelectStatement, SourceLocation)> = Vec::new();
    collect_selects_from_stmt(&curr_stmt.statement, loc, &mut selects);
    for (s, _) in selects {
        let mut found = false;
        // ROWNUM in WHERE clause
        if let Some(ref w) = s.where_clause {
            walk_expr(w, &mut |e| {
                if is_rownum_ref(e) {
                    found = true;
                    false
                } else {
                    true
                }
            });
        }
        // ROWNUM in SELECT target list
        if !found {
            for t in &s.targets {
                if let SelectTarget::Expr(e, _) = t {
                    walk_expr(e, &mut |e| {
                        if is_rownum_ref(e) {
                            found = true;
                            false
                        } else {
                            true
                        }
                    });
                    if found {
                        break;
                    }
                }
            }
        }
        if found {
            warnings.push(make_warning(
                WarningLevel::Performance,
                "P024",
                "rownum-pagination",
                "ROWNUM \u{4e3a} Oracle \u{4e13\u{6709}\u{5206}\u{9875}\u{8bed}\u{6cd5}".into(),
                Some("\u{5efa}\u{8bae}\u{4f7f}\u{7528} LIMIT/OFFSET \u{6216} FETCH FIRST"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

fn is_rownum_ref(e: &Expr) -> bool {
    matches!(e, Expr::ColumnRef(name) if name.len() == 1 && name[0].to_string().eq_ignore_ascii_case("rownum"))
}
```

Notes:
- ROWNUM is `Expr::ColumnRef(["rownum"])` (verified parser/expr.rs:1116-1130). Case-insensitive match handles `ROWNUM`/`rownum`.
- `Ident` → string via `.to_string()` (Display impl preserves case). Use `name[0].to_string().eq_ignore_ascii_case("rownum")`.
- Optional: also match `Expr::FunctionCall { name: ["rownum"], args: [] }` (paren form) — defer unless cheap; the issue only requires the bare pseudo-column.

**Step 3: Tests**

```rust
#[test]
fn p024_rownum_in_where_warns() {
    let stmts = parse("SELECT id FROM users WHERE ROWNUM <= 10");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P024"), "ROWNUM in WHERE should trigger P024");
}

#[test]
fn p024_rownum_in_select_list_warns() {
    let stmts = parse("SELECT ROWNUM, id FROM users");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P024"), "ROWNUM in SELECT list should trigger P024");
}

#[test]
fn p024_limit_offset_no_warn() {
    let stmts = parse("SELECT id FROM users LIMIT 10 OFFSET 20");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P024"), "LIMIT/OFFSET pagination should not trigger P024");
}

#[test]
fn p024_rownum_inside_function_warns() {
    // Works because collect_selects_from_stmt now descends into CREATE bodies (#296).
    let stmts = parse("CREATE FUNCTION f() RETURNS SETOF int LANGUAGE plpgsql AS $$ BEGIN RETURN QUERY SELECT id FROM users WHERE ROWNUM <= 10; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P024"), "ROWNUM inside function body should trigger P024");
}
```

Run: `cargo test --lib linter` — expected PASS.

**Step 4: Commit**

```bash
git add src/linter/rules_performance.rs src/linter/tests.rs
git commit -m "feat(linter): add P024 rownum-pagination rule (#297)"
```

---

## Task 8 — #298: C019 `commit-inside-loop`

**Files:**
- Modify: `src/linter/rules_caution.rs` (register at L9; new check fn after C014 block ~L855)
- Modify: `src/linter/tests.rs`

**Step 1: Register** (append to `rules` vec in `register()`):

```rust
LintRuleEntry {
    id: "C019",
    name: "commit-inside-loop",
    description: "COMMIT/ROLLBACK inside a PL/pgSQL loop breaks transaction atomicity and harms performance",
    level: WarningLevel::Caution,
    stmt_kind: StatementKind::PlBlock,
    check_fn: check_c019,
},
```

**Step 2: Implement** — clone P021's walker (`rules_performance.rs:1138-1188`) with a Commit/Rollback arm:

```rust
// ── C019: COMMIT/ROLLBACK inside a PL/pgSQL loop ──

fn check_c019(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let mut found = false;
    for block in crate::linter::pl_blocks_from_stmt(&curr_stmt.statement) {
        check_pl_stmts_for_commit_in_loop(&block.body, &mut found, false);
        if found {
            break;
        }
    }
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C019",
            "commit-inside-loop",
            "\u{5faa}\u{73af}\u{5185} COMMIT/ROLLBACK \u{4f1a}\u{5bfc}\u{81f4}\u{4e8b}\u{52a1}\u{4e0d}\u{539f}\u{5b50}\u{ff0c}\u{4e14}\u{9891}\u{7e41}\u{63d0}\u{4ea4}\u{5f71}\u{54cd}\u{6027}\u{80fd}".into(),
            Some("\u{5c06} COMMIT/ROLLBACK \u{79fb}\u{51fa}\u{5faa}\u{73af}\u{ff0c}\u{6216}\u{4f7f}\u{7528}\u{6279}\u{91cf}\u{64cd}\u{4f5c}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn check_pl_stmts_for_commit_in_loop(pl_stmts: &[PlStatement], found: &mut bool, inside_loop: bool) {
    if *found {
        return;
    }
    for s in pl_stmts {
        match s {
            PlStatement::Loop(l) => check_pl_stmts_for_commit_in_loop(&l.body, found, true),
            PlStatement::While(w) => check_pl_stmts_for_commit_in_loop(&w.body, found, true),
            PlStatement::For(f) => check_pl_stmts_for_commit_in_loop(&f.body, found, true),
            PlStatement::ForEach(f) => check_pl_stmts_for_commit_in_loop(&f.body, found, true),
            PlStatement::Commit { .. } | PlStatement::Rollback { .. } if inside_loop => {
                *found = true;
                return;
            }
            PlStatement::Block(b) => check_pl_stmts_for_commit_in_loop(&b.body, found, inside_loop),
            PlStatement::If(i) => {
                check_pl_stmts_for_commit_in_loop(&i.then_stmts, found, inside_loop);
                for e in &i.elsifs {
                    check_pl_stmts_for_commit_in_loop(&e.stmts, found, inside_loop);
                }
                check_pl_stmts_for_commit_in_loop(&i.else_stmts, found, inside_loop);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_commit_in_loop(&w.stmts, found, inside_loop);
                }
                check_pl_stmts_for_commit_in_loop(&c.else_stmts, found, inside_loop);
            }
            _ => {}
        }
        if *found {
            return;
        }
    }
}
```

Verify imports: `PlStatement` is already imported in rules_caution.rs (L1 `use crate::ast::plpgsql::{..., PlStatement}`).

**Step 3: Tests**

```rust
#[test]
fn c019_commit_in_loop_warns() {
    let stmts = parse("DO $$ BEGIN FOR i IN 1..10 LOOP INSERT INTO t VALUES (i); COMMIT; END LOOP; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C019"), "COMMIT inside LOOP should trigger C019");
}

#[test]
fn c019_commit_outside_loop_no_warn() {
    let stmts = parse("DO $$ BEGIN INSERT INTO t VALUES (1); COMMIT; END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C019"), "COMMIT outside loop should NOT trigger C019 (C014 covers it)");
    assert!(has_rule(&w, "C014"), "COMMIT at block level still triggers C014");
}

#[test]
fn c019_rollback_in_while_loop_warns() {
    let stmts = parse("DO $$ BEGIN WHILE x < 10 LOOP ROLLBACK; x := x + 1; END LOOP; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C019"), "ROLLBACK inside WHILE loop should trigger C019");
}

#[test]
fn c019_commit_in_function_loop_warns() {
    // Enabled by #296 traversal.
    let stmts = parse("CREATE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$ BEGIN FOR i IN 1..10 LOOP COMMIT; END LOOP; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C019"), "COMMIT in function loop should trigger C019");
}
```

Run: `cargo test --lib linter` — expected PASS.

**Step 4: Commit**

```bash
git add src/linter/rules_caution.rs src/linter/tests.rs
git commit -m "feat(linter): add C019 commit-inside-loop rule (#298)"
```

---

## Task 9 — #299: R013 `implicit-join`

**Files:**
- Modify: `src/linter/rules_prohibition.rs` (register at L15; new check fn after R012 ~L117)
- Modify: `src/linter/tests.rs`

**Step 1: Register** (append to `rules` vec in `register()`):

```rust
LintRuleEntry {
    id: "R013",
    name: "implicit-join",
    description: "Oracle-style comma-separated FROM (implicit join) hurts readability; use ANSI JOIN",
    level: WarningLevel::Prohibition,
    stmt_kind: StatementKind::Select,
    check_fn: check_r013,
},
```

**Step 2: Implement** (after `check_r012`):

```rust
// R013: Oracle implicit join — FROM a, b (comma-separated table list)
// Explicit JOINs fold into a single TableRef::Join; multiple top-level
// TableRef entries mean comma-separated FROM.
fn check_r013(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if s.from.len() > 1 {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Prohibition,
                "R013",
                "implicit-join",
                "\u{4f7f}\u{7528} Oracle \u{9690}\u{5f0f}\u{8fde}\u{63a5}\u{8bed}\u{6cd5}\uff08\u{9017}\u{53f7}\u{5206}\u{9694} FROM\uff09\u{ff0c}\u{5efa}\u{8bae}\u{6539}\u{4e3a} ANSI JOIN".into(),
                Some("\u{6539}\u{7528} INNER JOIN / LEFT JOIN \u{63d0}\u{9ad8}\u{53ef}\u{8bfb}\u{6027}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}
```

Design note: `FROM a, b` with a WHERE join condition is an implicit inner join; without WHERE it is an implicit cross join (P013 does NOT catch comma-style cross joins — it only catches `CROSS JOIN` / `JOIN`-without-condition at rules_performance.rs:716-727). R013 therefore fires on `from.len() > 1` regardless of WHERE — simplest and matches the issue's trigger ("FROM 子句包含多个逗号分隔的表引用").

**Step 3: Tests**

```rust
#[test]
fn r013_implicit_join_warns() {
    let stmts = parse("SELECT a.id, b.name FROM a, b WHERE a.id = b.id");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R013"), "comma-separated FROM should trigger R013");
}

#[test]
fn r013_explicit_join_no_warn() {
    let stmts = parse("SELECT a.id, b.name FROM a INNER JOIN b ON a.id = b.id");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R013"), "explicit ANSI JOIN should not trigger R013");
}

#[test]
fn r013_single_table_no_warn() {
    let stmts = parse("SELECT id FROM a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R013"), "single-table FROM should not trigger R013");
}

#[test]
fn r013_cross_join_style_no_warn() {
    // Comma without WHERE = implicit cross join; R013 fires, P013 does not for comma style.
    let stmts = parse("SELECT * FROM a, b");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R013"), "comma-separated FROM without WHERE still triggers R013");
}
```

Run: `cargo test --lib linter` — expected PASS. Also verify no existing `p013_*` test (tests.rs:370) regresses: `SELECT * FROM a CROSS JOIN b` → `from.len() == 1` (single `TableRef::Join`) → R013 silent, P013 fires. Good.

**Step 4: Commit**

```bash
git add src/linter/rules_prohibition.rs src/linter/tests.rs
git commit -m "feat(linter): add R013 implicit-join rule (#299)"
```

---

## Task 10 — #300: P025 `in-subquery-to-exists`

**Files:**
- Modify: `src/linter/rules_performance.rs` (register at L9-202; new check fn after P024)
- Modify: `src/linter/tests.rs`

**Step 1: Register** (append after P024 entry):

```rust
LintRuleEntry {
    id: "P025",
    name: "in-subquery-to-exists",
    description: "IN (subquery) may perform worse than EXISTS or JOIN; consider rewriting",
    level: WarningLevel::Suggestion,
    stmt_kind: StatementKind::Dml,
    check_fn: check_p025,
},
```

**⚠️ Level decision (flagged for review):** the issue explicitly requests **Suggestion** level ("建议将此规则设为 Suggestion 级别而非 Prohibition 或 Performance"). `WarningLevel` is a per-entry field, and the R008/R009 precedent shows level is independent of file/prefix. However, a `P`-prefixed rule at Suggestion level breaks the P-series=Performance convention and will look odd in the rule catalog. **Alternative: rename to `S009` and give #301 `S010`.** Recommendation: honor the issue (P025 @ Suggestion) unless the reviewer prefers the S-series renumber — see Risk Notes.

**Step 2: Implement** — clone `check_p002` (rules_performance.rs:264-314), flip the InSubquery arm:

```rust
// P025: positive IN (subquery) → EXISTS or JOIN
fn check_p025(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Some(where_clause) = extract_where(&curr_stmt.statement) {
        walk_expr(where_clause, &mut |e| match e {
            Expr::InSubquery { negated: false, .. } => {
                warnings.push(make_warning(
                    WarningLevel::Suggestion,
                    "P025",
                    "in-subquery-to-exists",
                    "IN \u{5b50}\u{67e5}\u{8be2}\u{53ef}\u{80fd}\u{6027}\u{80fd}\u{4e0d}\u{4f73}\u{ff0c}\u{8003}\u{8651}\u{4f7f}\u{7528} EXISTS \u{6216} JOIN \u{66ff}\u{4ee3}".into(),
                    Some("\u{6539}\u{7528} EXISTS (SELECT 1 ...) \u{6216} JOIN"),
                    loc,
                    None,
                    confidence,
                ));
                false
            }
            _ => true,
        });
    }
}
```

`extract_where` (rules_performance.rs:204-210) handles Select/Update/Delete; `negated: false` distinguishes from P002 (`negated: true`). `Expr::InList` (literal lists) and `ScalarSublink` are NOT matched (issue: "非列表、非 negated").

**Step 3: Tests**

```rust
#[test]
fn p025_in_subquery_warns() {
    let stmts = parse("SELECT * FROM a WHERE id IN (SELECT id FROM b)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P025"), "positive IN subquery should trigger P025");
    assert!(!has_rule(&w, "P002"), "positive IN should NOT trigger P002");
}

#[test]
fn p025_not_in_still_p002() {
    let stmts = parse("SELECT * FROM a WHERE id NOT IN (SELECT id FROM b)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P025"), "NOT IN should not trigger P025");
    assert!(has_rule(&w, "P002"), "NOT IN still triggers P002");
}

#[test]
fn p025_in_list_no_warn() {
    let stmts = parse("SELECT * FROM a WHERE id IN (1, 2, 3)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P025"), "IN with literal list should not trigger P025");
}
```

Run: `cargo test --lib linter` — expected PASS. Verify existing `p002_*` tests (tests.rs:231) still PASS.

**Step 4: Commit**

```bash
git add src/linter/rules_performance.rs src/linter/tests.rs
git commit -m "feat(linter): add P025 in-subquery-to-exists rule (#300)"
```

---

## Task 11 — #301: S009 `tautological-condition`

**Files:**
- Modify: `src/linter/rules_suggestion.rs` (register at L9; new check fn after S008 ~L358)
- Modify: `src/linter/rules_performance.rs` — make `literals_equal` reusable (L900-905)
- Modify: `src/linter/mod.rs` — move/export `literals_equal` OR duplicate it
- Modify: `src/linter/tests.rs`

**Step 1: Make `literals_equal` shared**

Option A (preferred, DRY): move `literals_equal` from `rules_performance.rs:900-905` to `src/linter/mod.rs` as `pub(crate)`, update P015's call site (`rules_performance.rs:886`) to `crate::linter::literals_equal(low, high)`, and import in `rules_suggestion.rs`.
Option B (YAGNI): duplicate the 5-line helper in `rules_suggestion.rs`. Acceptable but violates DRY; prefer A since 2 call sites already exist.

**Step 1b: Make `extract_where` shared** — `check_s009` below calls `extract_where`, which currently exists ONLY as a private helper at `src/linter/rules_performance.rs:204-210`. Two clean options:
- Option A (preferred): move `extract_where` to `src/linter/mod.rs` as `pub(crate)`, update its 10+ call sites in `rules_performance.rs` (P002/P004/P005/P009/P011/P015 etc.) to use it via the existing `use crate::linter::{...}` import (add `extract_where` to that import list at rules_performance.rs:3-6), and import it in `rules_suggestion.rs`.
- Option B (smaller diff): add a local `extract_where` copy in `rules_suggestion.rs` (9 lines, same match on Select/Update/Delete).

Prefer Option A for DRY; both are acceptable. Verify with `grep -n "extract_where(" src/linter/rules_performance.rs | wc -l` to size the call-site update.

**Step 2: Register** (append to `rules` vec in `register()`):

```rust
LintRuleEntry {
    id: "S009",
    name: "tautological-condition",
    description: "Tautological conditions like 1=1 in WHERE are redundant; remove them",
    level: WarningLevel::Suggestion,
    stmt_kind: StatementKind::Dml,
    check_fn: check_s009,
},
```

**Step 3: Implement** (after `check_s008`):

```rust
// ── S009: tautological condition (1=1, 'a'='a') in WHERE ──

fn check_s009(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Some(where_clause) = extract_where(&curr_stmt.statement) {
        walk_expr(where_clause, &mut |e| {
            if let Expr::BinaryOp { op, left, right } = e {
                if is_comparison_op(op) && crate::linter::literals_equal(left, right) {
                    warnings.push(make_warning(
                        WarningLevel::Suggestion,
                        "S009",
                        "tautological-condition",
                        format!("WHERE \u{5b50}\u{53e5}\u{4e2d}\u{5b58}\u{5728}\u{6052}\u{771f}\u{6761}\u{4ef6} {} \u{ff0c}\u{5efa}\u{8bae}\u{79fb}\u{9664}", op),
                        Some("\u{79fb}\u{9664}\u{8be5}\u{6761}\u{4ef6}\uff0c\u{76f4}\u{63a5}\u{7f16}\u{5199}\u{5176}\u{4ed6}\u{6761}\u{4ef6}"),
                        loc,
                        None,
                        confidence,
                    ));
                }
            }
            true
        });
    }
}

fn is_comparison_op(op: &str) -> bool {
    matches!(op, "=" | "<>" | "!=" | ">" | "<" | ">=" | "<=")
}
```

**iBatis/MyBatis exclusion (issue requirement):** the issue says `1=1` is idiomatic in dynamic SQL and should NOT warn there. Detection of origin is out of scope for the linter's plain-SQL path (the linter receives parsed statements, not the XML context). **Decision: implement without origin detection in v1.** The `--mybatis`/`parse-xml` pipeline uses `Expr::MyBatisParam`/`MyBatisRawExpr` for placeholders, so a future refinement can skip S009 when the WHERE contains MyBatis expr variants. Flag in Risk Notes.

**Step 4: Tests**

```rust
#[test]
fn s009_one_equals_one_warns() {
    let stmts = parse("SELECT * FROM users WHERE 1=1 AND status = 'A'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "S009"), "1=1 in WHERE should trigger S009");
}

#[test]
fn s009_string_literals_equal_warns() {
    let stmts = parse("SELECT * FROM users WHERE 'a' = 'a'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "S009"), "equal string literals should trigger S009");
}

#[test]
fn s009_one_equals_two_no_warn() {
    let stmts = parse("SELECT * FROM users WHERE 1 = 2");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S009"), "1=2 is not tautological");
}

#[test]
fn s009_column_equals_literal_no_warn() {
    let stmts = parse("SELECT * FROM users WHERE status = 'A'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S009"), "column=literal should not trigger S009");
}
```

Run: `cargo test --lib linter` — expected PASS.

**Step 5: Commit**

```bash
git add src/linter/rules_suggestion.rs src/linter/rules_performance.rs src/linter/mod.rs src/linter/tests.rs
git commit -m "feat(linter): add S009 tautological-condition rule (#301)"
```

---

## Task 12 — Documentation + version + metadata

**Files:**
- Modify: `docs/user-guide.md` (rule catalog table, section 11.4, ~L859-925)
- Modify: `README.md` (rule counts in Phase 5 row + Status line)
- Modify: `Cargo.toml` (version bump)
- Modify: `CHANGELOG.md` (add entry)

**Step 1: Update rule catalog**

Add rows for P024, P025, C019, R013, S009 in `docs/user-guide.md` section 11.4 with columns (ID | name | 适用 | 描述) matching the existing table format (see existing rows at L884/895/897/903/920-922).

**Step 2: Update rule counts**

Run: `grep -rn "53 rule\|53 lint\|54 rules" README.md docs/ src/ | head -20`
Then replace the stale counts (README Phase 5 + Status, user-guide) with the actual post-change count. Authoritative count: write a tiny throwaway test or use `cargo test` with `all_rules_metadata().len()`; expected 61 (56 + 5). Update text accordingly.

**Step 3: Bump version**

Check `Cargo.toml` `version`. Given 1 bugfix + 5 features: bump minor (0.10.1 → 0.11.0) — check repo convention from `git log --oneline -15` (e.g. "Bump v0.8.30: R011/R012 ORDER BY + DISTINCT/GROUP BY linter rules" suggests feature bumps use minor). Add CHANGELOG entry summarizing #296-#301.

**Step 4: Commit**

```bash
git add docs/user-guide.md README.md Cargo.toml CHANGELOG.md
git commit -m "docs: document P024/P025/C019/R013/S009 rules; bump version"
```

---

## Verification (per AGENTS.md — mandatory before commit/push)

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-features -- -D warnings
cargo test --all-features
```

Plus targeted CLI smoke (after `cargo build`):

```bash
# #296 — must now warn
printf "CREATE OR REPLACE FUNCTION bad_func RETURN VARCHAR2 IS\n v_name VARCHAR2;\nBEGIN\n SELECT * INTO v_name FROM users WHERE id = 1;\n EXECUTE IMMEDIATE 'DELETE FROM log WHERE id=' || v_name;\n RETURN v_name;\nEXCEPTION\n WHEN OTHERS THEN RETURN NULL;\nEND;" | target/debug/ogsql validate --lint
# #297
echo "SELECT id FROM users WHERE ROWNUM <= 10" | target/debug/ogsql validate --lint
# #298
echo "DO \$\$ BEGIN FOR i IN 1..10 LOOP COMMIT; END LOOP; END \$\$" | target/debug/ogsql validate --lint
# #299
echo "SELECT a.id, b.name FROM a, b WHERE a.id = b.id" | target/debug/ogsql validate --lint
# #300
echo "SELECT * FROM a WHERE id IN (SELECT id FROM b)" | target/debug/ogsql validate --lint
# #301
echo "SELECT * FROM users WHERE 1=1 AND status = 'A'" | target/debug/ogsql validate --lint
```

All six must emit the corresponding rule warnings.

---

## Out of Scope

- Reclassifying other DDL-with-bodies (e.g. `CreateTrigger` bodies, `AlterFunction`) — #296 names only the 3 CREATE variants; others remain Ddl.
- `collect_selects_from_pl_stmts` handling `PlStatement::Execute` (dynamic SQL) — dynamic SQL is not statically parsed; out of scope.
- ROWNUM `FunctionCall` (paren) form for P024.
- S009 iBatis-origin detection (needs XML-context plumbing; deferred).
- Updating `docs/ast-json-reference.md` — no AST shape changes in this plan (StatementKind/`pl_blocks_from_stmt` are internal; StatementInfo untouched).
- P025/P024/C019/R013/S009 in the MCP/HTTP rule listing — they flow automatically via `all_rules_metadata()`.

## Risk Notes

1. **#296 reclassification** — the only behavior-risky change. Mitigations: (a) the sole Ddl-gated rule C010 (rules_caution.rs:64) matches only `CreateTable`/`CreateTableAs` (check_c010 at L451-488) and is unaffected by reclassifying the 3 CREATE variants; (b) existing R010 tests (tests.rs:1395-1540) exercise CREATE bodies and must stay green; (c) full `cargo test --all-features` gate.
2. **R001 semantics** — `has_outermost` logic must NOT treat embedded SELECTs as outermost. With `CreateFunction` now `PlBlock`, `has_outermost` (rules_prohibition.rs:138-146) is `false` for it → all embedded SELECT * warn. Verified by design; covered by `r001_select_star_inside_function_warns` + existing `r001_outer_with_inner_allowed`.
3. **R001 `stmt_kind: All` side effect** — changing R001 from `Dml` to `All` (Task 5) also newly dispatches R001 on top-level `CreateTableAs`/`CreateView`/`CreateMaterializedView` (previously unreachable: Dml-gated vs Ddl-classified). `has_outermost` (L138-146) already lists those variants, so the pre-existing logic activates as designed: outer `SELECT *` in a view/CTAS body is allowed only when it wraps inner explicit columns; standalone `CREATE VIEW v AS SELECT * FROM t` now warns. This is a deliberate, design-consistent behavior expansion — flag in PR description. If reviewers want zero behavior change outside PL bodies, alternative is a dedicated `stmt_kind` (e.g. `DmlOrPlBlock`) — rejected as over-engineering unless requested.
4. **P025 level naming** — `P025` @ Suggestion violates the P-series=Performance convention. Options: keep as-is (honor issue) vs renumber to `S010` (and shift #301 to `S011`) — the latter churns 2 issues' rule IDs. **Default: keep P025 @ Suggestion**; reviewer should veto if convention matters more than issue fidelity.
5. **S009 iBatis false positives** — `SELECT * FROM t WHERE 1=1 AND id=#{id}` (MyBatis) would warn. Deferred origin detection; if reviewers consider it a blocker, add a guard: skip when the WHERE clause contains `Expr::MyBatisParam`/`MyBatisRawExpr` (adds ~10 lines to check_s009).
6. **C013 package-body test** — the PACKAGE BODY test SQL must parse cleanly in this codebase (Oracle-style `RETURN VARCHAR2 IS` inside package). If the parser rejects it, use `CREATE OR REPLACE PACKAGE BODY pkg AS FUNCTION f RETURN VARCHAR2 IS ... END; END pkg;` variant that matches existing `parse_package_body_items` support (see R010's `PackageItem::Function` handling at rules_prohibition.rs:706). Verify parse first (`parse(...)` panics on error → test will surface it).
7. **Double-warning risk** — the #246 double-iteration fix (commit `2c767f8`) must not regress: R001/R005/R006/R007 now dispatch on both `CreateFunction` (as PlBlock) AND... they dispatch once per top-level statement; `collect_selects_from_stmt`/`where_and_tables` each run once per dispatch. No duplication introduced — but run the full suite to confirm no warning-count assertions break (e.g. tests asserting exact warning counts).
