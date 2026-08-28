use crate::ast::plpgsql::{PlBlock, PlDeclaration, PlStatement, RaiseLevel};
use crate::ast::{Expr, HintInfo, InsertSource, LockClause, Statement, StatementInfo};
use crate::linter::{
    loc_from_spanned, make_warning, stmt_location, walk_expr, Confidence, LintConfig, LintRuleEntry, SqlLinter,
    SqlWarning, StatementKind, WarningLevel,
};
use crate::parser::hint_validator::KNOWN_HINTS;

pub fn register(linter: &mut SqlLinter) {
    let rules: Vec<LintRuleEntry> = vec![
        LintRuleEntry {
            id: "C001",
            name: "hint-unknown",
            description: "Unknown hint not in GaussDB known hint list will be silently ignored",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Dml,
            check_fn: check_c001,
        },
        LintRuleEntry {
            id: "C005",
            name: "hint-contradictory",
            description: "Contradictory or mutually exclusive hints are present and may cause unexpected behavior",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Dml,
            check_fn: check_c005,
        },
        LintRuleEntry {
            id: "C006",
            name: "hint-table-not-in-from",
            description: "Hint references a table not present in the FROM clause",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Dml,
            check_fn: check_c006,
        },
        LintRuleEntry {
            id: "C007",
            name: "update-without-where",
            description: "UPDATE without WHERE clause may affect the entire table data",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Update,
            check_fn: check_c007,
        },
        LintRuleEntry {
            id: "C008",
            name: "delete-without-where",
            description: "DELETE without WHERE clause will remove all table data",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Delete,
            check_fn: check_c008,
        },
        LintRuleEntry {
            id: "C009",
            name: "insert-no-column-list",
            description: "INSERT without explicit target column list relies on table definition order",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Insert,
            check_fn: check_c009,
        },
        LintRuleEntry {
            id: "C010",
            name: "unlogged-table",
            description: "UNLOGGED TABLE data is lost on crash or failover",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Ddl,
            check_fn: check_c010,
        },
        LintRuleEntry {
            id: "C011",
            name: "goto-statement",
            description: "GOTO violates structured programming best practices; use IF/EXIT/CONTINUE instead",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c011,
        },
        LintRuleEntry {
            id: "C012",
            name: "execute-concat-sql-injection",
            description: "EXECUTE IMMEDIATE with string concatenation risks SQL injection; use USING parameterization",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c012,
        },
        LintRuleEntry {
            id: "C013",
            name: "exception-swallow",
            description: "WHEN OTHERS handler without re-raising silently swallows errors",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c013,
        },
        LintRuleEntry {
            id: "C014",
            name: "pl-commit-rollback",
            description: "COMMIT/ROLLBACK inside PL/pgSQL blocks complicates transaction control",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c014,
        },
        LintRuleEntry {
            id: "C015",
            name: "select-for-update-blocking",
            description: "SELECT ... FOR UPDATE without NOWAIT/SKIP LOCKED may cause long blocking",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Select,
            check_fn: check_c015,
        },
        LintRuleEntry {
            id: "C016",
            name: "autonomous-transaction",
            description: "PRAGMA AUTONOMOUS_TRANSACTION creates independent transactions with high overhead",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c016,
        },
        LintRuleEntry {
            id: "C017",
            name: "raise-in-exception-clears-variables",
            description: "RAISE in EXCEPTION block clears all local variable values including OUT parameters",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c017,
        },
        LintRuleEntry {
            id: "C018",
            name: "excessive-insert-values",
            description:
                "Excessive INSERT VALUES rows may cause long transactions, lock contention, and high rollback costs",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::Insert,
            check_fn: check_c018,
        },
        LintRuleEntry {
            id: "C019",
            name: "commit-inside-loop",
            description: "COMMIT/ROLLBACK inside a PL/pgSQL loop breaks transaction atomicity and harms performance",
            level: WarningLevel::Caution,
            stmt_kind: StatementKind::PlBlock,
            check_fn: check_c019,
        },
    ];
    for rule in rules {
        linter.register(rule);
    }
}

/// Extract hints from a statement (SELECT, INSERT, UPDATE, DELETE).
fn extract_hints(stmt: &Statement) -> Vec<&HintInfo> {
    match stmt {
        Statement::Select(s) => s.hints.iter().collect(),
        Statement::Insert(s) => s.hints.iter().collect(),
        Statement::Update(s) => s.hints.iter().collect(),
        Statement::Delete(s) => s.hints.iter().collect(),
        _ => vec![],
    }
}

// ── C001: Unknown hint ──

fn check_c001(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    for hint in extract_hints(&curr_stmt.statement) {
        if !KNOWN_HINTS.contains(&hint.name.as_str()) {
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C001",
                "hint-unknown",
                format!("Hint '{}' \u{4e0d}\u{5728} GaussDB \u{5df2}\u{77e5} Hint \u{5217}\u{8868}\u{4e2d}\u{ff0c}\u{5c06}\u{88ab}\u{9759}\u{9ed8}\u{5ffd}\u{7565}", hint.name),
                Some("\u{68c0}\u{67e5} Hint \u{62fc}\u{5199}\u{662f}\u{5426}\u{6b63}\u{786e}"),
                loc,
                Some("Hint \u{4f7f}\u{7528}\u{89c4}\u{8303}"),
                confidence,
            ));
        }
    }
}

// ── C005: Contradictory hints ──

fn check_c005(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let contradictory_pairs: &[(&str, &str)] = &[
        ("tablescan", "indexscan"),
        ("tablescan", "indexonlyscan"),
        ("tablescan", "bitmapscan"),
        ("indexscan", "indexonlyscan"),
        ("nestloop", "hashjoin"),
        ("nestloop", "mergejoin"),
        ("hashjoin", "mergejoin"),
        ("use_hash_agg", "use_sort_agg"),
        ("expand_sublink", "no_expand_sublink"),
        ("expand_subquery", "no_expand_subquery"),
        ("gather", "redistribute"),
    ];

    let loc = stmt_location(curr_stmt);
    let hints = extract_hints(&curr_stmt.statement);
    if hints.is_empty() {
        return;
    }

    let mut seen = std::collections::HashSet::new();
    for hint in &hints {
        let lower = hint.name.to_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        let has_pos = hints.iter().any(|h| h.name == lower && !h.negated);
        let has_neg = hints.iter().any(|h| h.name == lower && h.negated);
        if has_pos && has_neg {
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C005",
                "hint-contradictory",
                format!("Hint \u{77db}\u{76fe}: '{lower}' \u{4e0e} 'no_{lower}' \u{540c}\u{65f6}\u{5b58}\u{5728}"),
                Some("\u{79fb}\u{9664}\u{77db}\u{76fe}\u{7684} Hint"),
                loc,
                None,
                confidence,
            ));
            seen.insert(lower);
        }
    }

    // Check mutually exclusive pairs
    for (a, b) in contradictory_pairs {
        let has_a = hints.iter().any(|h| h.name == *a);
        let has_b = hints.iter().any(|h| h.name == *b);
        if has_a && has_b {
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C005",
                "hint-contradictory",
                format!("Hint \u{4e92}\u{65a5}: '{a}' \u{4e0e} '{b}' \u{4e0d}\u{5e94}\u{540c}\u{65f6}\u{5b58}\u{5728}"),
                Some("\u{53ea}\u{4fdd}\u{7559}\u{4e00}\u{4e2a}\u{626b}\u{63cf}\u{65b9}\u{5f0f}/\u{8fde}\u{63a5}\u{65b9}\u{5f0f}\u{7684} Hint"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── C006: Hint table not in FROM ──

fn check_c006(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    let from_tables = collect_from_table_names(&curr_stmt.statement);
    if from_tables.is_empty() {
        return;
    }
    for hint in extract_hints(&curr_stmt.statement) {
        let hint_tables = extract_hint_table_refs(hint);
        for ht in hint_tables {
            let lower = ht.to_lowercase();
            if !from_tables.contains(&lower) {
                warnings.push(make_warning(
                    WarningLevel::Caution,
                    "C006",
                    "hint-table-not-in-from",
                    format!("Hint \u{5f15}\u{7528}\u{7684}\u{8868} '{ht}' \u{4e0d}\u{5728} FROM \u{5b50}\u{53e5}\u{4e2d}"),
                    Some("\u{68c0}\u{67e5} Hint \u{4e2d}\u{7684}\u{8868}\u{540d}\u{662f}\u{5426}\u{4e0e} FROM \u{5b50}\u{53e5}\u{4e00}\u{81f4}"),
                    loc,
                    None,
                    confidence,
                ));
            }
        }
    }
}

fn collect_from_table_names(stmt: &Statement) -> Vec<String> {
    use crate::ast::TableRef;
    let mut tables = Vec::new();
    let from: &[TableRef] = match stmt {
        Statement::Select(s) => &s.from,
        Statement::Update(s) => {
            for t in &s.tables {
                if let TableRef::Table { name, alias, .. } = t {
                    tables.push(name.last().cloned().unwrap_or_default().to_lowercase());
                    if let Some(a) = alias {
                        tables.push(a.to_lowercase());
                    }
                }
            }
            &s.from
        }
        Statement::Delete(s) => {
            for t in &s.tables {
                if let TableRef::Table { name, alias, .. } = t {
                    tables.push(name.last().cloned().unwrap_or_default().to_lowercase());
                    if let Some(a) = alias {
                        tables.push(a.to_lowercase());
                    }
                }
            }
            &s.using
        }
        _ => return tables,
    };
    collect_table_names_from(from, &mut tables);
    tables
}

fn collect_table_names_from(from: &[crate::ast::TableRef], tables: &mut Vec<String>) {
    use crate::ast::TableRef;
    for t in from {
        match t {
            TableRef::Table { name, alias, .. } => {
                tables.push(name.last().cloned().unwrap_or_default().to_lowercase());
                if let Some(a) = alias {
                    tables.push(a.to_lowercase());
                }
            }
            TableRef::Join { left, right, .. } => {
                collect_table_names_from(std::slice::from_ref(left), tables);
                collect_table_names_from(std::slice::from_ref(right), tables);
            }
            TableRef::Subquery { alias: Some(a), .. } => {
                tables.push(a.to_lowercase());
            }
            _ => {}
        }
    }
}

/// Extract table name references from a hint's arguments.
fn extract_hint_table_refs(hint: &HintInfo) -> Vec<String> {
    let mut tables = Vec::new();
    if let Some(ref args) = hint.args {
        for arg in args.split(',') {
            let trimmed = arg.trim();
            // Skip @queryblock prefixes like @sel$1
            let name = if trimmed.starts_with('@') {
                if let Some(space_pos) = trimmed.find(char::is_whitespace) {
                    trimmed[space_pos..].trim()
                } else {
                    continue;
                }
            } else {
                trimmed
            };
            if !name.is_empty() {
                tables.push(name.to_string());
            }
        }
    }
    tables
}

// ── C007: UPDATE without WHERE ──

fn check_c007(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Update(s) = &curr_stmt.statement {
        if s.where_clause.is_none() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C007",
                "update-without-where",
                "UPDATE \u{65e0} WHERE \u{5b50}\u{53e5}\u{ff0c}\u{53ef}\u{80fd}\u{5f71}\u{54cd}\u{5168}\u{8868}\u{6570}\u{636e}".into(),
                Some("\u{786e}\u{8ba4}\u{662f}\u{5426}\u{771f}\u{7684}\u{9700}\u{8981}\u{66f4}\u{65b0}\u{5168}\u{8868}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── C008: DELETE without WHERE ──

fn check_c008(
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
                WarningLevel::Caution,
                "C008",
                "delete-without-where",
                "DELETE \u{65e0} WHERE \u{5b50}\u{53e5}\u{ff0c}\u{5c06}\u{5220}\u{9664}\u{5168}\u{8868}\u{6570}\u{636e}".into(),
                Some("\u{786e}\u{8ba4}\u{662f}\u{5426}\u{5e94}\u{4f7f}\u{7528} TRUNCATE \u{6216}\u{6dfb}\u{52a0} WHERE \u{6761}\u{4ef6}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── C009: INSERT without column list ──

fn check_c009(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Insert(s) = &curr_stmt.statement {
        if s.columns.is_empty() {
            let loc = loc_from_spanned(s, stmt_location(curr_stmt));
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C009",
                "insert-no-column-list",
                "INSERT \u{672a}\u{6307}\u{5b9a}\u{76ee}\u{6807}\u{5217}\u{540d}\u{ff0c}\u{4f9d}\u{8d56}\u{8868}\u{5b9a}\u{4e49}\u{987a}\u{5e8f}".into(),
                Some("\u{663e}\u{5f0f}\u{6307}\u{5b9a}\u{76ee}\u{6807}\u{5217}\u{540d}\u{4ee5}\u{907f}\u{514d}\u{8868}\u{7ed3}\u{6784}\u{53d8}\u{5316}\u{5bfc}\u{81f4}\u{6570}\u{636e}\u{9519}\u{8bef}"),
                loc,
                None,
                confidence,
            ));
        }
    }
}

// ── C010: Unlogged table ──

fn check_c010(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    match &curr_stmt.statement {
        Statement::CreateTable(s) if s.unlogged => {
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C010",
                "unlogged-table",
                "UNLOGGED TABLE \u{5728}\u{6545}\u{969c}\u{6062}\u{590d}\u{65f6}\u{6570}\u{636e}\u{4f1a}\u{4e22}\u{5931}".into(),
                Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{4ee5}\u{4f7f}\u{7528}\u{666e}\u{901a}\u{8868}\u{66ff}\u{4ee3}"),
                loc,
                None,
                confidence,
            ));
        }
        Statement::CreateTableAs(s) if s.unlogged => {
            warnings.push(make_warning(
                WarningLevel::Caution,
                "C010",
                "unlogged-table",
                "UNLOGGED TABLE AS \u{5728}\u{6545}\u{969c}\u{6062}\u{590d}\u{65f6}\u{6570}\u{636e}\u{4f1a}\u{4e22}\u{5931}".into(),
                Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{4ee5}\u{4f7f}\u{7528}\u{666e}\u{901a}\u{8868}\u{66ff}\u{4ee3}"),
                loc,
                None,
                confidence,
            ));
        }
        _ => {}
    }
}

// ── C011: GOTO statement ──

fn check_c011(
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
    walk_pl_block_for_goto(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C011",
            "goto-statement",
            "PL/pgSQL \u{4e2d}\u{4f7f}\u{7528}\u{4e86} GOTO \u{8bed}\u{53e5}\u{ff0c}\u{4e0d}\u{7b26}\u{5408}\u{7ed3}\u{6784}\u{5316}\u{7f16}\u{7a0b}\u{5efa}\u{8bae}".into(),
            Some("\u{4f7f}\u{7528} IF/EXIT/CONTINUE \u{66ff}\u{4ee3} GOTO"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_block_for_goto(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_stmts_for_goto(&block.body, found);
        if *found {
            return;
        }
        if let Some(ref exc) = block.exception_block {
            for handler in &exc.handlers {
                check_pl_stmts_for_goto(&handler.statements, found);
                if *found {
                    return;
                }
            }
        }
    }
}

fn check_pl_stmts_for_goto(stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in stmts {
        match s {
            PlStatement::Goto { .. } => {
                *found = true;
                return;
            }
            PlStatement::Block(b) => {
                check_pl_stmts_for_goto(&b.body, found);
                if let Some(ref exc) = b.exception_block {
                    for handler in &exc.handlers {
                        check_pl_stmts_for_goto(&handler.statements, found);
                    }
                }
            }
            PlStatement::If(i) => {
                check_pl_stmts_for_goto(&i.then_stmts, found);
                for e in &i.elsifs {
                    check_pl_stmts_for_goto(&e.stmts, found);
                }
                check_pl_stmts_for_goto(&i.else_stmts, found);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_goto(&w.stmts, found);
                }
                check_pl_stmts_for_goto(&c.else_stmts, found);
            }
            PlStatement::Loop(l) => check_pl_stmts_for_goto(&l.body, found),
            PlStatement::While(w) => check_pl_stmts_for_goto(&w.body, found),
            PlStatement::For(f) => check_pl_stmts_for_goto(&f.body, found),
            PlStatement::ForEach(f) => check_pl_stmts_for_goto(&f.body, found),
            _ => {}
        }
        if *found {
            return;
        }
    }
}

// ── C012: EXECUTE with string concatenation (SQL injection risk) ──

fn check_c012(
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
    walk_pl_for_execute_concat(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C012",
            "execute-concat-sql-injection",
            "EXECUTE IMMEDIATE \u{4e2d}\u{4f7f}\u{7528}\u{5b57}\u{7b26}\u{4e32}\u{62fc}\u{63a5}\u{ff0c}\u{53ef}\u{80fd}\u{5b58}\u{5728} SQL \u{6ce8}\u{5165}\u{98ce}\u{9669}".into(),
            Some("\u{4f7f}\u{7528} USING \u{53c2}\u{6570}\u{5316}\u{67e5}\u{8be2}\u{66ff}\u{4ee3}\u{5b57}\u{7b26}\u{4e32}\u{62fc}\u{63a5}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_execute_concat(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_stmts_for_execute_concat(&block.body, found);
        if *found {
            return;
        }
    }
}

fn check_pl_stmts_for_execute_concat(stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in stmts {
        match s {
            PlStatement::Execute(e) => {
                if has_string_concat(&e.string_expr) {
                    *found = true;
                    return;
                }
            }
            PlStatement::Block(b) => {
                check_pl_stmts_for_execute_concat(&b.body, found);
            }
            PlStatement::If(i) => {
                check_pl_stmts_for_execute_concat(&i.then_stmts, found);
                for e in &i.elsifs {
                    check_pl_stmts_for_execute_concat(&e.stmts, found);
                }
                check_pl_stmts_for_execute_concat(&i.else_stmts, found);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_execute_concat(&w.stmts, found);
                }
                check_pl_stmts_for_execute_concat(&c.else_stmts, found);
            }
            PlStatement::Loop(l) => check_pl_stmts_for_execute_concat(&l.body, found),
            PlStatement::While(w) => check_pl_stmts_for_execute_concat(&w.body, found),
            PlStatement::For(f) => check_pl_stmts_for_execute_concat(&f.body, found),
            PlStatement::ForEach(f) => check_pl_stmts_for_execute_concat(&f.body, found),
            _ => {}
        }
        if *found {
            return;
        }
    }
}

fn has_string_concat(expr: &Expr) -> bool {
    let mut found = false;
    walk_expr(expr, &mut |e| {
        if found {
            return false;
        }
        if let Expr::BinaryOp { op, .. } = e {
            if op == "||" {
                found = true;
                return false;
            }
        }
        true
    });
    found
}

// ── C013: Exception handler swallows errors ──

fn check_c013(
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
    walk_pl_for_exception_swallow(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C013",
            "exception-swallow",
            "WHEN OTHERS THEN \u{5f02}\u{5e38}\u{5904}\u{7406}\u{4e2d}\u{672a}\u{91cd}\u{65b0}\u{629b}\u{51fa}\u{5f02}\u{5e38}\u{ff0c}\u{53ef}\u{80fd}\u{9759}\u{9ed8}\u{541e}\u{9519}".into(),
            Some("\u{5728} WHEN OTHERS \u{5904}\u{7406}\u{4e2d}\u{6dfb}\u{52a0} RAISE \u{91cd}\u{65b0}\u{629b}\u{51fa}\u{5f02}\u{5e38}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_exception_swallow(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_block_for_swallow(block, found);
        if *found {
            return;
        }
    }
}

fn check_block_for_swallow(block: &PlBlock, found: &mut bool) {
    if *found {
        return;
    }
    if let Some(ref exc) = block.exception_block {
        for handler in &exc.handlers {
            if handler.conditions.iter().any(|c| c.eq_ignore_ascii_case("others"))
                && !handler.statements.iter().any(has_raise)
            {
                *found = true;
                return;
            }
        }
    }
    // Recurse into nested blocks
    check_pl_stmts_for_swallow(&block.body, found);
}

fn check_pl_stmts_for_swallow(stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in stmts {
        match s {
            PlStatement::Block(b) => check_block_for_swallow(b, found),
            PlStatement::If(i) => {
                check_pl_stmts_for_swallow(&i.then_stmts, found);
                for e in &i.elsifs {
                    check_pl_stmts_for_swallow(&e.stmts, found);
                }
                check_pl_stmts_for_swallow(&i.else_stmts, found);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_swallow(&w.stmts, found);
                }
                check_pl_stmts_for_swallow(&c.else_stmts, found);
            }
            PlStatement::Loop(l) => check_pl_stmts_for_swallow(&l.body, found),
            PlStatement::While(w) => check_pl_stmts_for_swallow(&w.body, found),
            PlStatement::For(f) => check_pl_stmts_for_swallow(&f.body, found),
            _ => {}
        }
        if *found {
            return;
        }
    }
}

fn has_raise(s: &PlStatement) -> bool {
    matches!(s, PlStatement::Raise(r) if r.level.as_ref().map_or(true, |l| !matches!(l, RaiseLevel::Debug | RaiseLevel::Log)))
}

// ── C014: PL block COMMIT/ROLLBACK ──

fn check_c014(
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
    walk_pl_for_commit_rollback(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C014",
            "pl-commit-rollback",
            "PL/pgSQL \u{5757}\u{4e2d}\u{5305}\u{542b} COMMIT/ROLLBACK\u{ff0c}\u{53ef}\u{80fd}\u{590d}\u{6742}\u{5316}\u{4e8b}\u{52a1}\u{63a7}\u{5236}".into(),
            Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{53ef}\u{4ee5}\u{5c06}\u{4e8b}\u{52a1}\u{63a7}\u{5236}\u{4ea4}\u{7ed9}\u{5916}\u{5c42}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_commit_rollback(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_stmts_for_commit_rollback(&block.body, found);
        if *found {
            return;
        }
    }
}

fn check_pl_stmts_for_commit_rollback(pl_stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in pl_stmts {
        match s {
            PlStatement::Commit { .. } | PlStatement::Rollback { .. } => {
                *found = true;
                return;
            }
            PlStatement::Block(b) => {
                check_pl_stmts_for_commit_rollback(&b.body, found);
            }
            PlStatement::If(i) => {
                check_pl_stmts_for_commit_rollback(&i.then_stmts, found);
                for e in &i.elsifs {
                    check_pl_stmts_for_commit_rollback(&e.stmts, found);
                }
                check_pl_stmts_for_commit_rollback(&i.else_stmts, found);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_commit_rollback(&w.stmts, found);
                }
                check_pl_stmts_for_commit_rollback(&c.else_stmts, found);
            }
            PlStatement::Loop(l) => check_pl_stmts_for_commit_rollback(&l.body, found),
            PlStatement::While(w) => check_pl_stmts_for_commit_rollback(&w.body, found),
            PlStatement::For(f) => check_pl_stmts_for_commit_rollback(&f.body, found),
            _ => {}
        }
        if *found {
            return;
        }
    }
}

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

// ── C015: SELECT FOR UPDATE blocking ──

fn check_c015(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    _config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    if let Statement::Select(s) = &curr_stmt.statement {
        if let Some(ref lock) = s.lock_clause {
            let is_blocking = match lock {
                LockClause::Update { nowait: false, skip_locked: false, wait, .. }
                | LockClause::Share { nowait: false, skip_locked: false, wait, .. }
                | LockClause::NoKeyUpdate { nowait: false, skip_locked: false, wait, .. }
                | LockClause::KeyShare { nowait: false, skip_locked: false, wait, .. } => wait.is_none(),
                _ => false,
            };
            if is_blocking {
                let loc = loc_from_spanned(s, stmt_location(curr_stmt));
                warnings.push(make_warning(
                    WarningLevel::Caution,
                    "C015",
                    "select-for-update-blocking",
                    "SELECT ... FOR UPDATE \u{672a}\u{4f7f}\u{7528} NOWAIT/SKIP LOCKED\u{ff0c}\u{53ef}\u{80fd}\u{957f}\u{65f6}\u{95f4}\u{963b}\u{585e}".into(),
                    Some("\u{8003}\u{8651}\u{4f7f}\u{7528} NOWAIT \u{6216} SKIP LOCKED"),
                    loc,
                    None,
                    confidence,
                ));
            }
        }
    }
}

// ── C016: Autonomous transaction pragma ──

fn check_c016(
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
    walk_pl_for_autonomous(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C016",
            "autonomous-transaction",
            "PRAGMA AUTONOMOUS_TRANSACTION \u{4f1a}\u{521b}\u{5efa}\u{72ec}\u{7acb}\u{4e8b}\u{52a1}\u{ff0c}\u{6027}\u{80fd}\u{5f00}\u{9500}\u{8f83}\u{5927}".into(),
            Some("\u{8bc4}\u{4f30}\u{662f}\u{5426}\u{771f}\u{7684}\u{9700}\u{8981}\u{72ec}\u{7acb}\u{4e8b}\u{52a1}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_autonomous(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_pl_decls_for_autonomous(&block.declarations, found);
        if !*found {
            check_pl_stmts_for_autonomous(&block.body, found);
        }
        if *found {
            return;
        }
    }
}

fn check_pl_decls_for_autonomous(decls: &[PlDeclaration], found: &mut bool) {
    for d in decls {
        if let PlDeclaration::Pragma { name, .. } = d {
            if name.eq_ignore_ascii_case("AUTONOMOUS_TRANSACTION") {
                *found = true;
                return;
            }
        }
    }
}

fn check_pl_stmts_for_autonomous(pl_stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in pl_stmts {
        if let PlStatement::Block(b) = s {
            check_pl_decls_for_autonomous(&b.declarations, found);
            if !*found {
                check_pl_stmts_for_autonomous(&b.body, found);
            }
        }
        if *found {
            return;
        }
    }
}

// ── C017: RAISE in EXCEPTION clears variables ──

fn check_c017(
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
    walk_pl_for_raise_in_exception(&curr_stmt.statement, &mut found);
    if found {
        warnings.push(make_warning(
            WarningLevel::Caution,
            "C017",
            "raise-in-exception-clears-variables",
            "RAISE \u{5728} EXCEPTION \u{5757}\u{4e2d}\u{4f1a}\u{6e05}\u{7a7a}\u{6240}\u{6709}\u{5c40}\u{90e8}\u{53d8}\u{91cf}\u{503c}\u{ff08}\u{5305}\u{62ec} OUT \u{53c2}\u{6570}\u{ff09}\u{ff0c}\u{5bfc}\u{81f4}\u{8c03}\u{7528}\u{65b9}\u{65e0}\u{6cd5}\u{83b7}\u{53d6}\u{9519}\u{8bef}\u{4fe1}\u{606f}".into(),
            Some("\u{4f7f}\u{7528} RAISE INFO \u{4f20}\u{9012}\u{5177}\u{4f53}\u{9519}\u{8bef}\u{4fe1}\u{606f}\u{ff0c}\u{6216}\u{5728} RAISE \u{524d}\u{5c06}\u{8f93}\u{51fa}\u{503c}\u{4fdd}\u{5b58}\u{5230}\u{4e34}\u{65f6}\u{8868}/\u{5168}\u{5c40}\u{53d8}\u{91cf}"),
            loc,
            None,
            confidence,
        ));
    }
}

fn walk_pl_for_raise_in_exception(stmt: &Statement, found: &mut bool) {
    for block in crate::linter::pl_blocks_from_stmt(stmt) {
        check_exception_block_for_reraise(block, found);
        if *found {
            return;
        }
    }
}

fn check_exception_block_for_reraise(block: &PlBlock, found: &mut bool) {
    if *found {
        return;
    }
    if let Some(ref exc) = block.exception_block {
        for handler in &exc.handlers {
            if handler.statements.iter().any(has_reraise) {
                *found = true;
                return;
            }
        }
    }
    // Recurse into nested blocks
    check_pl_stmts_for_reraise(&block.body, found);
}

fn check_pl_stmts_for_reraise(stmts: &[PlStatement], found: &mut bool) {
    if *found {
        return;
    }
    for s in stmts {
        match s {
            PlStatement::Block(b) => check_exception_block_for_reraise(b, found),
            PlStatement::If(i) => {
                check_pl_stmts_for_reraise(&i.then_stmts, found);
                for e in &i.elsifs {
                    check_pl_stmts_for_reraise(&e.stmts, found);
                }
                check_pl_stmts_for_reraise(&i.else_stmts, found);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    check_pl_stmts_for_reraise(&w.stmts, found);
                }
                check_pl_stmts_for_reraise(&c.else_stmts, found);
            }
            PlStatement::Loop(l) => check_pl_stmts_for_reraise(&l.body, found),
            PlStatement::While(w) => check_pl_stmts_for_reraise(&w.body, found),
            PlStatement::For(f) => check_pl_stmts_for_reraise(&f.body, found),
            PlStatement::ForEach(f) => check_pl_stmts_for_reraise(&f.body, found),
            _ => {}
        }
        if *found {
            return;
        }
    }
}

/// Returns true if the statement is a re-raise (bare RAISE or RAISE EXCEPTION).
/// These propagate the exception and clear local variables in GaussDB.
fn has_reraise(s: &PlStatement) -> bool {
    matches!(
        s,
        PlStatement::Raise(r) if r.level.is_none() || matches!(r.level, Some(RaiseLevel::Exception))
    )
}

// ── C018: Excessive INSERT VALUES rows ──

fn check_c018(
    curr_stmt: &StatementInfo,
    _stmts: &[StatementInfo],
    _schema: Option<&crate::analyzer::schema::SchemaMap>,
    _indexes: Option<&crate::linter::IndexInfo>,
    config: &LintConfig,
    confidence: Confidence,
    warnings: &mut Vec<SqlWarning>,
) {
    let loc = stmt_location(curr_stmt);
    if let Statement::Insert(insert) = &curr_stmt.statement {
        if let InsertSource::Values(values) = &insert.source {
            let row_count = values.len();
            let col_count = if !insert.columns.is_empty() {
                insert.columns.len()
            } else if let Some(first) = values.first() {
                first.len()
            } else {
                0
            };
            let total_params = row_count * col_count;
            if total_params > config.max_insert_values_rows {
                warnings.push(make_warning(
                    WarningLevel::Caution,
                    "C018",
                    "excessive-insert-values",
                    format!(
                        "INSERT VALUES {row_count} 行 × {col_count} 列 = {total_params} 个绑定参数，超过阈值 {}。大批量可能导致长事务、锁竞争和回滚代价过高。建议每批插入 {}—{} 行，或使用 COPY",
                        config.max_insert_values_rows,
                        config.max_insert_values_rows / col_count.max(1) / 5,
                        config.max_insert_values_rows / col_count.max(1),
                    ),
                    Some("拆分为更小批次插入以减少锁持有时间"),
                    loc,
                    None,
                    confidence,
                ));
            }
        }
    }
}
