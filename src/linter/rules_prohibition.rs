use crate::ast::plpgsql::{PlBlock, PlStatement};
use crate::ast::{
    Expr, GroupByItem, InsertSource, PackageItem, SelectStatement, SelectTarget, Statement, StatementInfo,
};
use crate::linter::type_helpers::{
    build_column_type_map, classify_type_family, literal_type_family, resolve_column_type,
};
use crate::linter::{
    collect_inner_selects_only, collect_selects_from_stmt, loc_from_spanned, make_warning, stmt_location, walk_expr,
    Confidence, LintConfig, LintRuleEntry, SqlLinter, SqlWarning, StatementKind, WarningLevel,
};
use crate::token::SourceLocation;
use std::collections::HashSet;

pub fn register(linter: &mut SqlLinter) {
    let rules: Vec<LintRuleEntry> = vec![
        LintRuleEntry {
            id: "R001",
            name: "select-star",
            description: "SELECT * in nested queries only; outer-most SELECT * acceptable, nested subqueries must use explicit column lists",
            level: WarningLevel::Prohibition,
            // All (was Dml): also dispatch on PL-block statements so SELECT *
            // inside CREATE FUNCTION/PROCEDURE/PACKAGE BODY is caught (#296).
            stmt_kind: StatementKind::All,
            check_fn: check_r001,
        },
        LintRuleEntry {
            id: "R002",
            name: "large-column-sort",
            description: "Large GROUP BY or ORDER BY column counts may cause performance problems",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::Select,
            check_fn: check_r002,
        },
        LintRuleEntry {
            id: "R003",
            name: "lock-table",
            description: "LOCK TABLE may cause deadlock risks",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::All,
            check_fn: check_r003,
        },
        LintRuleEntry {
            id: "R004",
            name: "drop-cascade",
            description: "DROP ... CASCADE may accidentally drop dependent objects",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::All,
            check_fn: check_r004,
        },
        LintRuleEntry {
            id: "R005",
            name: "implicit-type-conversion",
            description: "Implicit type conversion in WHERE may cause index invalidation",
            level: WarningLevel::Prohibition,
            // All (was Dml): also dispatch on PL-block statements (#296).
            stmt_kind: StatementKind::All,
            check_fn: check_r005,
        },
        LintRuleEntry {
            id: "R006",
            name: "function-on-where-column",
            description: "Function wrapping column in WHERE clauses defeats index usage",
            level: WarningLevel::Prohibition,
            // All (was Dml): also dispatch on PL-block statements (#296).
            stmt_kind: StatementKind::All,
            check_fn: check_r006,
        },
        LintRuleEntry {
            id: "R007",
            name: "like-leading-wildcard",
            description: "LIKE with leading wildcard prevents index usage and triggers full table scan",
            level: WarningLevel::Prohibition,
            // All (was Dml): also dispatch on PL-block statements (#296).
            stmt_kind: StatementKind::All,
            check_fn: check_r007,
        },
        LintRuleEntry {
            id: "R008",
            name: "same-table-column-compare",
            description: "Same-table column comparison in WHERE may not use indexes correctly",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Dml,
            check_fn: check_r008,
        },
        LintRuleEntry {
            id: "R009",
            name: "scalar-subquery-in-select",
            description: "Scalar subquery in SELECT target list executes once per row",
            level: WarningLevel::Performance,
            stmt_kind: StatementKind::Select,
            check_fn: check_r009,
        },
        LintRuleEntry {
            id: "R010",
            name: "function-side-effect",
            description: "Functions should not contain explicit transaction control statements",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::All,
            check_fn: check_r010,
        },
        LintRuleEntry {
            id: "R011",
            name: "order-by-distinct-mismatch",
            description: "ORDER BY columns must be included in SELECT result columns when DISTINCT is used",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::Select,
            check_fn: check_r011,
        },
        LintRuleEntry {
            id: "R012",
            name: "order-by-group-by-mismatch",
            description: "ORDER BY columns must be included in SELECT result columns when GROUP BY is used",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::Select,
            check_fn: check_r012,
        },
        LintRuleEntry {
            id: "R013",
            name: "implicit-join",
            description: "Oracle-style comma-separated FROM (implicit join) hurts readability; use ANSI JOIN",
            level: WarningLevel::Prohibition,
            stmt_kind: StatementKind::Select,
            check_fn: check_r013,
        },
    ];
    for rule in rules {
        linter.register(rule);
    }
}

// R001: SELECT * in nested queries only.
// Outer-most SELECT * is acceptable. Nested subqueries, CTE bodies, and
// embedded SELECTs must use explicit column lists.
fn check_r001(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let mut all_selects: Vec<(&SelectStatement, SourceLocation)> = Vec::new();
    collect_selects_from_stmt(&curr_stmt.statement, stmt_location(curr_stmt), &mut all_selects);

    // The outermost SELECT is the first entry for Select/Insert-Select/
    // CreateTableAs/CreateView/CreateMaterializedView.  For other statement
    // types (Update, Delete, PL blocks, etc.) all collected SELECTs are
    // already inner and should always warn.
    let has_outermost = matches!(
        &curr_stmt.statement,
        Statement::Select(_)
            | Statement::CreateTableAs(_)
            | Statement::CreateView(_)
            | Statement::CreateMaterializedView(_)
    ) || matches!(&curr_stmt.statement,
        Statement::Insert(ins) if matches!(&ins.node.source, InsertSource::Select(_))
    );

    for (i, (s, loc)) in all_selects.iter().enumerate() {
        let is_outermost = i == 0 && has_outermost;

        let should_warn = if is_outermost {
            // Outermost SELECT * is allowed ONLY when it wraps inner
            // queries (it acts as a pass-through).  Standalone outer
            // SELECT * (no inner queries) still violates the rule.
            let mut inner = Vec::new();
            collect_inner_selects_only(s, &mut inner);
            inner.is_empty()
        } else {
            true
        };

        if should_warn {
            for target in &s.targets {
                if let SelectTarget::Star(None) = target {
                    warnings.push(make_warning(
                        WarningLevel::Prohibition, "R001", "select-star",
                        "SELECT * \u{8fdd}\u{53cd} GaussDB \u{7f16}\u{7801}\u{89c4}\u{8303}\u{ff1a}\u{8868}\u{7ed3}\u{6784}\u{53d8}\u{5316}\u{65f6}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{4e0d}\u{517c}\u{5bb9}".into(),
                        Some("\u{660e}\u{786e}\u{5217}\u{51fa}\u{6240}\u{9700}\u{5b57}\u{6bb5}\u{540d}"),
                        *loc,
                        Some("\u{5f00}\u{53d1}\u{8bbe}\u{8ba1}\u{5efa}\u{8bae} > SELECT \u{89c4}\u{8303}"),
                        confidence,
                    ));
                }
            }
        }
    }
}

// R002: Large column sort / group by / distinct
fn check_r002(
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
        let group_count = s.group_by.len();
        let order_count = s.order_by.len();
        if group_count > config.group_by_column_limit {
            warnings.push(make_warning(
                WarningLevel::Prohibition, "R002", "large-column-sort",
                format!("GROUP BY \u{5305}\u{542b} {group_count} \u{4e2a}\u{8868}\u{8fbe}\u{5f0f}\u{ff0c}\u{8d85}\u{8fc7}\u{9608}\u{503c} {} \u{ff0c}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{6027}\u{80fd}\u{95ee}\u{9898}", config.group_by_column_limit),
                Some("\u{7b80}\u{5316} GROUP BY\u{ff0c}\u{51cf}\u{5c11}\u{5206}\u{7ec4}\u{5217}\u{6570}\u{91cf}"), loc,
                Some("SELECT \u{89c4}\u{8303}"), confidence,
            ));
        }
        if order_count > config.group_by_column_limit {
            warnings.push(make_warning(
                WarningLevel::Prohibition, "R002", "large-column-sort",
                format!("ORDER BY \u{5305}\u{542b} {order_count} \u{4e2a}\u{8868}\u{8fbe}\u{5f0f}\u{ff0c}\u{8d85}\u{8fc7}\u{9608}\u{503c} {} \u{ff0c}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{6027}\u{80fd}\u{95ee}\u{9898}", config.group_by_column_limit),
                Some("\u{7b80}\u{5316} ORDER BY\u{ff0c}\u{51cf}\u{5c11}\u{6392}\u{5e8f}\u{5217}\u{6570}\u{91cf}"), loc,
                Some("SELECT \u{89c4}\u{8303}"), confidence,
            ));
        }
    }
}

// R003: LOCK TABLE
fn check_r003(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Lock(s) = &curr_stmt.statement {
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        warnings.push(make_warning(
            WarningLevel::Prohibition, "R003", "lock-table",
            "LOCK TABLE \u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{6b7b}\u{9501}\u{98ce}\u{9669}".into(),
            Some("\u{907f}\u{514d}\u{5728}\u{4e8b}\u{52a1}\u{4e2d}\u{4f7f}\u{7528} LOCK TABLE\u{ff0c}\u{4f18}\u{5148}\u{4f7f}\u{7528} SELECT ... FOR UPDATE"), loc,
            Some("SELECT \u{89c4}\u{8303}"), confidence,
        ));
    }
}

// R004: DROP ... CASCADE
fn check_r004(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Drop(s) = &curr_stmt.statement {
        if s.cascade {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            let obj_type = object_type_str(&s.object_type);
            warnings.push(make_warning(
                WarningLevel::Prohibition,
                "R004",
                "drop-cascade",
                format!("DROP {obj_type} CASCADE \u{53ef}\u{80fd}\u{8bef}\u{5220}\u{4f9d}\u{8d56}\u{5bf9}\u{8c61}"),
                Some("\u{786e}\u{8ba4}\u{4f9d}\u{8d56}\u{5173}\u{7cfb}\u{540e}\u{518d}\u{4f7f}\u{7528} CASCADE"),
                loc,
                Some("SQL \u{7f16}\u{5199}"),
                confidence,
            ));
        }
    }
}

fn object_type_str(ot: &crate::ast::ObjectType) -> &'static str {
    use crate::ast::ObjectType;
    match ot {
        ObjectType::Table => "TABLE",
        ObjectType::Index => "INDEX",
        ObjectType::Sequence => "SEQUENCE",
        ObjectType::View => "VIEW",
        ObjectType::Schema => "SCHEMA",
        ObjectType::Database => "DATABASE",
        ObjectType::Tablespace => "TABLESPACE",
        ObjectType::Function => "FUNCTION",
        ObjectType::Procedure => "PROCEDURE",
        ObjectType::Trigger => "TRIGGER",
        ObjectType::Extension => "EXTENSION",
        _ => "OBJECT",
    }
}

// R005: Implicit type conversion (schema-aware)
fn check_r005(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    // Without schema, R005 cannot distinguish genuine cross-family implicit
    // type conversion from safe same-family comparisons (e.g. varchar_col =
    // 'str'). Skip entirely to avoid false positives on legitimate col =
    // literal patterns. Mirrors S007's "no schema → skip, no evidence"
    // approach. See issue #240.
    let Some(schema) = schema else {
        return;
    };

    let loc = stmt_location(curr_stmt);
    // Handle non-SELECT/UPDATE/DELETE statements with fallback check
    if !matches!(curr_stmt.statement, Statement::Select(_) | Statement::Update(_) | Statement::Delete(_)) {
        let Some(wc) = extract_where_clause(&curr_stmt.statement) else { return };
        check_r005_fallback(wc, loc, confidence, warnings);
        return;
    }
    let (where_clause, tables) = match &curr_stmt.statement {
        Statement::Select(s) => (s.where_clause.as_ref(), &s.from),
        Statement::Update(s) => (s.where_clause.as_ref(), &s.tables),
        Statement::Delete(s) => (s.where_clause.as_ref(), &s.tables),
        _ => unreachable!(),
    };
    let Some(where_clause) = where_clause else { return };

    let col_types = build_column_type_map(schema, tables);

    let mut found = false;
    walk_expr(where_clause, &mut |e| {
        if found {
            return false;
        }
        if let Expr::BinaryOp { left, right, .. } = e {
            let l_lit = matches!(left.as_ref(), Expr::Literal(_));
            let r_lit = matches!(right.as_ref(), Expr::Literal(_));
            let l_col = matches!(left.as_ref(), Expr::ColumnRef(_));
            let r_col = matches!(right.as_ref(), Expr::ColumnRef(_));
            if !((l_lit && r_col) || (l_col && r_lit)) {
                return true;
            }

            let (lit_expr, col_expr) =
                if l_lit { (left.as_ref(), right.as_ref()) } else { (right.as_ref(), left.as_ref()) };
            if let (Expr::Literal(lit), Some(col_type)) = (lit_expr, resolve_column_type(col_expr, &col_types)) {
                let lit_family = literal_type_family(lit);
                let col_family = classify_type_family(&col_type);
                if lit_family == col_family {
                    return true;
                }
            }
            found = true;
            return false;
        }
        true
    });
    if found {
        warnings.push(make_warning(
                WarningLevel::Prohibition, "R005", "implicit-type-conversion",
                "WHERE \u{4e2d}\u{53ef}\u{80fd}\u{5b58}\u{5728}\u{9690}\u{5f0f}\u{7c7b}\u{578b}\u{8f6c}\u{6362}\u{ff0c}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{7d22}\u{5f15}\u{5931}\u{6548}".into(),
                Some("\u{663e}\u{5f0f}\u{6dfb}\u{52a0}\u{7c7b}\u{578b}\u{8f6c}\u{6362}\u{ff0c}\u{907f}\u{514d}\u{9690}\u{5f0f}\u{8f6c}\u{6362}\u{5bfc}\u{81f4}\u{7d22}\u{5f15}\u{5931}\u{6548}"), loc,
                Some("WHERE \u{89c4}\u{8303}"), confidence,
            ));
    }
}

fn check_r005_fallback(
    where_clause: &Expr,
    loc: crate::token::SourceLocation,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let mut found = false;
    walk_expr(where_clause, &mut |e| {
        if found {
            return false;
        }
        if let Expr::BinaryOp { left, right, .. } = e {
            let l_lit = matches!(**left, Expr::Literal(_));
            let r_lit = matches!(**right, Expr::Literal(_));
            let l_col = matches!(**left, Expr::ColumnRef(_));
            let r_col = matches!(**right, Expr::ColumnRef(_));
            if (l_lit && r_col) || (l_col && r_lit) {
                found = true;
                return false;
            }
        }
        true
    });
    if found {
        warnings.push(make_warning(
                    WarningLevel::Prohibition, "R005", "implicit-type-conversion",
                    "WHERE \u{4e2d}\u{53ef}\u{80fd}\u{5b58}\u{5728}\u{9690}\u{5f0f}\u{7c7b}\u{578b}\u{8f6c}\u{6362}\u{ff08}\u{9700}\u{7ed3}\u{5408}\u{5b57}\u{6bb5}\u{7c7b}\u{578b}\u{786e}\u{8ba4}\u{ff09}".into(),
                    Some("\u{663e}\u{5f0f}\u{6dfb}\u{52a0}\u{7c7b}\u{578b}\u{8f6c}\u{6362}\u{ff0c}\u{907f}\u{514d}\u{9690}\u{5f0f}\u{8f6c}\u{6362}\u{5bfc}\u{81f4}\u{7d22}\u{5f15}\u{5931}\u{6548}"), loc,
                    Some("WHERE \u{89c4}\u{8303}"), confidence,
                ));
    }
}

// R006: Function wrapping column in WHERE (index-killing pattern)
const SAFE_FUNCTIONS_ON_COLUMNS: &[&str] = &["coalesce", "nvl", "nvl2", "ifnull", "isnull", "greatest", "least"];

fn check_r006(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let (where_clause, tables) = where_and_tables(&curr_stmt.statement);
    let Some(where_clause) = where_clause else { return };

    walk_expr(where_clause, &mut |e| {
        if let Expr::FunctionCall { name, args, .. } = e {
            let fn_lower = name.last().map(|s| s.to_lowercase()).unwrap_or_default();
            if SAFE_FUNCTIONS_ON_COLUMNS.contains(&fn_lower.as_str()) {
                return true;
            }
            for arg in args {
                if let Expr::ColumnRef(col_ref) = arg {
                    match _indexes {
                        Some(idx_info) => {
                            let table = resolve_table_from_column(col_ref, tables);
                            let col_lower = col_ref.last().map(|s| s.to_lowercase()).unwrap_or_default();

                            let has_index = table
                                .as_ref()
                                .and_then(|t| idx_info.column_indexes.get(t))
                                .map(|cols| cols.contains(&col_lower))
                                .unwrap_or(false);

                            let has_func_index = table
                                .as_ref()
                                .map(|t| {
                                    crate::analyzer::schema::matches_function_index(
                                        &idx_info.indexes,
                                        t,
                                        &fn_lower,
                                        &col_lower,
                                    )
                                })
                                .unwrap_or(false);

                            if has_index && !has_func_index {
                                warnings.push(make_warning(
                                    WarningLevel::Prohibition,
                                    "R006",
                                    "function-on-where-column",
                                    "WHERE 中对有索引的列使用函数，将导致索引失效".into(),
                                    Some("将函数运算移到等号另一侧或使用函数索引"),
                                    loc,
                                    Some("WHERE 规范"),
                                    confidence,
                                ));
                                return false;
                            }
                        }
                        None => {
                            warnings.push(make_warning(
                                WarningLevel::Prohibition,
                                "R006",
                                "function-on-where-column",
                                "WHERE 中对列使用函数，可能导致索引失效".into(),
                                Some("将函数运算移到等号另一侧或使用函数索引"),
                                loc,
                                Some("WHERE 规范"),
                                confidence,
                            ));
                            return false;
                        }
                    }
                }
            }
        }
        if let Expr::TypeCast { expr, .. } = e {
            if let Expr::ColumnRef(col_ref) = expr.as_ref() {
                emit_index_aware_r006(col_ref, tables, _indexes, loc, confidence, warnings);
            }
        }
        if let Expr::BinaryOp { op, left, right, .. } = e {
            if is_sargability_breaking_op(op) {
                if let Expr::ColumnRef(col_ref) = left.as_ref() {
                    emit_index_aware_r006(col_ref, tables, _indexes, loc, confidence, warnings);
                }
                if let Expr::ColumnRef(col_ref) = right.as_ref() {
                    emit_index_aware_r006(col_ref, tables, _indexes, loc, confidence, warnings);
                }
            }
        }
        true
    });
}

fn is_sargability_breaking_op(op: &str) -> bool {
    matches!(op, "+" | "-" | "*" | "/" | "||")
}

fn emit_index_aware_r006(
    col_ref: &[crate::ast::Ident],
    tables: &[crate::ast::TableRef],
    _indexes: Option<&crate::linter::IndexInfo>,
    loc: crate::token::SourceLocation,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Some(idx_info) = _indexes {
        let table = resolve_table_from_column(col_ref, tables);
        let col_lower = col_ref.last().map(|s| s.to_lowercase()).unwrap_or_default();
        let has_index = table
            .as_ref()
            .and_then(|t| idx_info.column_indexes.get(t))
            .map(|cols| cols.contains(&col_lower))
            .unwrap_or(false);
        if !has_index {
            return;
        }
    }
    warnings.push(make_warning(
        WarningLevel::Prohibition,
        "R006",
        "function-on-where-column",
        "WHERE 中对有索引的列使用函数或表达式，将导致索引失效".into(),
        Some("将运算移到等号另一侧"),
        loc,
        Some("WHERE 规范"),
        confidence,
    ));
}

/// Extract the WHERE clause and FROM/tables list from a statement.
fn where_and_tables(stmt: &Statement) -> (Option<&Expr>, &[crate::ast::TableRef]) {
    match stmt {
        Statement::Select(s) => (s.where_clause.as_ref(), &s.from),
        Statement::Update(s) => (s.where_clause.as_ref(), &s.tables),
        Statement::Delete(s) => (s.where_clause.as_ref(), &s.tables),
        // #296: use WHERE of embedded SELECTs inside PL bodies / CREATE bodies
        // (tables list empty: not aggregated from the embedded SELECT's FROM).
        Statement::AnonyBlock(b) => (first_embedded_where(&b.block), &[]),
        Statement::Do(d) => (d.block.as_ref().and_then(first_embedded_where), &[]),
        Statement::CreateFunction(f) => (f.block.as_ref().and_then(first_embedded_where), &[]),
        Statement::CreateProcedure(p) => (p.block.as_ref().and_then(first_embedded_where), &[]),
        Statement::CreatePackageBody(_) => {
            (crate::linter::pl_blocks_from_stmt(stmt).into_iter().find_map(first_embedded_where), &[])
        }
        _ => (extract_where_clause(stmt), &[]),
    }
}

/// Resolve which table a column belongs to from the FROM clause.
/// Returns the table name (lowercase), or None if unresolvable.
fn resolve_table_from_column(col_ref: &[crate::ast::Ident], tables: &[crate::ast::TableRef]) -> Option<String> {
    if col_ref.len() >= 2 {
        return Some(col_ref[col_ref.len() - 2].to_lowercase());
    }

    if col_ref.len() == 1 {
        for tref in tables {
            if let Some(name) = table_name(tref) {
                return Some(name.to_lowercase());
            }
        }
    }

    None
}

/// Extract the effective table name from a TableRef (handles alias).
fn table_name(tref: &crate::ast::TableRef) -> Option<&str> {
    use crate::ast::TableRef;
    match tref {
        TableRef::Table { name, alias, .. } => {
            if let Some(a) = alias {
                Some(a.as_str())
            } else {
                name.last().map(|s| s.as_str())
            }
        }
        TableRef::Join { left, .. } => table_name(left),
        TableRef::Subquery { alias, .. } | TableRef::FunctionCall { alias, .. } | TableRef::Values { alias, .. } => {
            alias.as_deref()
        }
        TableRef::Pivot { source, .. } | TableRef::Unpivot { source, .. } => table_name(source),
    }
}

// R007: LIKE with leading wildcard
fn check_r007(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let (where_clause, tables) = where_and_tables(&curr_stmt.statement);
    let Some(where_clause) = where_clause else { return };
    walk_expr(where_clause, &mut |e| {
        if let Expr::Like { expr, pattern, negated: false, .. } = e {
            if let Expr::Literal(crate::ast::Literal::String(s)) = pattern.as_ref() {
                if s.starts_with('%') || s.starts_with('_') {
                    let should_warn = match _indexes {
                        Some(idx_info) => {
                            if let Expr::ColumnRef(col_ref) = expr.as_ref() {
                                let table = resolve_table_from_column(col_ref, tables);
                                let col_lower = col_ref.last().map(|s| s.to_lowercase()).unwrap_or_default();
                                table
                                    .and_then(|t| idx_info.column_indexes.get(&t))
                                    .map(|cols| cols.contains(&col_lower))
                                    .unwrap_or(false)
                            } else {
                                true
                            }
                        }
                        None => true,
                    };
                    if should_warn {
                        warnings.push(make_warning(
                            WarningLevel::Prohibition, "R007", "like-leading-wildcard",
                            format!("LIKE '\u{524d}\u{5bfc}\u{901a}\u{914d}\u{7b26} {s}' \u{5c06}\u{5bfc}\u{81f4}\u{65e0}\u{6cd5}\u{4f7f}\u{7528}\u{7d22}\u{5f15}\u{ff0c}\u{89e6}\u{53d1}\u{5168}\u{8868}\u{626b}\u{63cf}"),
                            Some("\u{907f}\u{514d}\u{4ee5}\u{901a}\u{914d}\u{7b26}\u{5f00}\u{5934}\u{7684} LIKE \u{6a21}\u{5f0f}"), loc,
                            Some("WHERE \u{89c4}\u{8303}"), confidence,
                        ));
                    }
                }
            }
        }
        true
    });
}

// R008: Same-table column comparison in WHERE
fn check_r008(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Some(where_clause) = extract_where_clause(&curr_stmt.statement) {
        walk_expr(where_clause, &mut |e| {
            if let Expr::BinaryOp { left, right, .. } = e {
                if let (Expr::ColumnRef(l), Expr::ColumnRef(r)) = (left.as_ref(), right.as_ref()) {
                    if l.len() >= 2 && r.len() >= 2 && l[0] == r[0] {
                        warnings.push(make_warning(
                            WarningLevel::Caution, "R008", "same-table-column-compare",
                            format!("\u{540c}\u{8868}\u{5217}\u{6bd4}\u{8f83}: {}.{} \u{4e0e} {}.{}\u{ff0c}\u{53ef}\u{80fd}\u{672a}\u{6b63}\u{786e}\u{4f7f}\u{7528}\u{7d22}\u{5f15}", l[0], l.last().unwrap_or(&"".into()), r[0], r.last().unwrap_or(&"".into())),
                            Some("\u{68c0}\u{67e5}\u{662f}\u{5426}\u{5e94}\u{4f7f}\u{7528}\u{4e0d}\u{540c}\u{8868}\u{7684}\u{5217}\u{8fdb}\u{884c}\u{6bd4}\u{8f83}"), loc,
                            Some("WHERE \u{89c4}\u{8303}"), confidence,
                        ));
                    }
                }
            }
            true
        });
    }
}

// R009: Scalar subquery in SELECT target list
fn check_r009(
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
        for target in &s.targets {
            if let SelectTarget::Expr(e, _) = target {
                let mut found = false;
                walk_expr(e, &mut |inner| {
                    if found {
                        return false;
                    }
                    if matches!(inner, Expr::Subquery(_)) {
                        found = true;
                        return false;
                    }
                    true
                });
                if found {
                    warnings.push(make_warning(
                        WarningLevel::Performance, "R009", "scalar-subquery-in-select",
                        "SELECT \u{5217}\u{4e2d}\u{5305}\u{542b}\u{6807}\u{91cf}\u{5b50}\u{67e5}\u{8be2}\u{ff0c}\u{6bcf}\u{884c}\u{90fd}\u{4f1a}\u{6267}\u{884c}\u{4e00}\u{6b21}\u{5b50}\u{67e5}\u{8be2}".into(),
                        Some("\u{6539}\u{7528} JOIN \u{66ff}\u{4ee3}\u{6807}\u{91cf}\u{5b50}\u{67e5}\u{8be2}"), loc,
                        Some("SQL \u{7f16}\u{5199}"), confidence,
                    ));
                }
            }
        }
    }
}

// R010: Function side effect — explicit transaction control (COMMIT/ROLLBACK/SAVEPOINT
// /SET TRANSACTION) or calls to procedures/functions with transactions.
// DML statements (INSERT/UPDATE/DELETE/MERGE/TRUNCATE) without explicit transaction
// control are allowed in functions.
fn check_r010(
    curr_stmt: &StatementInfo,
    stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let tx_procedures = build_tx_procedure_set(stmts);
    let loc = stmt_location(curr_stmt);

    match &curr_stmt.statement {
        Statement::CreateFunction(func) => {
            if let Some(block) = &func.block {
                r010_check_function(&func.name.join("."), block, loc, &tx_procedures, confidence, warnings);
            }
        }
        Statement::CreatePackageBody(pkg_body) => {
            let pkg_name = pkg_body.name.join(".");
            for item in &pkg_body.items {
                if let PackageItem::Function(func) = item {
                    if let Some(block) = &func.block {
                        let full_name = format!("{}.{}", pkg_name, func.name.join("."));
                        r010_check_function(&full_name, block, loc, &tx_procedures, confidence, warnings);
                    }
                }
            }
        }
        _ => {}
    }
}

fn r010_check_function(
    func_name: &str,
    block: &PlBlock,
    loc: crate::token::SourceLocation,
    tx_procedures: &std::collections::HashSet<String>,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let mut has_dml = false;
    let mut has_tx = false;
    let mut called_tx_procs: Vec<String> = Vec::new();

    scan_block_for_side_effects(block, tx_procedures, &mut has_dml, &mut has_tx, &mut called_tx_procs);

    if !has_tx && called_tx_procs.is_empty() {
        return;
    }

    let mut details: Vec<String> = Vec::new();
    if has_tx {
        details.push(
            "\u{4e8b}\u{52a1}\u{63a7}\u{5236}\u{8bed}\u{53e5}\u{ff08}COMMIT/ROLLBACK/SAVEPOINT/SET TRANSACTION\u{ff09}"
                .to_string(),
        );
    }
    for name in &called_tx_procs {
        details.push(format!(
            "\u{8c03}\u{7528}\u{542b}\u{4e8b}\u{52a1}\u{7684}\u{8fc7}\u{7a0b}/\u{51fd}\u{6570} \"{}\"",
            name
        ));
    }
    let message = format!(
        "\u{51fd}\u{6570} \"{}\" \u{5305}\u{542b}\u{4e0d}\u{5efa}\u{8bae}\u{5728}\u{51fd}\u{6570}\u{4e2d}\u{4f7f}\u{7528}\u{7684}\u{64cd}\u{4f5c}\u{ff1a}{}",
        func_name,
        details.join("\u{ff0c}")
    );

    warnings.push(make_warning(
        WarningLevel::Prohibition,
        "R010",
        "function-side-effect",
        message,
        Some(
            "\u{51fd}\u{6570}\u{4e0d}\u{5e94}\u{663e}\u{5f0f}\u{63d0}\u{4ea4}\u{6216}\u{56de}\u{6eda}\u{4e8b}\u{52a1}\u{ff0c}\u{8003}\u{8651}\u{5c06} COMMIT/ROLLBACK \u{79fb}\u{81f3}\u{8fc7}\u{7a0b}\u{4e2d}\u{ff0c}\u{51fd}\u{6570}\u{5185}\u{53ef}\u{4f7f}\u{7528} DML \u{4f46}\u{4e0d}\u{5e94}\u{63a7}\u{5236}\u{4e8b}\u{52a1}",
        ),
        loc,
        Some("\u{5f00}\u{53d1}\u{8bbe}\u{8ba1}\u{5efa}\u{8bae} > \u{51fd}\u{6570}\u{89c4}\u{8303}"),
        confidence,
    ));
}

/// Build a set of procedure/function names that contain COMMIT or ROLLBACK.
fn build_tx_procedure_set(stmts: &[StatementInfo]) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for info in stmts {
        match &info.statement {
            Statement::CreateProcedure(p) => {
                if let Some(block) = &p.block {
                    if pl_block_has_tx_control(block) {
                        set.insert(p.name.join(".").to_lowercase());
                    }
                }
            }
            Statement::CreateFunction(f) => {
                if let Some(block) = &f.block {
                    if pl_block_has_tx_control(block) {
                        set.insert(f.name.join(".").to_lowercase());
                    }
                }
            }
            Statement::CreatePackageBody(pkg_body) => {
                let pkg_name = pkg_body.name.join(".").to_lowercase();
                for item in &pkg_body.items {
                    match item {
                        PackageItem::Procedure(p) => {
                            if let Some(block) = &p.block {
                                if pl_block_has_tx_control(block) {
                                    set.insert(format!("{}.{}", pkg_name, p.name.join(".").to_lowercase()));
                                }
                            }
                        }
                        PackageItem::Function(f) => {
                            if let Some(block) = &f.block {
                                if pl_block_has_tx_control(block) {
                                    set.insert(format!("{}.{}", pkg_name, f.name.join(".").to_lowercase()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => continue,
        }
    }
    set
}

/// Check if a PL block contains COMMIT or ROLLBACK (recursively).
fn pl_block_has_tx_control(block: &PlBlock) -> bool {
    pl_stmts_have_tx(&block.body)
}

fn pl_stmts_have_tx(stmts: &[PlStatement]) -> bool {
    for stmt in stmts {
        if pl_stmt_has_tx(stmt) {
            return true;
        }
    }
    false
}

/// Returns true for leaf PL statements that constitute transaction control.
fn is_pl_tx_stmt(stmt: &PlStatement) -> bool {
    matches!(
        stmt,
        PlStatement::Commit { .. }
            | PlStatement::Rollback { .. }
            | PlStatement::Savepoint { .. }
            | PlStatement::ReleaseSavepoint { .. }
            | PlStatement::SetTransaction { .. }
    )
}

fn pl_stmt_has_tx(stmt: &PlStatement) -> bool {
    if is_pl_tx_stmt(stmt) {
        return true;
    }
    match stmt {
        PlStatement::Block(block) => pl_block_has_tx_control(block),
        PlStatement::If(if_stmt) => {
            pl_stmts_have_tx(&if_stmt.then_stmts)
                || if_stmt.elsifs.iter().any(|e| pl_stmts_have_tx(&e.stmts))
                || pl_stmts_have_tx(&if_stmt.else_stmts)
        }
        PlStatement::Case(case_stmt) => {
            case_stmt.whens.iter().any(|w| pl_stmts_have_tx(&w.stmts)) || pl_stmts_have_tx(&case_stmt.else_stmts)
        }
        PlStatement::Loop(loop_stmt) => pl_stmts_have_tx(&loop_stmt.body),
        PlStatement::While(while_stmt) => pl_stmts_have_tx(&while_stmt.body),
        PlStatement::For(for_stmt) => pl_stmts_have_tx(&for_stmt.body),
        PlStatement::ForEach(foreach_stmt) => pl_stmts_have_tx(&foreach_stmt.body),
        _ => false,
    }
}

/// Recursively scan a PL block for side effects.
fn scan_block_for_side_effects(
    block: &PlBlock,
    tx_procedures: &std::collections::HashSet<String>,
    has_dml: &mut bool,
    has_tx: &mut bool,
    called_tx_procs: &mut Vec<String>,
) {
    scan_stmts_for_side_effects(&block.body, tx_procedures, has_dml, has_tx, called_tx_procs);
    if let Some(ref exc) = block.exception_block {
        for handler in &exc.handlers {
            scan_stmts_for_side_effects(&handler.statements, tx_procedures, has_dml, has_tx, called_tx_procs);
        }
    }
}

fn scan_stmts_for_side_effects(
    stmts: &[PlStatement],
    tx_procedures: &std::collections::HashSet<String>,
    has_dml: &mut bool,
    has_tx: &mut bool,
    called_tx_procs: &mut Vec<String>,
) {
    for stmt in stmts {
        match stmt {
            PlStatement::Block(inner) => {
                scan_block_for_side_effects(inner, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::SqlStatement { statement, .. } => {
                if is_non_select_dml(statement) {
                    *has_dml = true;
                }
            }
            PlStatement::Sql(sql_text) => {
                if sql_text_starts_with_dml(sql_text) {
                    *has_dml = true;
                }
                if sql_text_has_tx(sql_text) {
                    *has_tx = true;
                }
            }
            PlStatement::Execute(exec) => {
                if let Some(ref parsed) = exec.parsed_query {
                    if is_non_select_dml(parsed) {
                        *has_dml = true;
                    }
                }
            }
            PlStatement::ProcedureCall(call) => {
                let callee = call.name.join(".").to_lowercase();
                if tx_procedures.contains(&callee) && !called_tx_procs.contains(&call.name.join(".")) {
                    called_tx_procs.push(call.name.join("."));
                }
            }
            PlStatement::If(if_stmt) => {
                scan_stmts_for_side_effects(&if_stmt.then_stmts, tx_procedures, has_dml, has_tx, called_tx_procs);
                for elsif in &if_stmt.elsifs {
                    scan_stmts_for_side_effects(&elsif.stmts, tx_procedures, has_dml, has_tx, called_tx_procs);
                }
                scan_stmts_for_side_effects(&if_stmt.else_stmts, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::Case(case_stmt) => {
                for when in &case_stmt.whens {
                    scan_stmts_for_side_effects(&when.stmts, tx_procedures, has_dml, has_tx, called_tx_procs);
                }
                scan_stmts_for_side_effects(&case_stmt.else_stmts, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::Loop(loop_stmt) => {
                scan_stmts_for_side_effects(&loop_stmt.body, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::While(while_stmt) => {
                scan_stmts_for_side_effects(&while_stmt.body, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::For(for_stmt) => {
                scan_stmts_for_side_effects(&for_stmt.body, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            PlStatement::ForEach(foreach_stmt) => {
                scan_stmts_for_side_effects(&foreach_stmt.body, tx_procedures, has_dml, has_tx, called_tx_procs);
            }
            _ => {
                if is_pl_tx_stmt(stmt) {
                    *has_tx = true;
                }
            }
        }
    }
}

/// Check if a SQL statement is non-SELECT DML.
fn is_non_select_dml(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::Insert(_)
            | Statement::InsertAll(_)
            | Statement::InsertFirst(_)
            | Statement::Replace(_)
            | Statement::Update(_)
            | Statement::Delete(_)
            | Statement::Merge(_)
            | Statement::Truncate(_)
            | Statement::Copy(_)
    )
}

/// Quick check if a raw SQL text (from PlStatement::Sql) starts with a DML keyword.
fn sql_text_starts_with_dml(text: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    lower.starts_with("insert")
        || lower.starts_with("update")
        || lower.starts_with("delete")
        || lower.starts_with("merge")
        || lower.starts_with("truncate")
        || lower.starts_with("copy")
        || lower.starts_with("replace")
}

/// Quick check if a raw SQL text (from PlStatement::Sql) starts with a transaction keyword.
fn sql_text_has_tx(text: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    lower.starts_with("start transaction")
        || lower.starts_with("begin transaction")
        || lower.starts_with("begin work")
        || lower.starts_with("savepoint")
        || lower.starts_with("release savepoint")
        || lower.starts_with("rollback to savepoint")
        || lower.starts_with("set transaction")
}

// ── R011: ORDER BY with DISTINCT — columns must appear in SELECT targets ──

fn check_r011(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if !s.distinct || s.order_by.is_empty() {
            return;
        }
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        let extra_names = std::collections::HashSet::new();
        check_order_by_select_targets(
            s,
            loc,
            "DISTINCT",
            "R011",
            "order-by-distinct-mismatch",
            &extra_names,
            confidence,
            warnings,
        );
    }
}

// ── R012: ORDER BY with GROUP BY — columns must appear in SELECT targets ──

fn check_r012(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if s.group_by.is_empty() || s.order_by.is_empty() {
            return;
        }
        // Collect GROUP BY column names — they are acceptable ORDER BY targets
        // even when absent from SELECT (SQL standard allows this).
        let extra_names: std::collections::HashSet<String> = s
            .group_by
            .iter()
            .filter_map(|gb| {
                if let GroupByItem::Expr(Expr::ColumnRef(name)) = gb {
                    name.last().map(|n| n.to_lowercase())
                } else {
                    None
                }
            })
            .collect();
        let loc = loc_from_spanned(s, stmt_location(curr_stmt));
        check_order_by_select_targets(
            s,
            loc,
            "GROUP BY",
            "R012",
            "order-by-group-by-mismatch",
            &extra_names,
            confidence,
            warnings,
        );
    }
}

/// Shared check: verify all ORDER BY expressions are present in the SELECT target list.
fn check_order_by_select_targets(
    select: &SelectStatement,
    loc: crate::token::SourceLocation,
    clause: &str,                                    // "DISTINCT" or "GROUP BY"
    rule_id: &str,                                   // "R011" or "R012"
    rule_name: &str,                                 // rule name for SqlWarning
    extra_names: &std::collections::HashSet<String>, // additional acceptable column names (e.g. GROUP BY columns for R012)
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    // If any target is Star (SELECT *, t.*) or a QualifiedStar expression,
    // we cannot determine the full column set without schema — skip.
    let has_star = select
        .targets
        .iter()
        .any(|t| matches!(t, SelectTarget::Star(_) | SelectTarget::Expr(Expr::QualifiedStar(_), _)));
    if has_star {
        return;
    }

    // Collect output column identities from SELECT targets,
    // plus any additional acceptable names (e.g. GROUP BY columns).
    let output_names: HashSet<String> = {
        let mut names: HashSet<String> = select
            .targets
            .iter()
            .flat_map(|t| {
                let mut ns = Vec::new();
                match t {
                    SelectTarget::Expr(Expr::ColumnRef(ref name), alias) => {
                        if let Some(last) = name.last() {
                            ns.push(last.to_lowercase());
                        }
                        if let Some(a) = alias {
                            ns.push(a.to_lowercase());
                        }
                    }
                    SelectTarget::Expr(_, alias) => {
                        if let Some(a) = alias {
                            ns.push(a.to_lowercase());
                        }
                    }
                    SelectTarget::Star(_) => {} // handled above
                }
                ns
            })
            .collect();
        names.extend(extra_names.iter().cloned());
        names
    };

    for order_item in &select.order_by {
        let is_covered = order_by_item_covered(&order_item.expr, &output_names, select.targets.len());
        if !is_covered {
            let expr_str = format_expr_brief(&order_item.expr);
            let suggestion = format!("将 \"{expr_str}\" 添加到 SELECT 结果集中，或使用列别名引用");
            warnings.push(make_warning(
                WarningLevel::Prohibition,
                rule_id,
                rule_name,
                format!("ORDER BY 表达式 \"{expr_str}\" 未包含在 SELECT 结果集中，与 {clause} 一起使用时可能导致错误"),
                Some(&suggestion),
                loc,
                Some("SELECT 规范"),
                confidence,
            ));
        }
    }
}

// ── R013: Oracle implicit join — FROM a, b (comma-separated table list) ──
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
                "\u{4f7f}\u{7528} Oracle \u{9690}\u{5f0f}\u{8fde}\u{63a5}\u{8bed}\u{6cd5}\u{ff08}\u{9017}\u{53f7}\u{5206}\u{9694} FROM\u{ff09}\u{ff0c}\u{5efa}\u{8bae}\u{6539}\u{4e3a} ANSI JOIN".into(),
                Some("\u{6539}\u{7528} INNER JOIN / LEFT JOIN \u{63d0}\u{9ad8}\u{53ef}\u{8bfb}\u{6027}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

/// Check whether a single ORDER BY expression is covered by the SELECT output.
fn order_by_item_covered(expr: &Expr, output_names: &HashSet<String>, target_count: usize) -> bool {
    match expr {
        // Positional reference: ORDER BY 1, ORDER BY 2, etc. (1-based)
        Expr::Literal(crate::ast::Literal::Integer(n)) => {
            if *n >= 1 {
                return (*n as usize) <= target_count;
            }
            false
        }
        // Column reference: check by name
        Expr::ColumnRef(name) | Expr::ColumnRefOuterJoin(name) => {
            if let Some(last) = name.last() {
                let lower = last.to_lowercase();
                return output_names.contains(&lower);
            }
            false
        }
        // For other expressions, check if they match an alias
        _ => {
            // Expressions don't have a simple name, so they're only covered
            // if a SELECT target alias matches (unlikely for arbitrary expressions)
            false
        }
    }
}

/// Produce a brief string representation of an expression for use in warning messages.
fn format_expr_brief(expr: &Expr) -> String {
    match expr {
        Expr::ColumnRef(name) | Expr::ColumnRefOuterJoin(name) => name.join("."),
        Expr::Literal(crate::ast::Literal::Integer(n)) => n.to_string(),
        Expr::Literal(crate::ast::Literal::String(s)) => format!("'{s}'"),
        Expr::Literal(crate::ast::Literal::Boolean(b)) => b.to_string(),
        Expr::Literal(crate::ast::Literal::Null) => "NULL".to_string(),
        Expr::Literal(lit) => format!("{lit:?}"), // fallback for Float/Bit/Hex/etc.
        Expr::FunctionCall { name, .. } => {
            format!("{}()", name.join("."))
        }
        _ => {
            // Fallback: use a generic placeholder
            "<expression>".to_string()
        }
    }
}

fn extract_where_clause(stmt: &Statement) -> Option<&Expr> {
    match stmt {
        Statement::Select(s) => s.where_clause.as_ref(),
        Statement::Update(s) => s.where_clause.as_ref(),
        Statement::Delete(s) => s.where_clause.as_ref(),
        // #296: check WHERE of embedded SELECTs inside PL bodies / CREATE bodies.
        Statement::AnonyBlock(b) => first_embedded_where(&b.block),
        Statement::Do(d) => d.block.as_ref().and_then(first_embedded_where),
        Statement::CreateFunction(f) => f.block.as_ref().and_then(first_embedded_where),
        Statement::CreateProcedure(p) => p.block.as_ref().and_then(first_embedded_where),
        Statement::CreatePackageBody(_) => {
            crate::linter::pl_blocks_from_stmt(stmt).into_iter().find_map(first_embedded_where)
        }
        _ => None,
    }
}

fn first_embedded_where(block: &PlBlock) -> Option<&Expr> {
    let mut selects: Vec<(&SelectStatement, SourceLocation)> = Vec::new();
    crate::linter::collect_selects_from_pl_block(block, &mut selects);
    selects.iter().find_map(|(s, _)| s.where_clause.as_ref())
}
