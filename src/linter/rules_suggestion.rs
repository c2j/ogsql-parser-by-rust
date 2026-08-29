use crate::ast::plpgsql::{PlDataType, PlDeclaration, PlStatement};
use crate::ast::{Expr, Statement, StatementInfo};
use crate::linter::type_helpers::{build_column_type_map, is_string_type, resolve_column_type};
use crate::linter::{
    extract_where, literals_equal, loc_from_spanned, make_warning, stmt_location, walk_expr, Confidence, LintConfig,
    LintRuleEntry, SqlLinter, SqlWarning, StatementKind, WarningLevel,
};

pub fn register(linter: &mut SqlLinter) {
    let rules: Vec<LintRuleEntry> = vec![
        LintRuleEntry {
            id: "S001",
            name: "delete-full-table-use-truncate",
            description: "Full-table DELETE can be replaced with TRUNCATE for faster space reclamation",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Delete,
            check_fn: check_s001,
        },
        LintRuleEntry {
            id: "S002",
            name: "limit-offset-use-cursor",
            description: "OFFSET pagination performs poorly with large offsets; consider cursor-based pagination",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Select,
            check_fn: check_s002,
        },
        LintRuleEntry {
            id: "S005",
            name: "prefer-percent-type",
            description: "PL/pgSQL variables should use %TYPE or %ROWTYPE anchoring for better maintainability",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_s005,
        },
        LintRuleEntry {
            id: "S006",
            name: "limit-without-order-by",
            description: "LIMIT without ORDER BY produces non-deterministic results",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Select,
            check_fn: check_s006,
        },
        LintRuleEntry {
            id: "S007",
            name: "explicit-type-for-literals",
            description: "String literals in WHERE should use explicit type casting to avoid implicit conversion",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Dml,
            check_fn: check_s007,
        },
        LintRuleEntry {
            id: "S008",
            name: "complex-sql-consider-split",
            description:
                "Overly long SQL text may be difficult to maintain; consider splitting into simpler queries or CTEs",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::All,
            check_fn: check_s008,
        },
        LintRuleEntry {
            id: "S009",
            name: "tautological-condition",
            description: "Tautological conditions like 1=1 in WHERE are redundant; remove them",
            level: WarningLevel::Suggestion,
            stmt_kind: StatementKind::Dml,
            check_fn: check_s009,
        },
    ];
    for rule in rules {
        linter.register(rule);
    }
}

// ── S001: DELETE full table → use TRUNCATE ──

fn check_s001(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Delete(s) = &curr_stmt.statement {
        if s.where_clause.is_none() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Suggestion,
                "S001",
                "delete-full-table-use-truncate",
                "DELETE \u{5168}\u{8868}\u{53ef}\u{7528} TRUNCATE \u{66ff}\u{4ee3}\u{ff0c}\u{91ca}\u{653e}\u{7a7a}\u{95f4}\u{66f4}\u{5feb}".into(),
                Some("\u{4f7f}\u{7528} TRUNCATE TABLE \u{66ff}\u{4ee3} DELETE \u{65e0} WHERE"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── S002: LIMIT + OFFSET → use cursor ──

fn check_s002(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if s.offset.is_some() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Suggestion,
                "S002",
                "limit-offset-use-cursor",
                "OFFSET \u{5206}\u{9875}\u{5728}\u{5927}\u{504f}\u{79fb}\u{65f6}\u{6027}\u{80fd}\u{8f83}\u{5dee}"
                    .into(),
                Some("\u{8003}\u{8651}\u{4f7f}\u{7528}\u{6e38}\u{6807}\u{7ffb}\u{9875}\u{66ff}\u{4ee3} LIMIT/OFFSET"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── S005: Prefer %TYPE anchored types ──

fn check_s005(
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
    walk_pl_for_type_name(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Suggestion,
            "S005",
            "prefer-percent-type",
            "PL/pgSQL \u{53d8}\u{91cf}\u{4f7f}\u{7528}\u{666e}\u{901a}\u{7c7b}\u{578b}\u{800c}\u{975e} %TYPE/%ROWTYPE \u{951a}\u{5b9a}".into(),
            Some("\u{4f7f}\u{7528} table.column%TYPE \u{6216} table%ROWTYPE \u{951a}\u{5b9a}\u{7c7b}\u{578b}\u{4ee5}\u{63d0}\u{9ad8}\u{53ef}\u{7ef4}\u{62a4}\u{6027}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_type_name(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_decls_for_type_name(&block.declarations, found);
        if !*found {
            check_pl_stmts_for_type_name(&block.body, found);
        }
        if *found {
            return;
        }
    }
}

fn check_decls_for_type_name(decls: &[PlDeclaration], found: &mut bool) {
    for d in decls {
        if let PlDeclaration::Variable(v) = d {
            if let PlDataType::TypeName(name) = &v.data_type {
                let lower = name.to_lowercase();
                let is_simple = [
                    "integer",
                    "int",
                    "bigint",
                    "smallint",
                    "serial",
                    "bigserial",
                    "text",
                    "varchar",
                    "char",
                    "character",
                    "boolean",
                    "bool",
                    "numeric",
                    "decimal",
                    "real",
                    "double precision",
                    "float",
                    "date",
                    "timestamp",
                    "timestamptz",
                    "time",
                    "timetz",
                    "interval",
                    "bytea",
                    "uuid",
                    "json",
                    "jsonb",
                ]
                .contains(&lower.as_str());
                if !is_simple {
                    *found = true;
                    return;
                }
            }
        }
    }
}

fn check_pl_stmts_for_type_name(pl_stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in pl_stmts {
        if let PlStatement::Block(b) = s {
            check_decls_for_type_name(&b.declarations, found);
            if !*found {
                check_pl_stmts_for_type_name(&b.body, found);
            }
        }
        if *found {
            return;
        }
    }
}

// ── S006: LIMIT without ORDER BY ──

fn check_s006(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        let has_limit = s.limit.is_some() || s.fetch.is_some() || matches!(&s.fetch, Some(f) if f.count.is_some());
        if has_limit && s.order_by.is_empty() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Suggestion,
                "S006",
                "limit-without-order-by",
                "LIMIT \u{65e0} ORDER BY\u{ff0c}\u{7ed3}\u{679c}\u{987a}\u{5e8f}\u{4e0d}\u{786e}\u{5b9a}".into(),
                Some("\u{6dfb}\u{52a0} ORDER BY \u{4fdd}\u{8bc1}\u{7ed3}\u{679c}\u{786e}\u{5b9a}\u{6027}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── S007: Explicit type for literals in WHERE ──

fn check_s007(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let (where_clause, tables) = match &curr_stmt.statement {
        Statement::Select(s) => (s.where_clause.as_ref(), &s.from),
        Statement::Update(s) => (s.where_clause.as_ref(), &s.tables),
        Statement::Delete(s) => (s.where_clause.as_ref(), &s.tables),
        _ => return,
    };
    let Some(where_clause) = where_clause else { return };

    // Only warn when schema is available and provides concrete evidence of
    // a cross-family comparison (e.g. int_col = 'str'). Without schema or
    // when the column can't be resolved, skip — no evidence to warn on.
    let col_types = schema.map(|s| build_column_type_map(s, tables));

    let mut found = false;
    walk_expr(where_clause, &mut |e| {
        if found {
            return false;
        }
        if let Expr::BinaryOp { left, right, op, .. } = e {
            let is_compare = matches!(op.as_str(), "=" | "<>" | "!=" | ">" | "<" | ">=" | "<=");
            if !is_compare {
                return true;
            }
            let l_untyped = matches!(left.as_ref(), Expr::Literal(crate::ast::Literal::String(_)));
            let r_untyped = matches!(right.as_ref(), Expr::Literal(crate::ast::Literal::String(_)));
            if !l_untyped && !r_untyped {
                return true;
            }

            let Some(ref ct) = col_types else {
                return true; // no schema → skip, no evidence
            };
            let col_expr = if l_untyped { right.as_ref() } else { left.as_ref() };
            let Some(col_type) = resolve_column_type(col_expr, ct) else {
                return true; // column unresolved → skip, no evidence
            };
            if is_string_type(&col_type) {
                return true; // same-family → skip, safe
            }
            // Cross-family: concrete evidence of potential implicit conversion.
            found = true;
            return false;
        }
        true
    });
    if found {
        warnings.push(make_warning(
            WarningLevel::Suggestion,
            "S007",
            "explicit-type-for-literals",
            "WHERE \u{4e2d}\u{5b57}\u{7b26}\u{4e32}\u{5e38}\u{91cf}\u{672a}\u{663e}\u{5f0f}\u{6307}\u{5b9a}\u{7c7b}\u{578b}\u{ff0c}\u{53ef}\u{80fd}\u{5bfc}\u{81f4}\u{9690}\u{5f0f}\u{8f6c}\u{6362}".into(),
            Some("\u{4f7f}\u{7528}\u{663e}\u{5f0f}\u{7c7b}\u{578b}\u{8f6c}\u{6362}\u{ff1a} 'val'::type \u{6216} CAST('val' AS type)"),
            loc,
            None,
            confidence,
        ));
    }
}

// ── S008: Complex SQL — consider splitting ──

fn check_s008(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if curr_stmt.sql_text.len() > config.sql_length_limit {
        let loc = stmt_location(curr_stmt);
        warnings.push(make_warning(
            WarningLevel::Suggestion,
            "S008",
            "complex-sql-consider-split",
            format!(
                "SQL \u{6587}\u{672c}\u{957f}\u{5ea6} {} \u{5b57}\u{7b26}\u{ff0c}\u{8d85}\u{8fc7}\u{9608}\u{503c} {}\u{ff0c}\u{5efa}\u{8bae}\u{62c6}\u{5206}\u{7b80}\u{5316}",
                curr_stmt.sql_text.len(),
                config.sql_length_limit
            ),
            Some("\u{5c06}\u{590d}\u{6742} SQL \u{62c6}\u{5206}\u{4e3a}\u{591a}\u{4e2a}\u{7b80}\u{5355}\u{67e5}\u{8be2}\u{6216}\u{4f7f}\u{7528} CTE"),
            loc,
            None,
            confidence,
        ));
    }
}

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
                if is_tautological_op(op) && literals_equal(left, right) {
                    warnings.push(make_warning(
                        WarningLevel::Suggestion,
                        "S009",
                        "tautological-condition",
                        format!("WHERE \u{5b50}\u{53e5}\u{4e2d}\u{5b58}\u{5728}\u{6052}\u{771f}\u{6761}\u{4ef6}\u{ff08}{op}\u{ff09}\u{ff0c}\u{5efa}\u{8bae}\u{79fb}\u{9664}"),
                        Some("\u{79fb}\u{9664}\u{8be5}\u{6761}\u{4ef6}\u{ff0c}\u{76f4}\u{63a5}\u{7f16}\u{5199}\u{5176}\u{4ed6}\u{6761}\u{4ef6}"),
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

/// Operators that are always true when both operands are the same literal.
/// `<>`/`!=`/`>`/`<` are deliberately excluded: with equal operands they are
/// always *false* (contradictions), not tautologies.
fn is_tautological_op(op: &str) -> bool {
    matches!(op, "=" | ">=" | "<=")
}
