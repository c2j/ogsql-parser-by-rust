use crate::ast::plpgsql::PlStatement;
use crate::ast::{Expr, InsertSource, SelectStatement, SelectTarget, SetOperation, Statement, StatementInfo, TableRef};
use crate::linter::{
    collect_selects_from_stmt, extract_where, literals_equal, loc_from_spanned, make_warning, stmt_location, walk_expr,
    walk_select_exprs, Confidence, LintConfig, LintRuleEntry, SqlLinter, SqlWarning, StatementKind, WarningLevel,
};
use crate::token::SourceLocation;

pub fn register(linter: &mut SqlLinter) {
    let rules: Vec<LintRuleEntry> = vec![
        LintRuleEntry {
            id: "P001",
            name: "union-without-all",
            description: "UNION without ALL performs unnecessary deduplication sorting",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p001,
        },
        LintRuleEntry {
            id: "P002",
            name: "not-in-subquery",
            description:
                "NOT IN (subquery) has poor performance and NULL value handling issues; use NOT EXISTS instead",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p002,
        },
        LintRuleEntry {
            id: "P003",
            name: "in-list-too-large",
            description: "Oversized IN lists cause slow parsing and poor performance; consider JOIN or temp table",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p003,
        },
        LintRuleEntry {
            id: "P004",
            name: "or-to-union-all",
            description: "Top-level OR in WHERE may cause the optimizer to abandon index usage; consider UNION ALL",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p004,
        },
        LintRuleEntry {
            id: "P005",
            name: "now-function-non-pushable",
            description: "now() and similar timestamp functions are non-pushable in distributed queries",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p005,
        },
        LintRuleEntry {
            id: "P006",
            name: "count-star-large-table",
            description: "COUNT(*) on large tables has poor performance; consider approximate statistics",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p006,
        },
        LintRuleEntry {
            id: "P007",
            name: "too-many-non-equi-joins",
            description: "Excessive non-equi join conditions degrade query performance",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p007,
        },
        LintRuleEntry {
            id: "P008",
            name: "group-by-without-hashagg",
            description: "GROUP BY without hash_agg hint may require work_mem tuning",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p008,
        },
        LintRuleEntry {
            id: "P009",
            name: "function-instead-of-case",
            description: "NVL/NVL2/DECODE/IIF functions can be replaced with CASE expression for better performance",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p009,
        },
        LintRuleEntry {
            id: "P010",
            name: "multi-column-update-subquery",
            description: "Multi-column UPDATE from subquery is inefficient; consider JOIN-based UPDATE",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Update,
            check_fn: check_p010,
        },
        LintRuleEntry {
            id: "P011",
            name: "correlated-subquery",
            description:
                "Correlated subquery executes once per outer row causing N+1 performance issues; consider JOIN",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p011,
        },
        LintRuleEntry {
            id: "P012",
            name: "unnecessary-distinct",
            description: "DISTINCT may be unnecessary if the result set is already unique; verify with schema info",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p012,
        },
        LintRuleEntry {
            id: "P013",
            name: "cartesian-product",
            description: "Missing join condition or explicit CROSS JOIN produces a Cartesian product",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_p013,
        },
        LintRuleEntry {
            id: "P014",
            name: "deeply-nested-subquery",
            description: "Deeply nested subqueries degrade performance; consider CTE or temp table",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p014,
        },
        LintRuleEntry {
            id: "P015",
            name: "range-equals-same-value",
            description: "BETWEEN with equal upper and lower bounds should be simplified to an equality condition",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p015,
        },
        LintRuleEntry {
            id: "P016",
            name: "update-from-no-join-condition",
            description: "UPDATE FROM without WHERE clause may produce unintended Cartesian product updates",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Update,
            check_fn: check_p016,
        },
        LintRuleEntry {
            id: "P017",
            name: "merge-without-unique-index",
            description: "MERGE statement ON condition requires a unique index for deterministic behavior",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Merge,
            check_fn: check_p017,
        },
        LintRuleEntry {
            id: "P018",
            name: "insert-select-no-columns",
            description: "INSERT ... SELECT without explicit target column list relies on column order",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Insert,
            check_fn: check_p018,
        },
        LintRuleEntry {
            id: "P019",
            name: "multi-table-update",
            description:
                "Multi-table UPDATE may produce unexpected results; consider splitting into single-table updates",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Update,
            check_fn: check_p019,
        },
        LintRuleEntry {
            id: "P020",
            name: "insert-all-multi-table",
            description: "INSERT ALL/FIRST multi-table insert should be evaluated for simplification to single INSERT",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::All,
            check_fn: check_p020,
        },
        LintRuleEntry {
            id: "P021",
            name: "row-by-row-insert-in-loop",
            description: "Row-by-row INSERT inside PL/pgSQL loop should use FORALL bulk operations",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_p021,
        },
        LintRuleEntry {
            id: "P022",
            name: "explain-in-production",
            description: "EXPLAIN statements should not appear in production code",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::All,
            check_fn: check_p022,
        },
        LintRuleEntry {
            id: "P023",
            name: "connect-by-performance",
            description: "CONNECT BY hierarchical queries may degrade severely with large datasets or deep recursion",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::All,
            check_fn: check_p023,
        },
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
        LintRuleEntry {
            id: "P025",
            name: "in-subquery-to-exists",
            description: "IN (subquery) may perform worse than EXISTS or JOIN; consider rewriting",
            // Suggestion per issue #300 (positive IN is a soft perf hint, unlike
            // NOT IN); P-prefix kept per issue's requested ID despite the level.
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Dml,
            check_fn: check_p025,
        },
    ];
    for rule in rules {
        linter.register(rule);
    }
}

// P001: UNION without ALL
fn check_p001(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        check_select_union(s, loc, confidence, warnings);
    }
}

fn check_select_union(
    select: &crate::ast::SelectStatement,
    loc: crate::token::SourceLocation,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Some(SetOperation::Union { all: false, .. }) = &select.set_operation {
        warnings.push(make_warning(
            WarningLevel::Performance, "P001", "union-without-all",
            "UNION \u{672a}\u{4f7f}\u{7528} ALL\u{ff0c}\u{5b58}\u{5728}\u{4e0d}\u{5fc5}\u{8981}\u{7684}\u{53bb}\u{91cd}\u{6392}\u{5e8f}".into(),
            Some("\u{5982}\u{679c}\u{786e}\u{8ba4}\u{65e0}\u{91cd}\u{53e0}\u{ff0c}\u{6539}\u{7528} UNION ALL"), loc,
            None, confidence,
        ));
    }
    if let Some(ref set_op) = select.set_operation {
        match set_op {
            SetOperation::Union { right, .. } => {
                check_select_union(right, loc, confidence, warnings);
            }
            SetOperation::Intersect { right, .. } => {
                check_select_union(right, loc, confidence, warnings);
            }
            SetOperation::Except { right, .. } => {
                check_select_union(right, loc, confidence, warnings);
            }
        }
    }
    if let Some(ref w) = select.with {
        for cte in &w.ctes {
            check_select_union(&cte.query, loc, confidence, warnings);
        }
    }
}

// P002: NOT IN (subquery) -- use NOT EXISTS instead
fn check_p002(
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
            Expr::InSubquery { negated: true, .. } => {
                warnings.push(make_warning(
                            WarningLevel::Performance, "P002", "not-in-subquery",
                            "NOT IN (\u{5b50}\u{67e5}\u{8be2}) \u{6027}\u{80fd}\u{8f83}\u{5dee}\u{ff0c}\u{5e76}\u{4e14} NULL \u{503c}\u{4f1a}\u{5bfc}\u{81f4}\u{7ed3}\u{679c}\u{4e0d}\u{7b26}\u{9884}\u{671f}".into(),
                            Some("\u{6539}\u{4e3a} NOT EXISTS"), loc,
                            None, confidence,
                        ));
                false
            }
            Expr::ScalarSublink { op, sublink_type, .. } => {
                let is_not_in = op == "<>" || op == "!=" || op == "NOT IN";
                if is_not_in && matches!(sublink_type, crate::ast::ScalarSublinkType::All) {
                    warnings.push(make_warning(
                                WarningLevel::Performance, "P002", "not-in-subquery",
                                "\u{6807}\u{91cf}\u{5b50}\u{94fe}\u{63a5} NOT IN/<> ALL \u{6a21}\u{5f0f}\u{ff0c}\u{6027}\u{80fd}\u{8f83}\u{5dee}".into(),
                                Some("\u{6539}\u{4e3a} NOT EXISTS"), loc,
                                None, confidence,
                            ));
                }
                true
            }
            Expr::InList { negated: true, list, .. } if list.len() > 10 => {
                warnings.push(make_warning(
                    WarningLevel::Performance,
                    "P002",
                    "not-in-subquery",
                    format!(
                        "NOT IN \u{5217}\u{8868}\u{542b} {} \u{4e2a}\u{503c}\u{ff0c}\u{6027}\u{80fd}\u{8f83}\u{5dee}",
                        list.len()
                    ),
                    Some("\u{6539}\u{4e3a} NOT EXISTS \u{6216} LEFT JOIN ... WHERE ... IS NULL"),
                    loc,
                    None,
                    confidence,
                ));
                true
            }
            _ => true,
        });
    }
}

// P003: IN list too large
fn check_p003(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Some(where_clause) = extract_where(&curr_stmt.statement) {
        walk_expr(where_clause, &mut |e| {
            if let Expr::InList { list, .. } = e {
                if list.len() > config.in_list_threshold {
                    warnings.push(make_warning(
                        WarningLevel::Performance, "P003", "in-list-too-large",
                        format!("IN \u{5217}\u{8868}\u{542b} {} \u{4e2a}\u{503c}\u{ff0c}\u{8d85}\u{8fc7}\u{9608}\u{503c} {}\u{ff0c}\u{5bfc}\u{81f4}\u{89e3}\u{6790}\u{7f13}\u{6162}", list.len(), config.in_list_threshold),
                        Some("\u{6539}\u{7528} INNER JOIN \u{6216}\u{4e34}\u{65f6}\u{8868}"), loc,
                        None, confidence,
                    ));
                }
            }
            true
        });
    }
}

// P004: OR as top-level WHERE condition
fn check_p004(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Some(Expr::BinaryOp { op, .. }) = extract_where(&curr_stmt.statement) {
        if op.eq_ignore_ascii_case("OR") {
            warnings.push(make_warning(
                WarningLevel::Performance, "P004", "or-to-union-all",
                "WHERE \u{9876}\u{5c42}\u{4e3a} OR \u{6761}\u{4ef6}\u{ff0c}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{4f18}\u{5316}\u{5660}\u{653e}\u{5f03}\u{7d22}\u{5f15}".into(),
                Some("\u{8003}\u{8651}\u{5c06} OR \u{6539}\u{5199}\u{4e3a} UNION ALL"), loc,
                None, confidence,
            ));
        }
    }
}

// P005: now() function -- non-pushable in distributed queries
fn check_p005(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let mut check_fn = |e: &Expr| {
        if let Expr::FunctionCall { name, .. } = e {
            let fn_name = name.last().map(|s| s.to_lowercase()).unwrap_or_default();
            if fn_name == "now" || fn_name == "current_timestamp" || fn_name == "sysdate" {
                warnings.push(make_warning(
                    WarningLevel::Performance, "P005", "now-function-non-pushable",
                    format!("\u{51fd}\u{6570} {fn_name}() \u{4e0d}\u{53ef}\u{4e0b}\u{63a8}\u{ff0c}\u{5c06}\u{5bfc}\u{81f4}\u{5206}\u{5e03}\u{5f0f}\u{67e5}\u{8be2}\u{6027}\u{80fd}\u{4e0b}\u{964d}"),
                    Some("\u{7528}\u{65f6}\u{95f4}\u{5b8f}\u{6216}\u{53c2}\u{6570}\u{5316}\u{67e5}\u{8be2}\u{66ff}\u{4ee3}"), loc,
                    Some("SQL \u{67e5}\u{8be2}\u{6700}\u{4f73}\u{5b9e}\u{8df5}"), confidence,
                ));
            }
        }
    };
    match &curr_stmt.statement {
        Statement::Select(s) => walk_select_exprs(s, &mut |e| {
            check_fn(e);
            true
        }),
        Statement::Update(s) => {
            for a in &s.assignments {
                walk_expr(&a.value, &mut |e| {
                    check_fn(e);
                    true
                });
            }
            if let Some(ref w) = s.where_clause {
                walk_expr(w, &mut |e| {
                    check_fn(e);
                    true
                });
            }
        }
        Statement::Delete(s) => {
            if let Some(ref w) = s.where_clause {
                walk_expr(w, &mut |e| {
                    check_fn(e);
                    true
                });
            }
        }
        _ => {}
    }
}

// P006: COUNT(*) on large table
fn check_p006(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        walk_select_exprs(s, &mut |e| {
            if let Expr::FunctionCall { name, args, .. } = e {
                let fn_name = name.last().map(|s| s.to_lowercase()).unwrap_or_default();
                if fn_name == "count" {
                    let is_star = args.iter().any(|a| {
                        matches!(a, Expr::ColumnRef(n) if n.len() == 1 && n[0] == "*")
                            || matches!(a, Expr::QualifiedStar(_))
                    });
                    if is_star || args.is_empty() {
                        warnings.push(make_warning(
                            WarningLevel::Performance, "P006", "count-star-large-table",
                            "COUNT(*) \u{5728}\u{5927}\u{8868}\u{4e0a}\u{6027}\u{80fd}\u{8f83}\u{5dee}".into(),
                            Some("\u{8003}\u{8651}\u{4f7f}\u{7528} pg_class.reltuples \u{6216}\u{8fd1}\u{4f3c}\u{7edf}\u{8ba1}\u{4fe1}\u{606f}"), loc,
                            None, confidence,
                        ));
                    }
                }
            }
            true
        });
    }
}

// P007: Too many non-equi joins
fn check_p007(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        let mut count = 0usize;
        count_non_equi_joins_in_from(&s.from, &mut count);
        if count > config.non_equi_join_limit {
            warnings.push(make_warning(
                WarningLevel::Performance, "P007", "too-many-non-equi-joins",
                format!("\u{975e}\u{7b49}\u{503c} JOIN \u{6761}\u{4ef6} {count} \u{4e2a}\u{ff0c}\u{8d85}\u{8fc7}\u{9608}\u{503c} {}\u{ff0c}\u{6027}\u{80fd}\u{8f83}\u{5dee}", config.non_equi_join_limit),
                Some("\u{4f18}\u{5148}\u{4f7f}\u{7528}\u{7b49}\u{503c}\u{67e5}\u{8be2}"), loc,
                None, confidence,
            ));
        }
    }
}

fn count_non_equi_joins_in_from(from: &[TableRef], count: &mut usize) {
    for t in from {
        if let TableRef::Join { condition: Some(cond), .. } = t {
            count_non_equi_in_expr(cond, count);
        }
    }
}

fn count_non_equi_in_expr(expr: &Expr, count: &mut usize) {
    if let Expr::BinaryOp { op, left, right, .. } = expr {
        let is_equi = op == "=" || op.eq_ignore_ascii_case("is") || op.eq_ignore_ascii_case("is not");
        if !is_equi {
            *count += 1;
        }
        count_non_equi_in_expr(left, count);
        count_non_equi_in_expr(right, count);
    }
}

// P008: GROUP BY without hash_agg hint
fn check_p008(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if !s.group_by.is_empty() {
            let has_hashagg = s
                .hints
                .iter()
                .any(|h| h.name.to_lowercase().contains("hashagg") || h.name.to_lowercase() == "use_hash_agg");
            if !has_hashagg {
                let loc = loc_from_spanned(s, stmt_location(curr_stmt));
                warnings.push(make_warning(
                    WarningLevel::Performance, "P008", "group-by-without-hashagg",
                    "GROUP BY \u{64cd}\u{4f5c}\u{672a}\u{4f7f}\u{7528} hash_agg \u{63d0}\u{793a}\u{ff0c}\u{53ef}\u{80fd}\u{9700}\u{8981}\u{8c03}\u{5927} work_mem".into(),
                    Some("\u{8003}\u{8651}\u{6dfb}\u{52a0} /*+ hash_agg */ \u{63d0}\u{793a}\u{6216}\u{8c03}\u{5927} work_mem"), loc,
                    None, confidence,
                ));
            }
        }
    }
}

// P010: Multi-column UPDATE from subquery
fn check_p010(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Update(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        for assignment in &s.assignments {
            if assignment.columns.len() > 1 && matches!(&assignment.value, Expr::Subquery(_)) {
                warnings.push(make_warning(
                    WarningLevel::Performance,
                    "P010",
                    "multi-column-update-subquery",
                    format!(
                        "UPDATE SET ({}\u{5217}) = (SELECT ...) \u{6548}\u{7387}\u{8f83}\u{4f4e}",
                        assignment.columns.len()
                    ),
                    Some("\u{6539}\u{7528} UPDATE ... FROM ... WHERE ... \u{7684} JOIN \u{98ce}\u{683c}"),
                    loc,
                    None,
                    confidence,
                ));
            }
        }
    }
}

// P011: Correlated subquery (basic AST-only detection)
fn check_p011(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let outer_tables = collect_from_tables(&curr_stmt.statement);
    if outer_tables.is_empty() {
        return;
    }
    if let Some(where_clause) = extract_where(&curr_stmt.statement) {
        let mut found = false;
        walk_expr(where_clause, &mut |e| {
            if found {
                return false;
            }
            match e {
                Expr::InSubquery { subquery, .. } | Expr::Exists(subquery) | Expr::Subquery(subquery) => {
                    if has_correlated_ref(&subquery.where_clause, &outer_tables) {
                        found = true;
                        return false;
                    }
                }
                Expr::ScalarSublink { subquery, .. } if has_correlated_ref(&subquery.where_clause, &outer_tables) => {
                    found = true;
                    return false;
                }
                _ => {}
            }
            true
        });
        if found {
            warnings.push(make_warning(
                WarningLevel::Performance, "P011", "correlated-subquery",
                "\u{5173}\u{8054}\u{5b50}\u{67e5}\u{8be2}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{6bcf}\u{884c}\u{6267}\u{884c}\u{4e00}\u{6b21}\u{5b50}\u{67e5}\u{8be2}".into(),
                Some("\u{6539}\u{5199}\u{4e3a}\u{7b49}\u{503c} JOIN"), loc,
                None, confidence,
            ));
        }
    }
}

fn collect_from_tables(stmt: &Statement) -> Vec<String> {
    let mut tables = Vec::new();
    match stmt {
        Statement::Select(s) => collect_table_names(&s.from, &mut tables),
        Statement::Update(s) => {
            collect_table_names(&s.tables, &mut tables);
            collect_table_names(&s.from, &mut tables);
        }
        Statement::Delete(s) => {
            collect_table_names(&s.tables, &mut tables);
        }
        _ => {}
    }
    tables
}

fn collect_table_names(from: &[TableRef], tables: &mut Vec<String>) {
    for t in from {
        match t {
            TableRef::Table { name, alias, .. } => {
                let tbl_name = name.last().cloned().unwrap_or_default();
                tables.push(tbl_name.to_lowercase());
                if let Some(a) = alias {
                    tables.push(a.to_lowercase());
                }
            }
            TableRef::Join { left, right, .. } => {
                collect_table_names(std::slice::from_ref(left), tables);
                collect_table_names(std::slice::from_ref(right), tables);
            }
            TableRef::Subquery { alias: Some(a), .. } => {
                tables.push(a.to_lowercase());
            }
            _ => {}
        }
    }
}

fn has_correlated_ref(expr: &Option<Expr>, outer_tables: &[String]) -> bool {
    let Some(expr) = expr else { return false };
    let mut found = false;
    walk_expr(expr, &mut |e| {
        if found {
            return false;
        }
        if let Expr::ColumnRef(name) = e {
            if name.len() >= 2 {
                let table_part = &name[0];
                if outer_tables.contains(&table_part.to_lowercase()) {
                    found = true;
                    return false;
                }
            }
        }
        true
    });
    found
}

// P012: Unnecessary DISTINCT (basic -- warns on all DISTINCT)
fn check_p012(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if s.distinct {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Performance, "P012", "unnecessary-distinct",
                "DISTINCT \u{5b58}\u{5728}\u{ff0c}\u{9700}\u{7ed3}\u{5408}\u{552f}\u{4e00}\u{952e}\u{5224}\u{65ad}\u{662f}\u{5426}\u{5fc5}\u{8981}\u{ff08}\u{9700} schema \u{4fe1}\u{606f}\u{8fdb}\u{4e00}\u{6b65}\u{786e}\u{8ba4}\u{ff09}".into(),
                Some("\u{68c0}\u{67e5}\u{53bb}\u{91cd}\u{5fc5}\u{8981}\u{6027}\u{ff0c}\u{5982}\u{679c}\u{5df2}\u{542b}\u{552f}\u{4e00}\u{5217}\u{5219}\u{53ef}\u{79fb}\u{9664}"), loc,
                None, confidence,
            ));
        }
    }
}

// P013: Cartesian product (CROSS JOIN or missing join condition)
fn check_p013(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        check_cartesian_in_from(&s.from, loc, confidence, warnings);
    }
}

fn check_cartesian_in_from(
    from: &[TableRef],
    loc: crate::token::SourceLocation,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    for t in from {
        if let TableRef::Join { condition: None, natural: false, using_columns, join_type, .. } = t {
            if using_columns.is_empty() {
                warnings.push(make_warning(
                    WarningLevel::Performance, "P013", "cartesian-product",
                    match join_type {
                        crate::ast::JoinType::Cross => "CROSS JOIN \u{4ea7}\u{751f}\u{7b1b}\u{5361}\u{5c14}\u{79ef}".into(),
                        _ => "JOIN \u{7f3a}\u{5c11}\u{8fde}\u{63a5}\u{6761}\u{4ef6}\u{ff0c}\u{53ef}\u{80fd}\u{4ea7}\u{751f}\u{7b1b}\u{5361}\u{5c14}\u{79ef}".into(),
                    },
                    Some("\u{8865}\u{5145} JOIN \u{6761}\u{4ef6}\u{6216}\u{786e}\u{8ba4}\u{4e3a} CROSS JOIN"), loc,
                    None, confidence,
                ));
            }
        }
        match t {
            TableRef::Join { left, right, .. } => {
                check_cartesian_in_from(std::slice::from_ref(left), loc, confidence, warnings);
                check_cartesian_in_from(std::slice::from_ref(right), loc, confidence, warnings);
            }
            TableRef::Subquery { query, .. } => {
                check_cartesian_in_from(&query.from, loc, confidence, warnings);
            }
            _ => {}
        }
    }
}

// P014: Deeply nested subquery
fn check_p014(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let max_depth = match &curr_stmt.statement {
        Statement::Select(s) => subquery_depth_select(s),
        Statement::Update(u) => {
            let mut d = 0;
            for a in &u.assignments {
                d = d.max(subquery_depth_expr(&a.value));
            }
            if let Some(ref w) = u.where_clause {
                d = d.max(subquery_depth_expr(w));
            }
            d
        }
        Statement::Delete(de) => {
            if let Some(ref w) = de.where_clause {
                subquery_depth_expr(w)
            } else {
                0
            }
        }
        _ => 0,
    };
    if max_depth >= config.subquery_depth_limit {
        warnings.push(make_warning(
                WarningLevel::Performance, "P014", "deeply-nested-subquery",
                format!("\u{5b50}\u{67e5}\u{8be2}\u{5d4c}\u{5957}\u{6df1}\u{5ea6} {max_depth} \u{8d85}\u{8fc7}\u{9608}\u{503c} {}\u{ff0c}\u{6027}\u{80fd}\u{53ef}\u{80fd}\u{8f83}\u{5dee}", config.subquery_depth_limit),
                Some("\u{62c6}\u{5206}\u{4e3a}\u{4e34}\u{65f6}\u{8868}\u{6216} CTE"), loc,
                None, confidence,
            ));
    }
}

fn subquery_depth_select(s: &crate::ast::SelectStatement) -> usize {
    let mut max_d = 0;
    for t in &s.targets {
        if let SelectTarget::Expr(e, _) = t {
            max_d = max_d.max(subquery_depth_expr(e));
        }
    }
    if let Some(ref w) = s.where_clause {
        max_d = max_d.max(subquery_depth_expr(w));
    }
    if let Some(ref h) = s.having {
        max_d = max_d.max(subquery_depth_expr(h));
    }
    if let Some(ref so) = s.set_operation {
        let inner = match so {
            SetOperation::Union { right, .. }
            | SetOperation::Intersect { right, .. }
            | SetOperation::Except { right, .. } => subquery_depth_select(right),
        };
        max_d = max_d.max(inner);
    }
    if let Some(ref w) = s.with {
        for cte in &w.ctes {
            max_d = max_d.max(subquery_depth_select(&cte.query));
        }
    }
    for t in &s.from {
        max_d = max_d.max(subquery_depth_from(t));
    }
    max_d
}

fn subquery_depth_from(t: &TableRef) -> usize {
    match t {
        TableRef::Subquery { query, .. } => 1 + subquery_depth_select(query),
        TableRef::Join { left, right, condition, .. } => {
            let l = subquery_depth_from(left);
            let r = subquery_depth_from(right);
            let c = condition.as_ref().map_or(0, subquery_depth_expr);
            l.max(r).max(c)
        }
        _ => 0,
    }
}

fn subquery_depth_expr(expr: &Expr) -> usize {
    match expr {
        Expr::Subquery(s) | Expr::Exists(s) => 1 + subquery_depth_select(s),
        Expr::InSubquery { subquery, .. } => 1 + subquery_depth_select(subquery),
        Expr::ScalarSublink { subquery, .. } => 1 + subquery_depth_select(subquery),
        Expr::BinaryOp { left, right, .. } => subquery_depth_expr(left).max(subquery_depth_expr(right)),
        Expr::UnaryOp { expr, .. } => subquery_depth_expr(expr),
        Expr::FunctionCall { args, over, filter, within_group, .. } => {
            let mut d = 0;
            for a in args {
                d = d.max(subquery_depth_expr(a));
            }
            if let Some(o) = over {
                for e in &o.partition_by {
                    d = d.max(subquery_depth_expr(e));
                }
            }
            if let Some(f) = filter {
                d = d.max(subquery_depth_expr(f));
            }
            for item in within_group {
                d = d.max(subquery_depth_expr(&item.expr));
            }
            d
        }
        Expr::Case { operand, whens, else_expr } => {
            let mut d = 0;
            if let Some(o) = operand {
                d = d.max(subquery_depth_expr(o));
            }
            for w in whens {
                d = d.max(subquery_depth_expr(&w.condition)).max(subquery_depth_expr(&w.result));
            }
            if let Some(e) = else_expr {
                d = d.max(subquery_depth_expr(e));
            }
            d
        }
        Expr::Parenthesized(e) => subquery_depth_expr(e),
        _ => 0,
    }
}

// P015: Range equals same value (col >= x AND col <= x where x == x)
fn check_p015(
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
            if let Expr::Between { low, high, negated: false, .. } = e {
                if literals_equal(low, high) {
                    warnings.push(make_warning(
                        WarningLevel::Performance, "P015", "range-equals-same-value",
                        "BETWEEN \u{4e0a}\u{4e0b}\u{754c}\u{76f8}\u{540c}\u{ff0c}\u{5e94}\u{7b80}\u{5316}\u{4e3a} \u{7b49}\u{4e8e}\u{6761}\u{4ef6}".into(),
                        Some("\u{7b80}\u{5316}\u{4e3a} ="), loc,
                        None, confidence,
                    ));
                }
            }
            true
        });
    }
}

// P016: UPDATE FROM without join condition in WHERE
fn check_p016(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Update(s) = &curr_stmt.statement {
        if !s.from.is_empty() && s.where_clause.is_none() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Performance, "P016", "update-from-no-join-condition",
                "UPDATE FROM \u{65e0} WHERE \u{5b50}\u{53e5}\u{ff0c}\u{53ef}\u{80fd}\u{4ea7}\u{751f}\u{7b1b}\u{5361}\u{5c14}\u{79ef}\u{66f4}\u{65b0}".into(),
                Some("WHERE \u{4e2d}\u{5173}\u{8054} FROM \u{8868}"), loc,
                None, confidence,
            ));
        }
    }
}

// P017: MERGE without unique index (basic -- always warns)
fn check_p017(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Merge(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        warnings.push(make_warning(
            WarningLevel::Performance, "P017", "merge-without-unique-index",
            "MERGE \u{8bed}\u{53e5} ON \u{6761}\u{4ef6}\u{9700}\u{8981}\u{552f}\u{4e00}\u{7d22}\u{5f15}\u{4fdd}\u{8bc1}\u{786e}\u{5b9a}\u{6027}\u{ff08}\u{9700} schema \u{4fe1}\u{606f}\u{786e}\u{8ba4}\u{ff09}".into(),
            Some("\u{786e}\u{4fdd} ON \u{6761}\u{4ef6}\u{5217}\u{6709}\u{552f}\u{4e00}\u{7d22}\u{5f15}"), loc,
            None, confidence,
        ));
    }
}

// P018: INSERT SELECT without column list
fn check_p018(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Insert(s) = &curr_stmt.statement {
        if s.columns.is_empty() && matches!(&s.source, InsertSource::Select(_)) {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                    WarningLevel::Performance, "P018", "insert-select-no-columns",
                    "INSERT INTO ... SELECT \u{672a}\u{6307}\u{5b9a}\u{76ee}\u{6807}\u{5217}\u{540d}\u{ff0c}\u{4f9d}\u{8d56}\u{5217}\u{987a}\u{5e8f}".into(),
                    Some("\u{663e}\u{5f0f}\u{6307}\u{5b9a}\u{76ee}\u{6807}\u{5217}\u{540d}"), loc,
                    None, confidence,
                ));
        }
    }
}

// P019: Multi-table UPDATE
fn check_p019(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Update(s) = &curr_stmt.statement {
        if s.tables.len() > 1 {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Performance, "P019", "multi-table-update",
                format!("\u{591a}\u{8868} UPDATE \u{5305}\u{542b} {} \u{4e2a}\u{8868}\u{ff0c}\u{53ef}\u{80fd}\u{4ea7}\u{751f}\u{975e}\u{9884}\u{671f}\u{7ed3}\u{679c}", s.tables.len()),
                Some("\u{62c6}\u{5206}\u{4e3a}\u{591a}\u{6761}\u{5355}\u{8868} UPDATE"), loc,
                None, confidence,
            ));
        }
    }
}

// P020: INSERT ALL / INSERT FIRST multi-table
fn check_p020(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    match &curr_stmt.statement {
        Statement::InsertAll(s) => {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Performance, "P020", "insert-all-multi-table",
                "INSERT ALL \u{591a}\u{8868}\u{63d2}\u{5165}\u{ff0c}\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{7528}\u{5355}\u{6761} INSERT...SELECT".into(),
                Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{7528}\u{5355}\u{6761} INSERT...SELECT \u{66ff}\u{4ee3}"), loc,
                None, confidence,
            ));
        }
        Statement::InsertFirst(s) => {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Performance, "P020", "insert-all-multi-table",
                "INSERT FIRST \u{591a}\u{8868}\u{63d2}\u{5165}\u{ff0c}\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{7528}\u{5355}\u{6761} INSERT...SELECT".into(),
                Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{7528}\u{5355}\u{6761} INSERT...SELECT \u{66ff}\u{4ee3}"), loc,
                None, confidence,
            ));
        }
        _ => {}
    }
}

// P022: EXPLAIN in production code
fn check_p022(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Explain(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        warnings.push(make_warning(
            WarningLevel::Performance,
            "P022",
            "explain-in-production",
            "EXPLAIN \u{8bed}\u{53e5}\u{4e0d}\u{5e94}\u{51fa}\u{73b0}\u{5728}\u{751f}\u{4ea7}\u{4ee3}\u{7801}\u{4e2d}"
                .into(),
            Some("\u{79fb}\u{9664} EXPLAIN \u{6216}\u{4ec5}\u{7528}\u{4e8e}\u{8c03}\u{8bd5}"),
            loc,
            None,
            confidence,
        ));
    }
}

// ── P009: Function that should be CASE ──

const GENERIC_CASE_SUGGESTION: &str = "\u{4f7f}\u{7528} CASE WHEN ... THEN ... ELSE ... END \u{66ff}\u{4ee3}";

/// Build a targeted replacement suggestion for `NVL`/`NVL2`/`DECODE`/`IIF` calls.
/// Returns `None` when `lower_name` is not one of the functions P009 flags.
fn case_replacement_suggestion(lower_name: &str, args: &[Expr]) -> Option<String> {
    let formatter = crate::formatter::SqlFormatter::new();
    match lower_name {
        "nvl" => {
            if let [a, b] = args {
                Some(format!(
                    "\u{4f7f}\u{7528} COALESCE({}, {}) \u{66ff}\u{4ee3}",
                    formatter.render_expr(a),
                    formatter.render_expr(b)
                ))
            } else {
                Some(GENERIC_CASE_SUGGESTION.to_string())
            }
        }
        "nvl2" => {
            if let [cond, then_val, else_val] = args {
                Some(format!(
                    "\u{4f7f}\u{7528} CASE WHEN {} IS NOT NULL THEN {} ELSE {} END \u{66ff}\u{4ee3}",
                    formatter.render_expr(cond),
                    formatter.render_expr(then_val),
                    formatter.render_expr(else_val)
                ))
            } else {
                Some(GENERIC_CASE_SUGGESTION.to_string())
            }
        }
        "decode" | "iif" => Some(GENERIC_CASE_SUGGESTION.to_string()),
        _ => None,
    }
}

fn check_p009(
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
            if let Expr::FunctionCall { name, args, .. } = e {
                if let Some(fn_name) = name.last() {
                    let lower = fn_name.to_lowercase();
                    if let Some(suggestion) = case_replacement_suggestion(&lower, args) {
                        warnings.push(make_warning(
                            WarningLevel::Performance,
                            "P009",
                            "function-instead-of-case",
                            format!("\u{51fd}\u{6570} {fn_name}() \u{53ef}\u{4ee5}\u{7528} CASE \u{8868}\u{8fbe}\u{5f0f}\u{66ff}\u{4ee3}\u{ff0c}\u{53ef}\u{80fd}\u{66f4}\u{9ad8}\u{6548}"),
                            Some(&suggestion),
                            loc,
                            None,
                            confidence,
                        ));
                        return false;
                    }
                }
            }
            true
        });
    }
}

// ── P021: Row-by-row INSERT in PL/pgSQL loop ──

fn check_p021(
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
    walk_pl_for_loop_insert(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Performance,
            "P021",
            "row-by-row-insert-in-loop",
            "\u{5faa}\u{73af}\u{4f53}\u{5185}\u{5305}\u{542b} INSERT\u{ff0c}\u{5e94}\u{4f7f}\u{7528} FORALL \u{6279}\u{91cf}\u{64cd}\u{4f5c}\u{66ff}\u{4ee3}".into(),
            Some("\u{4f7f}\u{7528} FORALL \u{6279}\u{91cf}\u{63d2}\u{5165}\u{6216} INSERT ... SELECT \u{66ff}\u{4ee3}\u{5faa}\u{73af} INSERT"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_loop_insert(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_stmts_for_loop_insert(&block.body, found, false);
        if *found {
            return;
        }
    }
}

fn check_pl_stmts_for_loop_insert(pl_stmts: &[crate::ast::plpgsql::PlStatement], found: &mut bool, inside_loop: bool) {
    if *found {
        return;
    }
    for s in pl_stmts {
        match s {
            PlStatement::Loop(l) => {
                check_pl_stmts_for_loop_insert(&l.body, found, true);
            }
            PlStatement::While(w) => {
                check_pl_stmts_for_loop_insert(&w.body, found, true);
            }
            PlStatement::For(f) => {
                check_pl_stmts_for_loop_insert(&f.body, found, true);
            }
            PlStatement::ForEach(f) => {
                check_pl_stmts_for_loop_insert(&f.body, found, true);
            }
            PlStatement::SqlStatement { statement, .. } if inside_loop => {
                if matches!(statement.as_ref(), Statement::Insert(_)) {
                    *found = true;
                    return;
                }
            }
            PlStatement::Sql(_) if inside_loop => {
                // Raw SQL string — check if it looks like INSERT
                // (best-effort for unparsed SQL)
            }
            PlStatement::Block(b) => {
                check_pl_stmts_for_loop_insert(&b.body, found, inside_loop);
            }
            PlStatement::If(i) => {
                check_pl_stmts_for_loop_insert(&i.then_stmts, found, inside_loop);
                for e in &i.elsifs {
                    check_pl_stmts_for_loop_insert(&e.stmts, found, inside_loop);
                }
                check_pl_stmts_for_loop_insert(&i.else_stmts, found, inside_loop);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_loop_insert(&w.stmts, found, inside_loop);
                }
                check_pl_stmts_for_loop_insert(&c.else_stmts, found, inside_loop);
            }
            _ => {}
        }
        if *found {
            return;
        }
    }
}

// ── P023: CONNECT BY hierarchical query ──
//
// CONNECT BY queries can degrade severely with large datasets or deep
// recursion.  GaussDB's WITH RECURSIVE CTE often provides better optimization
// and clearer semantics for complex traversals.
fn check_p023(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let mut selects: Vec<(&SelectStatement, SourceLocation)> = Vec::new();
    collect_selects_from_stmt(&curr_stmt.statement, stmt_location(curr_stmt), &mut selects);
    for (s, loc) in selects {
        if s.connect_by.is_some() {
            let mut msg = String::from("CONNECT BY 层级查询在数据量大或递归层次深时性能可能显著下降");
            if s.connect_by.as_ref().is_some_and(|cb| cb.start_with.is_none()) {
                msg.push_str("；缺少 START WITH 可能导致全表扫描");
            }
            warnings.push(make_warning(
                WarningLevel::Performance,
                "P023",
                "connect-by-performance",
                msg,
                Some("考虑使用 WITH RECURSIVE CTE 替代，或在 START WITH 中限制起始行范围，在 CONNECT BY 条件中添加额外过滤，使用 NOCYCLE 避免死循环"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── P024: ROWNUM pseudo-column (Oracle pagination) → LIMIT/OFFSET or FETCH FIRST ──

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
                "ROWNUM \u{4e3a} Oracle \u{4e13}\u{6709}\u{5206}\u{9875}\u{8bed}\u{6cd5}\u{ff0c}\u{5efa}\u{8bae}\u{4f7f}\u{7528} LIMIT/OFFSET \u{6216} FETCH FIRST".into(),
                Some("\u{5efa}\u{8bae}\u{4f7f}\u{7528} LIMIT/OFFSET \u{6216} FETCH FIRST"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

fn is_rownum_ref(e: &Expr) -> bool {
    matches!(e, Expr::ColumnRef(name) if name.len() == 1 && name[0].eq_ignore_ascii_case("rownum"))
}

// ── P025: positive IN (subquery) → EXISTS or JOIN ──

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
