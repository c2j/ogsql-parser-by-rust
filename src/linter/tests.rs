use crate::linter::{Confidence, LintConfig, SqlLinter, WarningLevel};
use crate::parser::Parser;

fn parse(sql: &str) -> Vec<crate::ast::StatementInfo> {
    let (stmts, _) = Parser::parse_sql(sql);
    stmts
}

fn lint(stmts: &[crate::ast::StatementInfo]) -> Vec<crate::linter::SqlWarning> {
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    linter.lint(stmts, None, Confidence::Full)
}

fn lint_confidence(stmts: &[crate::ast::StatementInfo], confidence: Confidence) -> Vec<crate::linter::SqlWarning> {
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    linter.lint(stmts, None, confidence)
}

fn lint_min_level(stmts: &[crate::ast::StatementInfo], level: WarningLevel) -> Vec<crate::linter::SqlWarning> {
    let config = LintConfig { min_level: level, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    linter.lint(stmts, None, Confidence::Full)
}

fn has_rule(warnings: &[crate::linter::SqlWarning], rule_id: &str) -> bool {
    warnings.iter().any(|w| w.rule_id == rule_id)
}

// ── R001: SELECT * ──
// Outermost SELECT * is allowed ONLY when it wraps inner queries.
// Standalone and inner/nested SELECT * always warn.

#[test]
fn r001_standalone_select_star_warns() {
    let stmts = parse("SELECT * FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "standalone SELECT * should trigger R001");
}

#[test]
fn r001_outer_with_inner_allowed() {
    // Outer * wraps an explicit inner — allowed pass-through.
    let stmts = parse("SELECT * FROM (SELECT id, name FROM t1) sub");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R001"), "outer * wrapping explicit inner should be allowed");
}

#[test]
fn r001_from_subquery_star_flagged() {
    let stmts = parse("SELECT id FROM (SELECT * FROM t1) sub");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "SELECT * in FROM subquery should trigger R001");
}

#[test]
fn r001_cte_body_star_flagged() {
    let stmts = parse("WITH cte AS (SELECT * FROM t1) SELECT id FROM cte");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "SELECT * in CTE body should trigger R001");
}

#[test]
fn r001_where_subquery_star_flagged() {
    let stmts = parse("SELECT id FROM t1 WHERE id IN (SELECT * FROM t2)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "SELECT * in WHERE subquery should trigger R001");
}

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

#[test]
fn r007_like_and_compound_warns() {
    // LIKE followed by AND must still trigger R007 (precedence regression guard).
    let stmts = parse("SELECT v FROM users WHERE name LIKE '%abc' AND status = 1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R007"), "LIKE '%...' AND ... should trigger R007");
}

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

#[test]
fn issue296_plain_dml_still_lints() {
    let stmts = parse("SELECT * FROM users WHERE DECODE(status, A, 1, 0) = 1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "plain SELECT * still triggers R001");
    assert!(has_rule(&w, "P009"), "DECODE still triggers P009");
}

#[test]
fn r001_union_both_select_star_warns() {
    let stmts = parse("SELECT * FROM t1 UNION ALL SELECT * FROM t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "UNION branches with SELECT * should trigger R001");
}

#[test]
fn r001_qualified_star_not_triggered() {
    let stmts = parse("SELECT t1.* FROM t1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R001"), "t1.* should not trigger R001");
}

#[test]
fn r001_explicit_columns_not_triggered() {
    let stmts = parse("SELECT id, name FROM t1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R001"));
}

// ── R002: Large column sort ──

#[test]
fn r002_large_group_by() {
    let sql = "SELECT a,b,c,d,e,f,g,h,i,j,k FROM t1 GROUP BY a,b,c,d,e,f,g,h,i,j,k";
    let stmts = parse(sql);
    let w = lint(&stmts);
    assert!(has_rule(&w, "R002"), "expected R002 for GROUP BY with 11 columns");
}

#[test]
fn r002_small_group_by_not_triggered() {
    let stmts = parse("SELECT a, b FROM t1 GROUP BY a, b");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R002"));
}

// ── R003: LOCK TABLE ──

#[test]
fn r003_lock_table() {
    let stmts = parse("LOCK TABLE t1 IN EXCLUSIVE MODE");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R003"), "expected R003 for LOCK TABLE");
}

// ── R004: DROP CASCADE ──

#[test]
fn r004_drop_cascade() {
    let stmts = parse("DROP TABLE t1 CASCADE");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R004"), "expected R004 for DROP CASCADE");
}

#[test]
fn r004_drop_no_cascade_not_triggered() {
    let stmts = parse("DROP TABLE t1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R004"));
}

// ── R005: Implicit type conversion ──

#[test]
fn r005_literal_column_comparison_no_schema_skipped() {
    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    let w = lint(&stmts);
    assert!(
        !has_rule(&w, "R005"),
        "without schema, R005 should skip — no evidence of cross-family conversion (issue #240)"
    );
}

#[test]
fn r005_both_columns_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE a = b");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R005"), "col=col should not trigger R005");
}

// ── R006: Function on WHERE column ──

#[test]
fn r006_function_on_column() {
    let stmts = parse("SELECT * FROM t WHERE LENGTH(name) > 5");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R006"), "expected R006 for LENGTH(col) in WHERE");
}

#[test]
fn r006_plain_column_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    // R005 may trigger, but R006 should not
    assert!(!has_rule(&lint(&stmts), "R006"));
}

// ── R007: LIKE leading wildcard ──

#[test]
fn r007_leading_percent() {
    let stmts = parse("SELECT * FROM t WHERE name LIKE '%abc'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R007"), "expected R007 for LIKE '%...'");
}

#[test]
fn r007_trailing_wildcard_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE name LIKE 'abc%'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R007"), "LIKE 'abc%' should not trigger R007");
}

// ── R008: Same-table column compare ──

#[test]
fn r008_same_table_columns() {
    let stmts = parse("SELECT * FROM t WHERE t.a > t.b");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R008"), "expected R008 for t.a > t.b");
}

#[test]
fn r008_different_tables_not_triggered() {
    let stmts = parse("SELECT * FROM t1 JOIN t2 ON t1.id = t2.id WHERE t1.a > t2.b");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R008"));
}

// ── R009: Scalar subquery in SELECT ──

#[test]
fn r009_subquery_in_select() {
    let stmts = parse("SELECT (SELECT MAX(id) FROM t2) FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R009"), "expected R009 for scalar subquery in SELECT");
}

#[test]
fn r009_subquery_in_where_not_triggered() {
    let stmts = parse("SELECT * FROM t1 WHERE id IN (SELECT id FROM t2)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R009"), "subquery in WHERE should not trigger R009");
}

// ── P001: UNION without ALL ──

#[test]
fn p001_union_without_all() {
    let stmts = parse("SELECT id FROM t1 UNION SELECT id FROM t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P001"), "expected P001 for UNION without ALL");
}

#[test]
fn p001_union_all_not_triggered() {
    let stmts = parse("SELECT id FROM t1 UNION ALL SELECT id FROM t2");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P001"));
}

// ── P002: NOT IN subquery ──

#[test]
fn p002_not_in_subquery() {
    let stmts = parse("SELECT * FROM t WHERE id NOT IN (SELECT id FROM t2)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P002"), "expected P002 for NOT IN (subquery)");
}

#[test]
fn p002_in_subquery_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE id IN (SELECT id FROM t2)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P002"));
}

// ── P003: IN list too large ──

#[test]
fn p003_large_in_list() {
    let values: Vec<&str> = (1..=600).map(|_| "1").collect();
    let sql = format!("SELECT * FROM t WHERE id IN ({})", values.join(","));
    let stmts = parse(&sql);
    let config = LintConfig { in_list_threshold: 500, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(has_rule(&w, "P003"), "expected P003 for IN list > 500");
}

#[test]
fn p003_small_in_list_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE id IN (1, 2, 3)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P003"));
}

// ── P004: OR as top-level WHERE ──

#[test]
fn p004_or_top_level() {
    let stmts = parse("SELECT * FROM t WHERE a = 1 OR b = 2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P004"), "expected P004 for top-level OR");
}

#[test]
fn p004_or_inside_and_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE c = 1 AND (a = 1 OR b = 2)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P004"), "OR inside AND should not trigger P004");
}

// ── P005: now() function ──

#[test]
fn p005_now_function() {
    let stmts = parse("SELECT * FROM t WHERE created > now()");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P005"), "expected P005 for now()");
}

#[test]
fn p005_other_function_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE name = UPPER('abc')");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P005"));
}

// ── P006: COUNT(*) ──

#[test]
fn p006_count_star() {
    let stmts = parse("SELECT COUNT(*) FROM t");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P006"), "expected P006 for COUNT(*)");
}

#[test]
fn p006_count_column_not_triggered() {
    let stmts = parse("SELECT COUNT(id) FROM t");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P006"));
}

// ── P009: Function that should be CASE ──

#[test]
fn p009_nvl_in_where() {
    let stmts = parse("SELECT * FROM t WHERE NVL(status, 0) = 1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P009"), "expected P009 for NVL in WHERE");
}

#[test]
fn p009_decode_in_where() {
    let stmts = parse("SELECT * FROM t WHERE DECODE(code, 1, 'A', 'B') = 'A'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P009"), "expected P009 for DECODE in WHERE");
}

#[test]
fn p009_coalesce_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE COALESCE(status, 0) = 1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P009"), "COALESCE should not trigger P009");
}

// ── P010: Multi-column UPDATE from subquery ──

#[test]
fn p010_multi_col_update_subquery() {
    let stmts = parse("UPDATE t SET (a, b) = (SELECT x, y FROM t2)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P010"), "expected P010 for multi-col UPDATE subquery");
}

#[test]
fn p010_single_col_not_triggered() {
    let stmts = parse("UPDATE t SET a = (SELECT x FROM t2)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P010"));
}

// ── P012: Unnecessary DISTINCT ──

#[test]
fn p012_distinct() {
    let stmts = parse("SELECT DISTINCT a, b FROM t");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P012"), "expected P012 for DISTINCT");
}

#[test]
fn p012_no_distinct_not_triggered() {
    let stmts = parse("SELECT a, b FROM t");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P012"));
}

// ── P013: Cartesian product ──

#[test]
fn p013_cross_join() {
    let stmts = parse("SELECT * FROM t1 CROSS JOIN t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P013"), "expected P013 for CROSS JOIN");
}

// ── P014: Deeply nested subquery ──

#[test]
fn p014_deep_nesting() {
    let sql = "SELECT * FROM t WHERE id IN (SELECT id FROM t2 WHERE id IN (SELECT id FROM t3 WHERE id IN (SELECT id FROM t4)))";
    let stmts = parse(sql);
    let config = LintConfig { subquery_depth_limit: 3, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(has_rule(&w, "P014"), "expected P014 for deeply nested subquery");
}

// ── P015: Range equals same value ──

#[test]
fn p015_between_same() {
    let stmts = parse("SELECT * FROM t WHERE a BETWEEN 5 AND 5");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P015"), "expected P015 for BETWEEN 5 AND 5");
}

#[test]
fn p015_between_different_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE a BETWEEN 1 AND 10");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P015"));
}

// ── P016: UPDATE FROM without join condition ──

#[test]
fn p016_update_from_no_where() {
    let stmts = parse("UPDATE t SET a = t2.b FROM t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P016"), "expected P016 for UPDATE FROM without WHERE");
}

#[test]
fn p016_update_from_with_where_not_triggered() {
    let stmts = parse("UPDATE t SET a = t2.b FROM t2 WHERE t.id = t2.id");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P016"));
}

// ── P017: MERGE ──

#[test]
fn p017_merge() {
    let sql = "MERGE INTO target t USING source s ON t.id = s.id WHEN MATCHED THEN UPDATE SET t.a = s.a";
    let stmts = parse(sql);
    let w = lint(&stmts);
    assert!(has_rule(&w, "P017"), "expected P017 for MERGE");
}

// ── P018: INSERT SELECT without columns ──

#[test]
fn p018_insert_select_no_columns() {
    let stmts = parse("INSERT INTO t SELECT * FROM t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P018"), "expected P018 for INSERT SELECT without columns");
}

#[test]
fn p018_insert_select_with_columns_not_triggered() {
    let stmts = parse("INSERT INTO t (a, b) SELECT x, y FROM t2");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P018"));
}

// ── P022: EXPLAIN in production ──

#[test]
fn p022_explain() {
    let stmts = parse("EXPLAIN SELECT * FROM t");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P022"), "expected P022 for EXPLAIN");
}

// ── P021: Row-by-row INSERT in loop ──

#[test]
fn p021_insert_in_loop() {
    let stmts = parse("DO $$ BEGIN FOR i IN 1..10 LOOP INSERT INTO t VALUES (i); END LOOP; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P021"), "expected P021 for INSERT in loop");
}

#[test]
fn p021_insert_outside_loop_not_triggered() {
    let stmts = parse("DO $$ BEGIN INSERT INTO t VALUES (1); END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P021"), "INSERT outside loop should not trigger P021");
}

// ── P023: CONNECT BY performance ──

#[test]
fn p023_connect_by() {
    let stmts = parse("SELECT * FROM emp CONNECT BY PRIOR empno = mgr");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY");
}

#[test]
fn p023_connect_by_with_start_with() {
    let stmts = parse("SELECT * FROM emp START WITH mgr IS NULL CONNECT BY PRIOR empno = mgr");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY with START WITH");
}

#[test]
fn p023_connect_by_nocycle() {
    let stmts = parse("SELECT * FROM emp CONNECT BY NOCYCLE PRIOR empno = mgr");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY NOCYCLE");
}

#[test]
fn p023_no_connect_by_not_triggered() {
    let stmts = parse("SELECT * FROM emp WHERE empno = 1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P023"), "plain SELECT should not trigger P023");
}

#[test]
fn p023_select_order_by_not_triggered() {
    let stmts = parse("SELECT * FROM users ORDER BY name");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "P023"));
}

// ── P024: ROWNUM pagination ──

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
    let stmts = parse(
        "CREATE FUNCTION f() RETURNS SETOF int LANGUAGE plpgsql AS $$ BEGIN RETURN QUERY SELECT id FROM users WHERE ROWNUM <= 10; END; $$",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "P024"), "ROWNUM inside function body should trigger P024");
}

// P023 nested scenarios — detected via collect_nested_selects

#[test]
fn p023_subquery_in_where_triggered() {
    let stmts = parse("SELECT * FROM t WHERE col IN (SELECT * FROM emp CONNECT BY PRIOR empno = mgr)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in IN subquery");
}

#[test]
fn p023_exists_subquery_triggered() {
    let stmts = parse("SELECT * FROM t WHERE EXISTS (SELECT 1 FROM emp CONNECT BY PRIOR empno = mgr)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in EXISTS subquery");
}

#[test]
fn p023_from_subquery_triggered() {
    let stmts = parse("SELECT * FROM (SELECT * FROM emp CONNECT BY PRIOR empno = mgr) sub");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in FROM subquery");
}

#[test]
fn p023_for_loop_triggered() {
    let stmts =
        parse("DO $$ BEGIN FOR rec IN SELECT * FROM emp CONNECT BY PRIOR empno = mgr LOOP NULL; END LOOP; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in FOR loop");
}

#[test]
fn p023_cursor_declaration_triggered() {
    let stmts =
        parse("DO $$ DECLARE cur CURSOR FOR SELECT * FROM emp CONNECT BY PRIOR empno = mgr; BEGIN NULL; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in cursor declaration");
}

#[test]
fn p023_cte_triggered() {
    let stmts = parse("WITH cte AS (SELECT * FROM emp CONNECT BY PRIOR empno = mgr) SELECT * FROM cte");
    let w = lint(&stmts);
    assert!(has_rule(&w, "P023"), "expected P023 for CONNECT BY in CTE");
}

// ── Confidence propagation ──

#[test]
fn confidence_partial_propagated() {
    let stmts = parse("SELECT * FROM t1");
    let w = lint_confidence(&stmts, Confidence::Partial);
    assert!(w.iter().all(|w| w.confidence == Confidence::Partial));
}

#[test]
fn confidence_full_propagated() {
    let stmts = parse("SELECT * FROM t1");
    let w = lint_confidence(&stmts, Confidence::Full);
    assert!(w.iter().all(|w| w.confidence == Confidence::Full));
}

// ── Min level filter ──

#[test]
fn min_level_prohibition_filters_lower() {
    let stmts = parse("SELECT DISTINCT a FROM t");
    let w = lint_min_level(&stmts, WarningLevel::Prohibition);
    assert!(!has_rule(&w, "P012"), "P012 (Performance) should be filtered by min_level=Prohibition");
}

// ── Suppress rule ──

#[test]
fn suppress_rule() {
    let config = LintConfig { suppress: vec!["R001".into()], ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let stmts = parse("SELECT * FROM t");
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(!has_rule(&w, "R001"), "R001 should be suppressed");
}

// ── Multiple rules on same statement ──

#[test]
fn multiple_rules_same_statement() {
    let stmts = parse("SELECT * FROM t1 UNION SELECT * FROM t2");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R001"), "standalone UNION branches with SELECT * should trigger R001");
    assert!(has_rule(&w, "P001"), "expected P001 for UNION");
}

// ── No warnings on clean SQL ──

#[test]
fn no_warnings_clean_sql() {
    let stmts = parse("SELECT id, name FROM users WHERE status = 'active' ORDER BY id");
    let w = lint(&stmts);
    // Without schema, neither R005 nor S007 fire (both require schema evidence).
    assert!(w.is_empty(), "clean SQL should not produce warnings: {:?}", w);
}

// ════════════════════════════════════════════════════════════════
// Phase 2a: Caution rules (C001-C016)
// ════════════════════════════════════════════════════════════════

// ── C001: Unknown hint ──

#[test]
fn c001_unknown_hint() {
    let stmts = parse("SELECT /*+ bogus_hint(t1) */ * FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C001"), "expected C001 for unknown hint");
}

#[test]
fn c001_known_hint_not_triggered() {
    let stmts = parse("SELECT /*+ hashjoin(t1) */ * FROM t1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C001"), "known hint should not trigger C001");
}

// ── C005: Contradictory hints ──

#[test]
fn c005_contradictory_scan_hints() {
    let stmts = parse("SELECT /*+ tablescan(t1) indexscan(t1) */ * FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C005"), "expected C005 for contradictory scan hints");
}

#[test]
fn c005_contradictory_negation() {
    let stmts = parse("SELECT /*+ expand_sublink no_expand_sublink */ * FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C005"), "expected C005 for hint + no_ negation");
}

// ── C006: Hint table not in FROM ──

#[test]
fn c006_hint_table_not_in_from() {
    let stmts = parse("SELECT /*+ hashjoin(ghost_table) */ * FROM t1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C006"), "expected C006 for hint referencing non-FROM table");
}

#[test]
fn c006_hint_table_in_from_not_triggered() {
    let stmts = parse("SELECT /*+ hashjoin(t1) */ * FROM t1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C006"), "hint table in FROM should not trigger C006");
}

// ── C007: UPDATE without WHERE ──

#[test]
fn c007_update_no_where() {
    let stmts = parse("UPDATE t SET a = 1");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C007"), "expected C007 for UPDATE without WHERE");
}

#[test]
fn c007_update_with_where_not_triggered() {
    let stmts = parse("UPDATE t SET a = 1 WHERE id = 1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C007"));
}

// ── C008: DELETE without WHERE ──

#[test]
fn c008_delete_no_where() {
    let stmts = parse("DELETE FROM t");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C008"), "expected C008 for DELETE without WHERE");
}

#[test]
fn c008_delete_with_where_not_triggered() {
    let stmts = parse("DELETE FROM t WHERE id = 1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C008"));
}

// ── C009: INSERT without column list ──

#[test]
fn c009_insert_no_columns() {
    let stmts = parse("INSERT INTO t VALUES (1, 2)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C009"), "expected C009 for INSERT without columns");
}

#[test]
fn c009_insert_with_columns_not_triggered() {
    let stmts = parse("INSERT INTO t (a, b) VALUES (1, 2)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C009"));
}

// ── C010: Unlogged table ──

#[test]
fn c010_unlogged_table() {
    let stmts = parse("CREATE UNLOGGED TABLE t (id INT)");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C010"), "expected C010 for UNLOGGED TABLE");
}

#[test]
fn c010_regular_table_not_triggered() {
    let stmts = parse("CREATE TABLE t (id INT)");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C010"));
}

// ── C011: GOTO statement ──

#[test]
fn c011_goto_in_pl() {
    let stmts = parse("DO $$ BEGIN GOTO label1; <<label1>> NULL; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C011"), "expected C011 for GOTO");
}

// ── C012: EXECUTE with concatenation ──

#[test]
fn c012_execute_concat() {
    let stmts = parse("DO $$ BEGIN EXECUTE IMMEDIATE 'SELECT * FROM ' || v_tablename; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C012"), "expected C012 for EXECUTE with concatenation");
}

#[test]
fn c012_execute_no_concat_not_triggered() {
    let stmts = parse("DO $$ BEGIN EXECUTE IMMEDIATE 'SELECT 1'; END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C012"), "EXECUTE without concatenation should not trigger C012");
}

// ── C013: Exception swallow ──

#[test]
fn c013_exception_swallow() {
    let stmts = parse("DO $$ BEGIN NULL; EXCEPTION WHEN OTHERS THEN NULL; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C013"), "expected C013 for WHEN OTHERS THEN without RAISE");
}

#[test]
fn c013_exception_with_raise_not_triggered() {
    let stmts = parse("DO $$ BEGIN NULL; EXCEPTION WHEN OTHERS THEN RAISE NOTICE '%', SQLERRM; END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C013"), "WHEN OTHERS with RAISE should not trigger C013");
}

// ── C014: PL COMMIT/ROLLBACK ──

#[test]
fn c014_pl_commit() {
    let stmts = parse("DO $$ BEGIN COMMIT; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C014"), "expected C014 for COMMIT in PL block");
}

#[test]
fn c014_pl_rollback() {
    let stmts = parse("DO $$ BEGIN ROLLBACK; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C014"), "expected C014 for ROLLBACK in PL block");
}

// ── #296: PL rules traverse CREATE FUNCTION/PROCEDURE/PACKAGE BODY ──

#[test]
fn c014_commit_in_function_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN COMMIT; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C014"), "COMMIT inside CREATE FUNCTION body should trigger C014");
}

#[test]
fn c012_concat_in_procedure_warns() {
    let stmts =
        parse("CREATE PROCEDURE p() LANGUAGE plpgsql AS $$ BEGIN EXECUTE IMMEDIATE 'SELECT * FROM ' || v_t; END; $$");
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

// ── C015: SELECT FOR UPDATE blocking ──

#[test]
fn c015_for_update_blocking() {
    let stmts = parse("SELECT * FROM t FOR UPDATE");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C015"), "expected C015 for SELECT FOR UPDATE");
}

#[test]
fn c015_for_update_nowait_not_triggered() {
    let stmts = parse("SELECT * FROM t FOR UPDATE NOWAIT");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C015"), "FOR UPDATE NOWAIT should not trigger C015");
}

// ── C016: Autonomous transaction ──

#[test]
fn c016_autonomous_transaction() {
    let stmts = parse("DO $$ DECLARE PRAGMA AUTONOMOUS_TRANSACTION; BEGIN NULL; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C016"), "expected C016 for AUTONOMOUS_TRANSACTION");
}

// ── C017: RAISE in EXCEPTION clears variables ──

#[test]
fn c017_raise_in_exception_bare_raise() {
    let stmts = parse("DO $$ BEGIN NULL; EXCEPTION WHEN OTHERS THEN c2 := 5; RAISE; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C017"), "expected C017 for bare RAISE in EXCEPTION");
}

#[test]
fn c017_raise_exception_in_exception() {
    let stmts = parse("DO $$ BEGIN NULL; EXCEPTION WHEN OTHERS THEN RAISE EXCEPTION 'err'; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C017"), "expected C017 for RAISE EXCEPTION in EXCEPTION");
}

#[test]
fn c017_raise_info_not_triggered() {
    let stmts = parse("DO $$ BEGIN NULL; EXCEPTION WHEN OTHERS THEN RAISE INFO 'msg'; END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C017"), "RAISE INFO should not trigger C017");
}

#[test]
fn c017_no_exception_block_not_triggered() {
    let stmts = parse("DO $$ BEGIN RAISE EXCEPTION 'err'; END $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C017"), "RAISE outside EXCEPTION should not trigger C017");
}

#[test]
fn c017_nested_block_outer_exception() {
    let stmts =
        parse("DO $$ BEGIN BEGIN NULL; EXCEPTION WHEN OTHERS THEN RAISE; END; EXCEPTION WHEN OTHERS THEN NULL; END $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "C017"), "expected C017 for nested block EXCEPTION with RAISE");
}

// ── C018: Excessive INSERT VALUES (rows × columns) ──

#[test]
fn c018_excessive_insert_values_trigger() {
    // 4 rows × 3 cols = 12 > 10
    let sql = "INSERT INTO t (a, b, c) VALUES (1,2,3), (4,5,6), (7,8,9), (10,11,12)";
    let stmts = parse(sql);
    let config = LintConfig { max_insert_values_rows: 10, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(has_rule(&w, "C018"), "4×3=12 > 10 should trigger C018");
}

#[test]
fn c018_insert_values_within_limit() {
    // 3 rows × 3 cols = 9 < 10
    let sql = "INSERT INTO t (a, b, c) VALUES (1,2,3), (4,5,6), (7,8,9)";
    let stmts = parse(sql);
    let config = LintConfig { max_insert_values_rows: 10, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(!has_rule(&w, "C018"), "3×3=9 ≤ 10 should NOT trigger C018");
}

#[test]
fn c018_implicit_columns_infer_from_values() {
    // No column list → infer 2 cols from values, 11 rows × 2 = 22 > 20
    let mut vals: Vec<String> = (0..11).map(|i| format!("({i}, 'x')")).collect();
    let sql = format!("INSERT INTO t VALUES {}", vals.join(", "));
    let stmts = parse(&sql);
    let config = LintConfig { max_insert_values_rows: 20, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(has_rule(&w, "C018"), "11×2=22 > 20 should trigger C018 with implicit columns");
}

#[test]
fn c018_single_row_not_triggered() {
    let stmts = parse("INSERT INTO t (a, b) VALUES (1, 'x')");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C018"), "single-row INSERT should not trigger C018");
}

#[test]
fn c018_insert_select_not_triggered() {
    let stmts = parse("INSERT INTO t (a, b) SELECT c, d FROM src");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "C018"), "INSERT SELECT should not trigger C018");
}

// ════════════════════════════════════════════════════════════════
// Phase 2b: Suggestion rules (S001-S008)
// ════════════════════════════════════════════════════════════════

// ── S001: DELETE full table → use TRUNCATE ──

#[test]
fn s001_delete_full_table() {
    let stmts = parse("DELETE FROM t");
    let w = lint(&stmts);
    assert!(has_rule(&w, "S001"), "expected S001 for DELETE without WHERE");
}

// ── S002: LIMIT + OFFSET → use cursor ──

#[test]
fn s002_limit_offset() {
    let stmts = parse("SELECT * FROM t LIMIT 10 OFFSET 20");
    let w = lint(&stmts);
    assert!(has_rule(&w, "S002"), "expected S002 for OFFSET");
}

#[test]
fn s002_limit_no_offset_not_triggered() {
    let stmts = parse("SELECT * FROM t LIMIT 10");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S002"), "LIMIT without OFFSET should not trigger S002");
}

// ── S006: LIMIT without ORDER BY ──

#[test]
fn s006_limit_no_order() {
    let stmts = parse("SELECT * FROM t LIMIT 10");
    let w = lint(&stmts);
    assert!(has_rule(&w, "S006"), "expected S006 for LIMIT without ORDER BY");
}

#[test]
fn s006_limit_with_order_not_triggered() {
    let stmts = parse("SELECT * FROM t ORDER BY id LIMIT 10");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S006"), "LIMIT with ORDER BY should not trigger S006");
}

// ── S007: Explicit type for literals ──

#[test]
fn s007_string_literal_in_where_no_schema() {
    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S007"), "without schema, S007 should NOT warn — no evidence of type mismatch");
}

#[test]
fn s007_varchar_column_same_family_no_warn() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("name".to_string(), "varchar(100)".to_string());
    cols.insert("age".to_string(), "integer".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "S007"), "varchar column vs string literal is same-family, should not warn");
}

#[test]
fn s007_int_column_cross_family_warns() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("name".to_string(), "varchar(100)".to_string());
    cols.insert("age".to_string(), "integer".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE age = '30'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(has_rule(&w, "S007"), "integer column vs string literal is cross-family, should warn");
}

#[test]
fn s007_text_column_same_family_no_warn() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("description".to_string(), "text".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE description = 'hello'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "S007"), "text column vs string literal is same-family, should not warn");
}

#[test]
fn s007_qualified_column_varchar_no_warn() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("pro_id".to_string(), "character varying(10)".to_string());
    schema.insert("file_info".to_string(), cols);

    let stmts = parse("DELETE FROM file_info WHERE pro_id = '10'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "S007"), "character varying column vs string literal is same-family, should not warn");
}

#[test]
fn s007_delete_without_schema_still_warns() {
    let stmts = parse("DELETE FROM file_info WHERE pro_id = '10'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "S007"), "without schema, S007 should NOT warn — no evidence of type mismatch");
}

#[test]
fn s007_numeric_column_cross_family_warns() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("amount".to_string(), "numeric(10,2)".to_string());
    schema.insert("orders".to_string(), cols);

    let stmts = parse("SELECT * FROM orders WHERE amount = '100.00'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(has_rule(&w, "S007"), "numeric column vs string literal is cross-family, should warn");
}

#[test]
fn s007_unresolved_column_still_warns() {
    use crate::analyzer::schema::SchemaMap;
    let schema: SchemaMap = std::collections::HashMap::new();

    let stmts = parse("SELECT * FROM unknown_table WHERE col = 'val'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(
        !has_rule(&w, "S007"),
        "unresolvable column with schema present should NOT warn — no evidence of type mismatch"
    );
}

#[test]
fn s007_table_alias_varchar_no_warn() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("status".to_string(), "varchar(20)".to_string());
    cols.insert("id".to_string(), "integer".to_string());
    schema.insert("users".to_string(), cols);

    let stmts = parse("SELECT * FROM users u WHERE u.status = 'active'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "S007"), "aliased varchar column vs string literal should not warn");
}

#[test]
fn s007_table_alias_int_cross_family_warns() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("status".to_string(), "varchar(20)".to_string());
    cols.insert("id".to_string(), "integer".to_string());
    schema.insert("users".to_string(), cols);

    let stmts = parse("SELECT * FROM users u WHERE u.id = '42'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(has_rule(&w, "S007"), "aliased integer column vs string literal should warn");
}

// ── S008: Complex SQL ──

#[test]
fn s008_complex_sql() {
    let long_sql = format!("SELECT {} FROM t", "a, ".repeat(500));
    let stmts = parse(&long_sql);
    let config = LintConfig { sql_length_limit: 200, ..LintConfig::default() };
    let linter = SqlLinter::with_default_rules(config);
    let w = linter.lint(&stmts, None, Confidence::Full);
    assert!(has_rule(&w, "S008"), "expected S008 for long SQL");
}

// ── R005: Schema-aware implicit type conversion ──

#[test]
fn r005_varchar_column_string_literal_no_warn_with_schema() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("name".to_string(), "varchar(100)".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "R005"), "varchar column vs string literal is same-family, should not warn with schema");
}

#[test]
fn r005_int_column_string_literal_warns_with_schema() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("age".to_string(), "integer".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE age = '30'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(has_rule(&w, "R005"), "integer column vs string literal is cross-family, should warn even with schema");
}

#[test]
fn r005_int_column_int_literal_no_warn_with_schema() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("age".to_string(), "integer".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("SELECT * FROM t WHERE age = 30");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "R005"), "integer column vs integer literal is same-family, should not warn with schema");
}

#[test]
fn r005_no_schema_skips_entirely() {
    let stmts = parse("SELECT * FROM t WHERE name = 'abc'");
    let w = lint(&stmts);
    assert!(
        !has_rule(&w, "R005"),
        "without schema, R005 must not warn — avoiding false positives on legitimate col = literal (issue #240)"
    );
}

#[test]
fn r005_update_with_schema_same_family() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("status".to_string(), "varchar(20)".to_string());
    schema.insert("t".to_string(), cols);

    let stmts = parse("UPDATE t SET status = 'active' WHERE status = 'inactive'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(!has_rule(&w, "R005"), "UPDATE: varchar column vs string literal is same-family, should not warn");
}

#[test]
fn r005_delete_with_schema_same_family() {
    use crate::analyzer::schema::SchemaMap;
    let mut schema: SchemaMap = std::collections::HashMap::new();
    let mut cols = std::collections::HashMap::new();
    cols.insert("pro_id".to_string(), "character varying(10)".to_string());
    schema.insert("file_info".to_string(), cols);

    let stmts = parse("DELETE FROM file_info WHERE pro_id = '10'");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, Some(&schema), Confidence::Full);
    assert!(
        !has_rule(&w, "R005"),
        "DELETE: character varying column vs string literal is same-family, should not warn"
    );
}

// ── R006: Safe function whitelist ──

#[test]
fn r006_coalesce_on_column_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE COALESCE(name, 'default') = 'abc'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R006"), "COALESCE(col, ...) is a safe function, should not trigger R006");
}

#[test]
fn r006_nvl_on_column_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE NVL(name, 'x') = 'abc'");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R006"), "NVL(col, ...) is a safe function, should not trigger R006");
}

#[test]
fn r006_upper_on_column_triggered() {
    let stmts = parse("SELECT * FROM t WHERE UPPER(name) = 'ABC'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R006"), "UPPER(col) is not a safe function, should trigger R006");
}

#[test]
fn r006_greatest_on_column_not_triggered() {
    let stmts = parse("SELECT * FROM t WHERE GREATEST(a, b) > 10");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R006"), "GREATEST is a safe function, should not trigger R006");
}

#[test]
fn r006_substr_on_column_now_detected() {
    // Comma-syntax substr became FunctionCall (#256), so R006 now detects it
    // (previously invisible because substr produced SpecialFunction).
    let stmts = parse("SELECT * FROM t WHERE substr(name, 1, 5) = 'abc'");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R006"), "substr(col, ...) should trigger R006 after AST change");
}

// ── R008: Downgraded to Caution level ──

#[test]
fn r008_same_table_columns_is_caution_level() {
    let stmts = parse("SELECT * FROM t WHERE t.a > t.b");
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let w = linter.lint(&stmts, None, Confidence::Full);
    let r008 = w.iter().find(|w| w.rule_id == "R008");
    assert!(r008.is_some(), "expected R008 for t.a > t.b");
    assert_eq!(r008.unwrap().level, WarningLevel::Caution, "R008 should be Caution level, not Prohibition");
}

#[test]
fn r008_filtered_by_min_level_caution() {
    let stmts = parse("SELECT * FROM t WHERE t.a > t.b");
    let w = lint_min_level(&stmts, WarningLevel::Performance);
    assert!(!has_rule(&w, "R008"), "R008 at Caution level should be filtered by Performance min_level");
}

// ── LintConfig from_toml_str ──

#[cfg(feature = "lint-config")]
#[test]
fn from_toml_str_partial() {
    let toml = r#"
min_level = "caution"
suppress = ["R001"]
"#;
    let config = LintConfig::from_toml_str(toml).unwrap();
    assert_eq!(config.min_level, WarningLevel::Caution);
    assert_eq!(config.min_confidence, Confidence::Partial); // default
    assert_eq!(config.suppress, vec!["R001".to_string()]);
    assert_eq!(config.in_list_threshold, 500); // default
}

#[cfg(feature = "lint-config")]
#[test]
fn from_toml_str_full() {
    let toml = r#"
min_level = "performance"
min_confidence = "full"
suppress = ["R001", "P003", "S007"]
in_list_threshold = 100
subquery_depth_limit = 5
sql_length_limit = 1000
non_equi_join_limit = 4
group_by_column_limit = 20
max_insert_values_rows = 200
"#;
    let config = LintConfig::from_toml_str(toml).unwrap();
    assert_eq!(config.min_level, WarningLevel::Performance);
    assert_eq!(config.min_confidence, Confidence::Full);
    assert_eq!(config.suppress, vec!["R001", "P003", "S007"]);
    assert_eq!(config.in_list_threshold, 100);
    assert_eq!(config.subquery_depth_limit, 5);
    assert_eq!(config.sql_length_limit, 1000);
    assert_eq!(config.non_equi_join_limit, 4);
    assert_eq!(config.group_by_column_limit, 20);
    assert_eq!(config.max_insert_values_rows, 200);
}

#[cfg(feature = "lint-config")]
#[test]
fn from_toml_str_invalid_toml() {
    let err = LintConfig::from_toml_str("this is not toml {{{").unwrap_err();
    assert!(!err.is_empty(), "expected error for invalid TOML");
}

#[cfg(feature = "lint-config")]
#[test]
fn from_toml_str_unknown_field() {
    // Unknown fields should be silently ignored by serde
    let toml = r#"
min_level = "caution"
unknown_field = "whatever"
"#;
    let config = LintConfig::from_toml_str(toml).unwrap();
    assert_eq!(config.min_level, WarningLevel::Caution);
    // other fields should still have defaults
    assert_eq!(config.min_confidence, Confidence::Partial);
}

#[cfg(feature = "lint-config")]
#[test]
fn from_toml_str_empty() {
    // Empty TOML should produce default config
    let config = LintConfig::from_toml_str("").unwrap();
    assert_eq!(config.min_level, WarningLevel::Suggestion);
    assert_eq!(config.min_confidence, Confidence::Partial);
}

// ════════════════════════════════════════════════════════════════
// Regression tests for issue #246: double-iteration bug
// ════════════════════════════════════════════════════════════════
//
// Issue #246: SqlLinter::lint had outer loop `for info in stmts` × inner loop
// `for info in stmts` inside each check_fn. For N statements, every warning
// was produced N times (N×M instead of M). PR1 fixed this by passing
// `curr_stmt: &StatementInfo` as first param and removing the inner loop.
//
// These tests lock in the fix: if anyone reintroduces the double-iteration,
// asserting exact counts will fail.

#[test]
fn test_s006_multi_statement_no_duplicate_warnings_issue_246() {
    // N=5 statements, M=2 have LIMIT without ORDER BY (stmts 1 and 3).
    // Without fix: count = N×M = 5×2 = 10. With fix: count = M = 2.
    let sql = "\
        SELECT * FROM t1 LIMIT 5;
        SELECT id FROM t2 WHERE id = 1;
        SELECT * FROM t3 LIMIT 10;
        SELECT id FROM t4 ORDER BY id;
        SELECT id FROM t5 ORDER BY id LIMIT 1";
    let stmts = parse(sql);
    let w = lint(&stmts);
    let count = w.iter().filter(|w| w.rule_id == "S006").count();
    assert_eq!(count, 2, "without the fix for issue #246, count would be 5×2=10 (N×M)");
}

#[test]
fn test_multiple_rules_multi_statement_no_duplication_issue_246() {
    let sql = "\
        SELECT * FROM t1;
        DELETE FROM t2;
        SELECT id FROM t3 LIMIT 5;
        SELECT id FROM t4 WHERE status = 'active' ORDER BY id";
    let stmts = parse(sql);
    let w = lint(&stmts);
    assert_eq!(w.iter().filter(|w| w.rule_id == "R001").count(), 1, "R001 should fire once for standalone SELECT *");
    assert_eq!(
        w.iter().filter(|w| w.rule_id == "S006").count(),
        1,
        "without fix for issue #246, S006 would fire 4 times"
    );
    assert_eq!(
        w.iter().filter(|w| w.rule_id == "S001").count(),
        1,
        "without fix for issue #246, S001 would fire 4 times"
    );
}

#[test]
fn test_s006_single_statement_still_warns_issue_246() {
    // Single-statement sanity: fix must not break the simple case.
    let stmts = parse("SELECT * FROM t LIMIT 10");
    let w = lint(&stmts);
    assert_eq!(w.iter().filter(|w| w.rule_id == "S006").count(), 1);
}

#[test]
fn test_s006_large_n_no_quadratic_explosion_issue_246() {
    // N=20 identical statements, all with LIMIT without ORDER BY.
    // Without fix: count = 20×20 = 400. With fix: count = 20.
    let sql = (0..20).map(|i| format!("SELECT * FROM t{i} LIMIT 5")).collect::<Vec<_>>().join(";\n");
    let stmts = parse(&sql);
    let w = lint(&stmts);
    assert_eq!(
        w.iter().filter(|w| w.rule_id == "S006").count(),
        20,
        "without fix for issue #246, count would be 20×20=400"
    );
}

// ── R010: function-side-effect ──

#[test]
fn r010_insert_in_function_ok() {
    let stmts = parse(
        "CREATE OR REPLACE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN INSERT INTO t VALUES (1); END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with INSERT (no transaction) should not trigger R010");
}

#[test]
fn r010_update_in_function_ok() {
    let stmts =
        parse("CREATE OR REPLACE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN UPDATE t SET c = 1; END; $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with UPDATE (no transaction) should not trigger R010");
}

#[test]
fn r010_delete_in_function_ok() {
    let stmts =
        parse("CREATE OR REPLACE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN DELETE FROM t; END; $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with DELETE (no transaction) should not trigger R010");
}

#[test]
fn r010_merge_in_function_ok() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN MERGE INTO t1 USING t2 ON t1.id = t2.id WHEN MATCHED THEN UPDATE SET name = t2.name; END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with MERGE (no transaction) should not trigger R010");
}

#[test]
fn r010_select_only_ok() {
    let stmts = parse(
        "CREATE OR REPLACE FUNCTION fn() RETURNS SETOF t LANGUAGE plpgsql AS $$ BEGIN RETURN QUERY SELECT * FROM t; END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with only SELECT should not trigger R010");
}

#[test]
fn r010_perform_ok() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN PERFORM * FROM t; END; $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with only PERFORM should not trigger R010");
}

#[test]
fn r010_dml_in_nested_block_ok() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN BEGIN INSERT INTO t VALUES (1); END; END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with DML in nested block (no transaction) should not trigger R010");
}

#[test]
fn r010_dml_in_if_ok() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN IF 1 > 0 THEN INSERT INTO t VALUES (1); END IF; END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with DML in IF (no transaction) should not trigger R010");
}

#[test]
fn r010_commit_in_function_warns() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN INSERT INTO t VALUES (1); COMMIT; END; $$",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with COMMIT should trigger R010");
}

#[test]
fn r010_calls_procedure_with_tx_warns() {
    let stmts = parse(
        "CREATE FUNCTION inner_f() RETURNS void LANGUAGE plpgsql AS $$ BEGIN INSERT INTO t VALUES (1); COMMIT; END; $$;\
         CREATE FUNCTION outer_f() RETURNS void LANGUAGE plpgsql AS $$ BEGIN inner_f(); END; $$",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function calling function with transaction should trigger R010");
}

#[test]
fn r010_calls_safe_procedure_ok() {
    let stmts = parse(
        "CREATE PROCEDURE p_safe() LANGUAGE plpgsql AS $$ BEGIN PERFORM * FROM t; END; $$;\
         CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN CALL p_safe(); END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function calling safe procedure should not trigger R010");
}

#[test]
fn r010_procedure_with_tx_ok() {
    let stmts = parse("CREATE PROCEDURE p() LANGUAGE plpgsql AS $$ BEGIN INSERT INTO t VALUES (1); COMMIT; END; $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "procedure with DML and COMMIT should not trigger R010 (only functions)");
}

#[test]
fn r010_ddl_only_ok() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN CREATE TEMP TABLE tmp (id INTEGER); DROP TABLE tmp; END; $$",
    );
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with only DDL should not trigger R010");
}

#[test]
fn r010_truncate_in_function_ok() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN TRUNCATE TABLE t; END; $$");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R010"), "function with TRUNCATE (no transaction) should not trigger R010");
}

#[test]
fn r010_savepoint_in_function_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN SAVEPOINT sp1; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with SAVEPOINT should trigger R010");
}

#[test]
fn r010_release_savepoint_in_function_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN RELEASE SAVEPOINT sp1; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with RELEASE SAVEPOINT should trigger R010");
}

#[test]
fn r010_rollback_to_savepoint_warns() {
    let stmts =
        parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN ROLLBACK TO SAVEPOINT sp1; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with ROLLBACK TO SAVEPOINT should trigger R010");
}

#[test]
fn r010_set_transaction_warns() {
    let stmts = parse(
        "CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN SET TRANSACTION ISOLATION LEVEL SERIALIZABLE; END; $$",
    );
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with SET TRANSACTION should trigger R010");
}

#[test]
fn r010_start_transaction_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN START TRANSACTION; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with START TRANSACTION should trigger R010");
}

#[test]
fn r010_commit_only_no_dml_warns() {
    let stmts = parse("CREATE FUNCTION fn() RETURNS void LANGUAGE plpgsql AS $$ BEGIN COMMIT; END; $$");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R010"), "function with only COMMIT (no DML) should still trigger R010");
}

// ── R011: ORDER BY with DISTINCT ──

#[test]
fn r011_distinct_order_by_column_in_select() {
    let stmts = parse("SELECT DISTINCT a, b FROM t ORDER BY a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "ORDER BY a with DISTINCT a,b should NOT trigger R011");
}

#[test]
fn r011_distinct_order_by_column_not_in_select() {
    let stmts = parse("SELECT DISTINCT a FROM t ORDER BY b");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R011"), "ORDER BY b with DISTINCT a should trigger R011");
}

#[test]
fn r011_distinct_order_by_alias() {
    let stmts = parse("SELECT DISTINCT a AS x FROM t ORDER BY x");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "ORDER BY alias should NOT trigger R011");
}

#[test]
fn r011_distinct_order_by_positional() {
    let stmts = parse("SELECT DISTINCT a, b FROM t ORDER BY 1");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "ORDER BY 1 (positional) should NOT trigger R011");
}

#[test]
fn r011_distinct_order_by_positional_out_of_range() {
    let stmts = parse("SELECT DISTINCT a, b FROM t ORDER BY 3");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R011"), "ORDER BY 3 with only 2 columns should trigger R011");
}

#[test]
fn r011_distinct_no_order_by_not_triggered() {
    let stmts = parse("SELECT DISTINCT a FROM t");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "DISTINCT without ORDER BY should NOT trigger R011");
}

#[test]
fn r011_order_by_no_distinct_not_triggered() {
    let stmts = parse("SELECT a FROM t ORDER BY b");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "ORDER BY without DISTINCT should NOT trigger R011");
}

#[test]
fn r011_distinct_star_skipped() {
    // SELECT * expands to all columns — can't determine without schema
    let stmts = parse("SELECT DISTINCT * FROM t ORDER BY a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R011"), "SELECT DISTINCT * should be skipped (no false positive)");
}

// ── R012: ORDER BY with GROUP BY ──

#[test]
fn r012_group_by_order_by_column_in_select() {
    let stmts = parse("SELECT a, b FROM t GROUP BY a, b ORDER BY a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "ORDER BY a with GROUP BY a,b should NOT trigger R012");
}

#[test]
fn r012_group_by_order_by_column_not_in_select() {
    let stmts = parse("SELECT a FROM t GROUP BY a ORDER BY b");
    let w = lint(&stmts);
    assert!(has_rule(&w, "R012"), "ORDER BY b with SELECT a should trigger R012");
}

#[test]
fn r012_group_by_order_by_alias() {
    let stmts = parse("SELECT a AS x, b FROM t GROUP BY a, b ORDER BY x");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "ORDER BY alias should NOT trigger R012");
}

#[test]
fn r012_group_by_order_by_positional() {
    let stmts = parse("SELECT a, b FROM t GROUP BY a, b ORDER BY 2");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "ORDER BY 2 (positional) should NOT trigger R012");
}

#[test]
fn r012_group_by_no_order_by_not_triggered() {
    let stmts = parse("SELECT a FROM t GROUP BY a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "GROUP BY without ORDER BY should NOT trigger R012");
}

#[test]
fn r012_order_by_no_group_by_not_triggered() {
    let stmts = parse("SELECT a FROM t ORDER BY a");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "ORDER BY without GROUP BY should NOT trigger R012");
}

#[test]
fn r012_group_by_star_skipped() {
    let stmts = parse("SELECT * FROM t GROUP BY a ORDER BY b");
    let w = lint(&stmts);
    assert!(!has_rule(&w, "R012"), "SELECT * with GROUP BY should be skipped (no false positive)");
}
