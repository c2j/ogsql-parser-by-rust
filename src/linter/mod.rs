mod rules_caution;
mod rules_performance;
mod rules_prohibition;
mod rules_suggestion;
#[cfg(feature = "ibatis")]
pub mod structured;
mod type_helpers;

#[cfg(test)]
mod tests;

use crate::analyzer::schema::{IndexMapV2, SchemaMap};
use crate::ast::{SelectStatement, Spanned, Statement, StatementInfo};
use crate::token::SourceLocation;
use std::collections::{HashMap, HashSet};
#[cfg(feature = "lint-config")]
use std::path::{Path, PathBuf};

/// SQL warning severity level. "Error" is deliberately NOT used to avoid
/// confusion with `ParserError` which represents syntax-level errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum WarningLevel {
    /// Prohibition -- violates GaussDB "rule"-level requirements; may cause data safety risks.
    Prohibition = 3,
    /// Performance -- identifiable performance pitfall with a clear optimization path.
    Performance = 2,
    /// Caution -- syntactically legal but easy to overlook; requires contextual judgment.
    Caution = 1,
    /// Suggestion -- improves maintainability and robustness; does not affect correctness.
    Suggestion = 0,
}

impl std::fmt::Display for WarningLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningLevel::Prohibition => write!(f, "prohibition"),
            WarningLevel::Performance => write!(f, "performance"),
            WarningLevel::Caution => write!(f, "caution"),
            WarningLevel::Suggestion => write!(f, "suggestion"),
        }
    }
}

/// Detection confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Confidence {
    /// Partial confidence (lower value) -- iBatis/MyBatis dynamic SQL.
    Partial,
    /// Full confidence (higher value) -- complete SQL.
    Full,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Full => write!(f, "full"),
            Confidence::Partial => write!(f, "partial"),
        }
    }
}

/// A single SQL warning produced by the linter.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SqlWarning {
    /// Severity level.
    pub level: WarningLevel,
    /// Rule identifier, e.g. "R001", "P002".
    pub rule_id: String,
    /// Human-readable rule name.
    pub rule_name: String,
    /// Warning description with SQL context.
    pub message: String,
    /// Optimization suggestion (optional).
    pub suggestion: Option<String>,
    /// Source location of the offending construct.
    pub location: SourceLocation,
    /// GaussDB documentation reference (optional).
    pub gaussdb_ref: Option<String>,
    /// Detection confidence.
    pub confidence: Confidence,
}

/// Statement category used by `SqlLinter` for first-level dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Select,
    Update,
    Delete,
    Insert,
    Merge,
    Dml,
    Ddl,
    PlBlock,
    All,
}

impl StatementKind {
    /// Returns true if `self` matches `other` considering the broad Dml/Ddl/All categories.
    pub fn matches(self, other: StatementKind) -> bool {
        match (self, other) {
            (StatementKind::All, _) | (_, StatementKind::All) => true,
            (
                StatementKind::Dml,
                StatementKind::Select
                | StatementKind::Update
                | StatementKind::Delete
                | StatementKind::Insert
                | StatementKind::Merge,
            ) => true,
            (StatementKind::Ddl, StatementKind::Ddl) => true,
            _ => self == other,
        }
    }
}

/// Linter configuration.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LintConfig {
    pub min_level: WarningLevel,
    pub min_confidence: Confidence,
    pub suppress: Vec<String>,
    /// P003 -- IN list size threshold (default 500).
    pub in_list_threshold: usize,
    /// P014 -- subquery nesting depth limit (default 3).
    pub subquery_depth_limit: usize,
    /// S008 -- SQL text length limit (default 4096, aligned with GaussDB track_activity_query_size).
    pub sql_length_limit: usize,
    /// P007 -- non-equi join count limit (default 2).
    pub non_equi_join_limit: usize,
    /// R002 -- GROUP BY column count limit (default 10).
    pub group_by_column_limit: usize,
    /// C018 -- max bind parameters (rows × columns) for INSERT VALUES (default 65535,
    /// aligned with database bind-parameter limit).
    pub max_insert_values_rows: usize,
    /// C018 (structured variant) — representative iteration count for
    /// `<foreach>` collections when estimating dynamic-SQL bind parameter
    /// totals. Defaults to 1000.
    pub foreach_estimated_rows: usize,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            min_level: WarningLevel::Suggestion,
            min_confidence: Confidence::Partial,
            suppress: vec![],
            in_list_threshold: 500,
            subquery_depth_limit: 3,
            sql_length_limit: 4096,
            non_equi_join_limit: 2,
            group_by_column_limit: 10,
            max_insert_values_rows: 65535,
            foreach_estimated_rows: 1000,
        }
    }
}

/// TOML config file structure mirroring `LintConfig` with all-optional fields
/// so that partial configuration files are valid.
///
/// # Example
///
/// ```toml
/// # .ogsql-lint.toml
/// min_level = "caution"
/// min_confidence = "full"
/// suppress = ["R001", "S007"]
/// in_list_threshold = 200
/// subquery_depth_limit = 5
/// ```
#[cfg(feature = "lint-config")]
#[derive(serde::Deserialize)]
pub struct LintConfigFile {
    pub min_level: Option<String>,
    pub min_confidence: Option<String>,
    pub suppress: Option<Vec<String>>,
    pub in_list_threshold: Option<usize>,
    pub subquery_depth_limit: Option<usize>,
    pub sql_length_limit: Option<usize>,
    pub non_equi_join_limit: Option<usize>,
    pub group_by_column_limit: Option<usize>,
    pub max_insert_values_rows: Option<usize>,
    pub foreach_estimated_rows: Option<usize>,
}

#[cfg(feature = "lint-config")]
impl LintConfig {
    /// Parse a TOML string and merge with defaults.
    /// Unknown TOML fields are silently ignored by serde.
    pub fn from_toml_str(s: &str) -> Result<Self, String> {
        let file: LintConfigFile = toml::from_str(s).map_err(|e| format!("invalid TOML config: {e}"))?;
        let mut config = Self::default();
        config.merge_file(file);
        Ok(config)
    }

    /// Load configuration from a TOML file.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
        Self::from_toml_str(&content)
    }

    /// Search standard paths for a lint config file and return the first found.
    ///
    /// Search order:
    /// 1. `.ogsql-lint.toml` in the current working directory
    /// 2. `~/.config/ogsql/lint.toml` (XDG convention)
    ///
    /// Returns `Ok(None)` when no config file is found.
    pub fn find_and_load() -> Result<Option<Self>, String> {
        let cwd = std::env::current_dir().map_err(|e| format!("cannot get cwd: {e}"))?;
        let local = cwd.join(".ogsql-lint.toml");
        if local.is_file() {
            return Self::load_from_file(&local).map(Some);
        }

        let xdg_base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")));
        if let Some(base) = xdg_base {
            let xdg = base.join("ogsql").join("lint.toml");
            if xdg.is_file() {
                return Self::load_from_file(&xdg).map(Some);
            }
        }

        Ok(None)
    }

    /// Apply values from a `LintConfigFile` onto `self`.
    /// Only fields that are `Some` in the file override the current value.
    pub fn merge_file(&mut self, file: LintConfigFile) {
        if let Some(ref level) = file.min_level {
            self.min_level = match level.to_lowercase().as_str() {
                "prohibition" => WarningLevel::Prohibition,
                "performance" => WarningLevel::Performance,
                "caution" => WarningLevel::Caution,
                _ => WarningLevel::Suggestion,
            };
        }
        if let Some(ref conf) = file.min_confidence {
            self.min_confidence = match conf.to_lowercase().as_str() {
                "full" => Confidence::Full,
                _ => Confidence::Partial,
            };
        }
        if let Some(suppress) = file.suppress {
            self.suppress = suppress;
        }
        if let Some(v) = file.in_list_threshold {
            self.in_list_threshold = v;
        }
        if let Some(v) = file.subquery_depth_limit {
            self.subquery_depth_limit = v;
        }
        if let Some(v) = file.sql_length_limit {
            self.sql_length_limit = v;
        }
        if let Some(v) = file.non_equi_join_limit {
            self.non_equi_join_limit = v;
        }
        if let Some(v) = file.group_by_column_limit {
            self.group_by_column_limit = v;
        }
        if let Some(v) = file.max_insert_values_rows {
            self.max_insert_values_rows = v;
        }
        if let Some(v) = file.foreach_estimated_rows {
            self.foreach_estimated_rows = v;
        }
    }
}

/// Pre-computed index metadata for index-aware lint rules.
pub struct IndexInfo {
    /// Raw index definitions: table → index_name → [col_or_expr, ...]
    pub indexes: IndexMapV2,
    /// Fast lookup: table (lowercase) → set of column names (lowercase) that have a plain index.
    pub column_indexes: HashMap<String, HashSet<String>>,
}

/// A single registered rule.
#[allow(clippy::type_complexity)]
pub struct LintRuleEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub level: WarningLevel,
    pub stmt_kind: StatementKind,
    pub check_fn: fn(
        curr_stmt: &StatementInfo,
        stmts: &[StatementInfo],
        Option<&SchemaMap>,
        Option<&IndexInfo>,
        &LintConfig,
        Confidence,
        &mut Vec<SqlWarning>,
    ),
}

/// Build a JSON summary of lint warnings, grouped by level and rule.
pub fn build_lint_summary(warnings: &[SqlWarning]) -> serde_json::Value {
    let mut by_level = std::collections::BTreeMap::new();
    let mut by_rule = std::collections::BTreeMap::new();
    for w in warnings {
        *by_level.entry(format!("{}", w.level)).or_insert(0usize) += 1;
        *by_rule.entry(w.rule_id.clone()).or_insert(0usize) += 1;
    }
    serde_json::json!({
        "total": warnings.len(),
        "by_level": by_level,
        "by_rule": by_rule,
    })
}

pub(crate) fn loc_from_spanned<T>(spanned: &Spanned<T>, fallback: SourceLocation) -> SourceLocation {
    spanned.span.as_ref().map_or(fallback, |s| s.start)
}

/// SQL linter orchestrator. Registers rules and dispatches them.
pub struct SqlLinter {
    rules: Vec<LintRuleEntry>,
    config: LintConfig,
    index_info: Option<IndexInfo>,
}

impl SqlLinter {
    pub fn new(config: LintConfig) -> Self {
        Self { rules: vec![], config, index_info: None }
    }

    /// Create a linter with all Phase 1 rules registered.
    pub fn with_default_rules(config: LintConfig) -> Self {
        let mut linter = Self::new(config);
        rules_prohibition::register(&mut linter);
        rules_performance::register(&mut linter);
        rules_caution::register(&mut linter);
        rules_suggestion::register(&mut linter);
        linter
    }

    pub fn register(&mut self, rule: LintRuleEntry) {
        self.rules.push(rule);
    }

    /// Set index metadata for index-aware rules (R006, R007, etc.).
    pub fn set_index_info(&mut self, indexes: IndexMapV2) {
        let column_indexes = crate::analyzer::schema::build_column_index_set(&indexes);
        self.index_info = Some(IndexInfo { indexes, column_indexes });
    }

    /// Run all registered rules against the given statements.
    pub fn lint(&self, stmts: &[StatementInfo], schema: Option<&SchemaMap>, confidence: Confidence) -> Vec<SqlWarning> {
        let mut warnings = Vec::new();
        if confidence < self.config.min_confidence {
            return warnings;
        }

        for info in stmts {
            let kind = classify_statement(&info.statement);
            for rule in &self.rules {
                if !rule.stmt_kind.matches(kind) {
                    continue;
                }
                if self.config.suppress.iter().any(|s| s == rule.id) {
                    continue;
                }
                if rule.level < self.config.min_level {
                    continue;
                }
                (rule.check_fn)(info, stmts, schema, self.index_info.as_ref(), &self.config, confidence, &mut warnings);
            }
        }

        // Sort by severity (highest first), then by rule_id for stable output.
        warnings.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.rule_id.cmp(&b.rule_id)));
        warnings
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &LintConfig {
        &self.config
    }
}

/// Return metadata for all 53 lint rules without requiring a `LintConfig`.
pub fn all_rules_metadata() -> Vec<LintRuleEntry> {
    let mut linter = SqlLinter::new(LintConfig::default());
    rules_prohibition::register(&mut linter);
    rules_performance::register(&mut linter);
    rules_caution::register(&mut linter);
    rules_suggestion::register(&mut linter);
    linter.rules
}

fn classify_statement(stmt: &Statement) -> StatementKind {
    match stmt {
        Statement::Select(_) => StatementKind::Select,
        Statement::Insert(_) | Statement::Replace(_) => StatementKind::Insert,
        Statement::InsertAll(_) | Statement::InsertFirst(_) => StatementKind::Insert,
        Statement::Update(_) => StatementKind::Update,
        Statement::Delete(_) => StatementKind::Delete,
        Statement::Merge(_) => StatementKind::Merge,
        Statement::Values(_) => StatementKind::Select,
        Statement::AnonyBlock(_) | Statement::Do(_) => StatementKind::PlBlock,
        // CREATE FUNCTION/PROCEDURE/PACKAGE BODY carry PL/pgSQL bodies; classify
        // as PlBlock so PL rules traverse the embedded blocks (issue #296).
        Statement::CreateFunction(_) | Statement::CreateProcedure(_) | Statement::CreatePackageBody(_) => {
            StatementKind::PlBlock
        }
        Statement::Lock(_) => StatementKind::All, // LockStatement is its own category
        Statement::Drop(_) => StatementKind::Ddl,
        Statement::Explain(_) => StatementKind::All,
        Statement::Truncate(_)
        | Statement::CreateTable(_)
        | Statement::CreateTableAs(_)
        | Statement::AlterTable(_)
        | Statement::AlterTablespace(_)
        | Statement::CreateIndex(_)
        | Statement::CreateGlobalIndex(_)
        | Statement::CreateSchema(_)
        | Statement::CreateDatabase(_)
        | Statement::CreateDatabaseLink(_)
        | Statement::CreateTablespace(_)
        | Statement::CreateType(_)
        | Statement::AlterIndex(_)
        | Statement::CreatePackage(_)
        | Statement::CreateView(_)
        | Statement::CreateMaterializedView(_)
        | Statement::CreateSequence(_)
        | Statement::CreateTrigger(_)
        | Statement::CreateExtension(_)
        | Statement::CreateRole(_)
        | Statement::CreateUser(_)
        | Statement::CreateGroup(_)
        | Statement::Grant(_)
        | Statement::Revoke(_)
        | Statement::Transaction(_)
        | Statement::Copy(_)
        | Statement::Vacuum(_)
        | Statement::VariableSet(_)
        | Statement::VariableShow(_)
        | Statement::VariableReset(_)
        | Statement::Call(_)
        | Statement::Comment(_)
        | Statement::AlterFunction(_)
        | Statement::AlterProcedure(_)
        | Statement::AlterSchema(_)
        | Statement::AlterDatabase(_)
        | Statement::AlterRole(_)
        | Statement::AlterUser(_)
        | Statement::AlterGroup(_)
        | Statement::CreateAggregate(_)
        | Statement::CreateOperator(_)
        | Statement::AlterSequence(_)
        | Statement::AlterExtension(_)
        | Statement::AlterView(_)
        | Statement::AlterTrigger(_)
        | Statement::CreateCast(_)
        | Statement::CreateConversion(_)
        | Statement::CreateDomain(_)
        | Statement::AlterDomain(_)
        | Statement::CreateForeignTable(_)
        | Statement::CreateForeignServer(_)
        | Statement::CreateFdw(_)
        | Statement::CreatePublication(_)
        | Statement::CreateSubscription(_)
        | Statement::CreateSynonym(_)
        | Statement::CreateModel(_)
        | Statement::CreateAm(_)
        | Statement::CreateDirectory(_)
        | Statement::CreateNode(_)
        | Statement::CreateNodeGroup(_)
        | Statement::CreateResourcePool(_)
        | Statement::CreateWorkloadGroup(_)
        | Statement::CreateAuditPolicy(_)
        | Statement::CreateMaskingPolicy(_)
        | Statement::CreateRlsPolicy(_)
        | Statement::CreateDataSource(_)
        | Statement::CreateEvent(_)
        | Statement::CreateOpClass(_)
        | Statement::CreateOpFamily(_)
        | Statement::CreateLanguage(_)
        | Statement::CreateWeakPasswordDictionaryWithValues(_)
        | Statement::AlterNode(_)
        | Statement::AlterNodeGroup(_)
        | Statement::AlterResourcePool(_)
        | Statement::AlterWorkloadGroup(_)
        | Statement::AlterAuditPolicy(_)
        | Statement::AlterMaskingPolicy(_)
        | Statement::AlterRlsPolicy(_)
        | Statement::AlterDataSource(_)
        | Statement::AlterEvent(_)
        | Statement::AlterOpFamily(_)
        | Statement::AlterOperator(_)
        | Statement::AlterMaterializedView(_)
        | Statement::AlterGlobalConfig(_)
        | Statement::RefreshMaterializedView(_)
        | Statement::Reindex(_)
        | Statement::Analyze(_)
        | Statement::Cluster(_)
        | Statement::AlterDefaultPrivileges(_)
        | Statement::CreateUserMapping(_)
        | Statement::AlterUserMapping(_)
        | Statement::DropUserMapping(_)
        | Statement::AlterCompositeType(_)
        | Statement::AlterForeignTable(_)
        | Statement::AlterForeignServer(_)
        | Statement::AlterFdw(_)
        | Statement::AlterPublication(_)
        | Statement::AlterSubscription(_)
        | Statement::AlterSynonym(_)
        | Statement::AlterDatabaseLink(_)
        | Statement::AlterDirectory(_)
        | Statement::AlterLanguage(_)
        | Statement::AlterLargeObject(_)
        | Statement::AlterPackage(_)
        | Statement::AlterSession(_)
        | Statement::AlterSystemKillSession(_)
        | Statement::CreateStream(_)
        | Statement::CreatePolicyLabel(_)
        | Statement::AlterPolicyLabel(_)
        | Statement::DropPolicyLabel(_)
        | Statement::GrantRole(_)
        | Statement::RevokeRole(_)
        | Statement::AlterCoordinator(_)
        | Statement::AlterAppWorkloadGroupMapping(_)
        | Statement::AlterTextSearchConfig(_)
        | Statement::AlterTextSearchDict(_)
        | Statement::CreateTextSearchConfig(_)
        | Statement::CreateTextSearchDict(_)
        | Statement::AlterTextSearchConfigFull(_)
        | Statement::AlterTextSearchDictFull(_)
        | Statement::CreateAppWorkloadGroupMapping(_)
        | Statement::DropAppWorkloadGroupMapping(_)
        | Statement::Rule(_)
        | Statement::DropRule(_)
        | Statement::RemovePackage(_)
        | Statement::Shutdown(_)
        | Statement::Barrier(_)
        | Statement::Purge(_)
        | Statement::TimeCapsule(_)
        | Statement::Snapshot(_)
        | Statement::Shrink(_)
        | Statement::Verify(_)
        | Statement::CleanConn(_)
        | Statement::Compile(_)
        | Statement::SecLabel(_)
        | Statement::CreateKey(_)
        | Statement::CreateContQuery(_)
        | Statement::ExpdpDatabase(_)
        | Statement::ExpdpTable(_)
        | Statement::ImpdpDatabase(_)
        | Statement::ImpdpTable(_)
        | Statement::ReassignOwned(_)
        | Statement::Move(_)
        | Statement::LockBuckets(_)
        | Statement::MarkBuckets(_)
        | Statement::SetSessionAuthorization(_)
        | Statement::PredictBy(_) => StatementKind::Ddl,
        Statement::Empty | Statement::Checkpoint | Statement::Abort => StatementKind::All,
        _ => StatementKind::All,
    }
}

/// Helper to construct a `SqlWarning`.
pub fn make_warning(
    level: WarningLevel,
    rule_id: &str,
    rule_name: &str,
    message: String,
    suggestion: Option<&str>,
    location: SourceLocation,
    gaussdb_ref: Option<&str>,
    confidence: Confidence,
) -> SqlWarning {
    SqlWarning {
        level,
        rule_id: rule_id.to_string(),
        rule_name: rule_name.to_string(),
        message,
        suggestion: suggestion.map(|s| s.to_string()),
        location,
        gaussdb_ref: gaussdb_ref.map(|s| s.to_string()),
        confidence,
    }
}

/// Recursively walk an expression tree, calling `f` on every `Expr` node.
/// Stops if `f` returns `false`.
pub fn walk_expr(expr: &crate::ast::Expr, f: &mut dyn FnMut(&crate::ast::Expr) -> bool) {
    if !f(expr) {
        return;
    }
    use crate::ast::Expr;
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            walk_expr(left, f);
            walk_expr(right, f);
        }
        Expr::UnaryOp { expr, .. } => walk_expr(expr, f),
        Expr::Like { expr, pattern, escape, .. } => {
            walk_expr(expr, f);
            walk_expr(pattern, f);
            if let Some(e) = escape {
                walk_expr(e, f);
            }
        }
        Expr::FunctionCall { args, over, filter, within_group, .. } => {
            for a in args {
                walk_expr(a, f);
            }
            if let Some(o) = over {
                for e in &o.partition_by {
                    walk_expr(e, f);
                }
                for item in &o.order_by {
                    walk_expr(&item.expr, f);
                }
            }
            if let Some(fe) = filter {
                walk_expr(fe, f);
            }
            for item in within_group {
                walk_expr(&item.expr, f);
            }
        }
        Expr::Case { operand, whens, else_expr } => {
            if let Some(o) = operand {
                walk_expr(o, f);
            }
            for w in whens {
                walk_expr(&w.condition, f);
                walk_expr(&w.result, f);
            }
            if let Some(e) = else_expr {
                walk_expr(e, f);
            }
        }
        Expr::Between { expr, low, high, .. } => {
            walk_expr(expr, f);
            walk_expr(low, f);
            walk_expr(high, f);
        }
        Expr::InList { expr, list, .. } => {
            walk_expr(expr, f);
            for e in list {
                walk_expr(e, f);
            }
        }
        Expr::InSubquery { expr, .. } => {
            walk_expr(expr, f);
        }
        Expr::Subquery(_) | Expr::Exists(_) => {}
        Expr::ScalarSublink { expr, .. } => {
            walk_expr(expr, f);
        }
        Expr::IsNull { expr, .. } => walk_expr(expr, f),
        Expr::IsBoolean { expr, .. } => walk_expr(expr, f),
        Expr::TypeCast { expr, .. } => walk_expr(expr, f),
        Expr::Treat { expr, .. } => walk_expr(expr, f),
        Expr::CollationFor { expr } => walk_expr(expr, f),
        Expr::Array(exprs) => {
            for e in exprs {
                walk_expr(e, f);
            }
        }
        Expr::Subscript { object, lower, upper, .. } => {
            walk_expr(object, f);
            if let Some(l) = lower {
                walk_expr(l, f);
            }
            if let Some(u) = upper {
                walk_expr(u, f);
            }
        }
        Expr::FieldAccess { object, .. } => walk_expr(object, f),
        Expr::Parenthesized(e) => walk_expr(e, f),
        Expr::RowConstructor(exprs) => {
            for e in exprs {
                walk_expr(e, f);
            }
        }
        Expr::Prior(e) => walk_expr(e, f),
        Expr::SpecialFunction { args, .. } => {
            for a in args {
                walk_expr(a, f);
            }
        }
        Expr::CurrentOf { .. }
        | Expr::PredictBy { .. }
        | Expr::SysDate
        | Expr::SequenceValue { .. }
        | Expr::CursorAttribute { .. }
        | Expr::PlVariable(_)
        | Expr::Literal(_)
        | Expr::ColumnRef(_)
        | Expr::ColumnRefOuterJoin(_)
        | Expr::QualifiedStar(_)
        | Expr::Parameter(_)
        | Expr::MyBatisParam(_)
        | Expr::MyBatisRawExpr(_)
        | Expr::JdbcParam
        | Expr::Default => {}
        Expr::XmlElement { .. } => {}
        Expr::XmlConcat(exprs) => {
            for e in exprs {
                walk_expr(e, f);
            }
        }
        Expr::XmlForest(items) => {
            for item in items {
                walk_expr(&item.expr, f);
            }
        }
        Expr::XmlParse { expr, .. } => walk_expr(expr, f),
        Expr::XmlPi { content, .. } => {
            if let Some(c) = content {
                walk_expr(c, f);
            }
        }
        Expr::XmlRoot { expr, version, .. } => {
            walk_expr(expr, f);
            if let Some(v) = version {
                walk_expr(v, f);
            }
        }
        Expr::XmlSerialize { expr, .. } => walk_expr(expr, f),
        Expr::AtTimeZone { expr, zone } => {
            walk_expr(expr, f);
            walk_expr(zone, f);
        }
    }
}

/// Walk all expressions in a `SelectStatement`.
pub fn walk_select_exprs(select: &SelectStatement, f: &mut dyn FnMut(&crate::ast::Expr) -> bool) {
    use crate::ast::SelectTarget;
    for target in &select.targets {
        if let SelectTarget::Expr(e, _) = target {
            walk_expr(e, f);
        }
    }
    if let Some(ref e) = select.where_clause {
        walk_expr(e, f);
    }
    if let Some(ref e) = select.having {
        walk_expr(e, f);
    }
    for item in &select.order_by {
        walk_expr(&item.expr, f);
    }
    for gb in &select.group_by {
        if let crate::ast::GroupByItem::Expr(e) = gb {
            walk_expr(e, f);
        }
    }
}

/// Get source location from a `StatementInfo`.
pub fn stmt_location(info: &StatementInfo) -> SourceLocation {
    SourceLocation { line: info.start_line, column: info.start_col, offset: 0 }
}

/// Collect ALL `SelectStatement` nodes reachable from the given statements,
/// including those nested inside:
///
/// - Subqueries in expressions (`WHERE col IN (SELECT ...)`, `EXISTS (...)` etc.)
/// - FROM-clause subqueries (`FROM (SELECT ...) AS t`)
/// - CTE bodies (`WITH cte AS (SELECT ...)`)
/// - Set operation branches (`UNION`, `INTERSECT`, `EXCEPT`)
/// - PL/pgSQL blocks (FOR loops, cursor declarations, OPEN cursor, SqlStatement)
///
/// Returns `(&SelectStatement, SourceLocation)` pairs for every discovered SELECT.
/// The location is the best available span from the enclosing context.
pub fn collect_nested_selects(stmts: &[StatementInfo]) -> Vec<(&SelectStatement, SourceLocation)> {
    let mut results = Vec::new();
    for info in stmts {
        collect_selects_from_stmt(&info.statement, stmt_location(info), &mut results);
    }
    results
}

pub(crate) fn collect_selects_from_stmt<'a>(
    stmt: &'a Statement,
    loc: SourceLocation,
    out: &mut Vec<(&'a SelectStatement, SourceLocation)>,
) {
    use crate::ast::InsertSource;
    match stmt {
        Statement::Select(s) => {
            out.push((s, loc));
            collect_nested_in_select(s, out);
        }
        Statement::Insert(ins) => {
            if let InsertSource::Select(s) = &ins.node.source {
                out.push((s, loc));
                collect_nested_in_select(s, out);
            }
        }
        Statement::Update(upd) => {
            for a in &upd.assignments {
                collect_selects_from_expr(&a.value, out);
            }
            if let Some(ref w) = upd.where_clause {
                collect_selects_from_expr(w, out);
            }
        }
        Statement::Delete(del) => {
            if let Some(ref w) = del.where_clause {
                collect_selects_from_expr(w, out);
            }
        }
        Statement::Merge(m) => {
            collect_selects_from_from(std::slice::from_ref(&m.source), out);
            collect_selects_from_expr(&m.on_condition, out);
        }
        Statement::AnonyBlock(b) => collect_selects_from_pl_block(&b.block, out),
        Statement::Do(d) => {
            if let Some(ref block) = d.block {
                collect_selects_from_pl_block(block, out);
            }
        }
        Statement::CreateTableAs(cta) => {
            out.push((&cta.query, loc));
            collect_nested_in_select(&cta.query, out);
        }
        Statement::CreateView(cv) => {
            out.push((&cv.query, loc));
            collect_nested_in_select(&cv.query, out);
        }
        Statement::CreateMaterializedView(cmv) => {
            out.push((&cmv.query, loc));
            collect_nested_in_select(&cmv.query, out);
        }
        _ => {}
    }
}

/// Walk into subqueries nested INSIDE a SELECT: its expressions, FROM, CTEs, set ops.
fn collect_nested_in_select<'a>(s: &'a SelectStatement, out: &mut Vec<(&'a SelectStatement, SourceLocation)>) {
    for t in &s.targets {
        if let crate::ast::SelectTarget::Expr(e, _) = t {
            collect_selects_from_expr(e, out);
        }
    }
    if let Some(ref w) = s.where_clause {
        collect_selects_from_expr(w, out);
    }
    if let Some(ref h) = s.having {
        collect_selects_from_expr(h, out);
    }
    for item in &s.order_by {
        collect_selects_from_expr(&item.expr, out);
    }
    for gb in &s.group_by {
        if let crate::ast::GroupByItem::Expr(e) = gb {
            collect_selects_from_expr(e, out);
        }
    }
    collect_selects_from_from(&s.from, out);
    if let Some(ref w) = s.with {
        for cte in &w.ctes {
            out.push((&cte.query, SourceLocation::default()));
            collect_nested_in_select(&cte.query, out);
        }
    }
    if let Some(ref so) = s.set_operation {
        use crate::ast::SetOperation;
        let right = match so {
            SetOperation::Union { right, .. }
            | SetOperation::Intersect { right, .. }
            | SetOperation::Except { right, .. } => right,
        };
        out.push((right, SourceLocation::default()));
        collect_nested_in_select(right, out);
    }
}

/// Like [`collect_nested_in_select`] but does NOT recurse into set-operation
/// branches (UNION/INTERSECT/EXCEPT), which are at the same outer level as the
/// primary SELECT.  Only true *inner* (nested) SELECTs are collected:
/// expression subqueries, FROM-clause subqueries, and CTE bodies.
pub(crate) fn collect_inner_selects_only<'a>(
    s: &'a SelectStatement,
    out: &mut Vec<(&'a SelectStatement, SourceLocation)>,
) {
    for t in &s.targets {
        if let crate::ast::SelectTarget::Expr(e, _) = t {
            collect_selects_from_expr(e, out);
        }
    }
    if let Some(ref w) = s.where_clause {
        collect_selects_from_expr(w, out);
    }
    if let Some(ref h) = s.having {
        collect_selects_from_expr(h, out);
    }
    for item in &s.order_by {
        collect_selects_from_expr(&item.expr, out);
    }
    for gb in &s.group_by {
        if let crate::ast::GroupByItem::Expr(e) = gb {
            collect_selects_from_expr(e, out);
        }
    }
    collect_selects_from_from(&s.from, out);
    if let Some(ref w) = s.with {
        for cte in &w.ctes {
            out.push((&cte.query, SourceLocation::default()));
            collect_inner_selects_only(&cte.query, out);
        }
    }
    // Intentionally do NOT walk set_operation branches — those are at the
    // same outer level.
}

/// Walk an expression tree and collect subqueries.
fn collect_selects_from_expr<'a>(expr: &'a crate::ast::Expr, out: &mut Vec<(&'a SelectStatement, SourceLocation)>) {
    use crate::ast::Expr;
    match expr {
        Expr::Subquery(s) | Expr::Exists(s) => {
            out.push((s, SourceLocation::default()));
            collect_nested_in_select(s, out);
        }
        Expr::InSubquery { expr, subquery, .. } => {
            out.push((subquery, SourceLocation::default()));
            collect_nested_in_select(subquery, out);
            collect_selects_from_expr(expr, out);
        }
        Expr::ScalarSublink { expr, subquery, .. } => {
            out.push((subquery, SourceLocation::default()));
            collect_nested_in_select(subquery, out);
            collect_selects_from_expr(expr, out);
        }
        Expr::AtTimeZone { expr, zone } => {
            collect_selects_from_expr(expr, out);
            collect_selects_from_expr(zone, out);
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_selects_from_expr(left, out);
            collect_selects_from_expr(right, out);
        }
        Expr::UnaryOp { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::FunctionCall { args, over, filter, within_group, .. } => {
            for a in args {
                collect_selects_from_expr(a, out);
            }
            if let Some(o) = over {
                for e in &o.partition_by {
                    collect_selects_from_expr(e, out);
                }
                for item in &o.order_by {
                    collect_selects_from_expr(&item.expr, out);
                }
            }
            if let Some(fe) = filter {
                collect_selects_from_expr(fe, out);
            }
            for item in within_group {
                collect_selects_from_expr(&item.expr, out);
            }
        }
        Expr::Case { operand, whens, else_expr } => {
            if let Some(o) = operand {
                collect_selects_from_expr(o, out);
            }
            for w in whens {
                collect_selects_from_expr(&w.condition, out);
                collect_selects_from_expr(&w.result, out);
            }
            if let Some(e) = else_expr {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::Between { expr, low, high, .. } => {
            collect_selects_from_expr(expr, out);
            collect_selects_from_expr(low, out);
            collect_selects_from_expr(high, out);
        }
        Expr::InList { expr, list, .. } => {
            collect_selects_from_expr(expr, out);
            for e in list {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::Like { expr, pattern, escape, .. } => {
            collect_selects_from_expr(expr, out);
            collect_selects_from_expr(pattern, out);
            if let Some(e) = escape {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::IsNull { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::IsBoolean { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::TypeCast { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::Treat { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::Array(elems) => {
            for e in elems {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::Subscript { object, lower, upper, .. } => {
            collect_selects_from_expr(object, out);
            if let Some(l) = lower {
                collect_selects_from_expr(l, out);
            }
            if let Some(u) = upper {
                collect_selects_from_expr(u, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_selects_from_expr(object, out),
        Expr::Parenthesized(e) => collect_selects_from_expr(e, out),
        Expr::Prior(e) => collect_selects_from_expr(e, out),
        Expr::RowConstructor(exprs) => {
            for e in exprs {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::SpecialFunction { args, .. } => {
            for a in args {
                collect_selects_from_expr(a, out);
            }
        }
        Expr::XmlConcat(exprs) => {
            for e in exprs {
                collect_selects_from_expr(e, out);
            }
        }
        Expr::XmlForest(items) => {
            for item in items {
                collect_selects_from_expr(&item.expr, out);
            }
        }
        Expr::XmlParse { expr, .. } => collect_selects_from_expr(expr, out),
        Expr::XmlPi { content, .. } => {
            if let Some(c) = content {
                collect_selects_from_expr(c, out);
            }
        }
        Expr::XmlRoot { expr, version, .. } => {
            collect_selects_from_expr(expr, out);
            if let Some(v) = version {
                collect_selects_from_expr(v, out);
            }
        }
        Expr::XmlSerialize { expr, .. } => collect_selects_from_expr(expr, out),
        // Terminal nodes
        Expr::Default
        | Expr::CurrentOf { .. }
        | Expr::PredictBy { .. }
        | Expr::SysDate
        | Expr::SequenceValue { .. }
        | Expr::CursorAttribute { .. }
        | Expr::PlVariable(_)
        | Expr::Literal(_)
        | Expr::ColumnRef(_)
        | Expr::ColumnRefOuterJoin(_)
        | Expr::QualifiedStar(_)
        | Expr::Parameter(_)
        | Expr::MyBatisParam(_)
        | Expr::MyBatisRawExpr(_)
        | Expr::JdbcParam
        | Expr::XmlElement { .. }
        | Expr::CollationFor { .. } => {}
    }
}

/// Walk FROM clause and collect subqueries.
fn collect_selects_from_from<'a>(
    from: &'a [crate::ast::TableRef],
    out: &mut Vec<(&'a SelectStatement, SourceLocation)>,
) {
    use crate::ast::TableRef;
    for t in from {
        match t {
            TableRef::Subquery { query, .. } => {
                out.push((query, SourceLocation::default()));
                collect_nested_in_select(query, out);
            }
            TableRef::Join { left, right, condition, .. } => {
                collect_selects_from_from(std::slice::from_ref(left), out);
                collect_selects_from_from(std::slice::from_ref(right), out);
                if let Some(cond) = condition {
                    collect_selects_from_expr(cond, out);
                }
            }
            _ => {}
        }
    }
}

/// Return all PL/pgSQL blocks reachable from a statement's PL body (incl. CREATE FUNCTION/PROCEDURE/PACKAGE BODY).
pub(crate) fn pl_blocks_from_stmt(stmt: &Statement) -> Vec<&crate::ast::plpgsql::PlBlock> {
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

/// Walk a PL/pgSQL block and collect embedded SELECTs.
pub(crate) fn collect_selects_from_pl_block<'a>(
    block: &'a crate::ast::plpgsql::PlBlock,
    out: &mut Vec<(&'a SelectStatement, SourceLocation)>,
) {
    use crate::ast::plpgsql::PlDeclaration;

    // Cursor declarations
    for decl in &block.declarations {
        if let PlDeclaration::Cursor(c) = decl {
            if let Some(ref parsed) = c.parsed_query {
                collect_selects_from_stmt(parsed, SourceLocation::default(), out);
            }
        }
    }

    // Body statements
    collect_selects_from_pl_stmts(&block.body, out);
}

fn collect_selects_from_pl_stmts<'a>(
    stmts: &'a [crate::ast::plpgsql::PlStatement],
    out: &mut Vec<(&'a SelectStatement, SourceLocation)>,
) {
    use crate::ast::plpgsql::{PlForKind, PlOpenKind, PlStatement};
    for s in stmts {
        match s {
            PlStatement::SqlStatement { statement, .. } => {
                collect_selects_from_stmt(statement, SourceLocation::default(), out);
            }
            PlStatement::For(f) => {
                if let PlForKind::Query { parsed_query: Some(ref q), .. } = f.kind {
                    collect_selects_from_stmt(q, SourceLocation::default(), out);
                }
            }
            PlStatement::Open(o) => {
                if let PlOpenKind::ForQuery { parsed_query: Some(ref q), .. } = o.kind {
                    collect_selects_from_stmt(q, SourceLocation::default(), out);
                }
            }
            PlStatement::Block(b) => {
                collect_selects_from_pl_stmts(&b.body, out);
            }
            PlStatement::If(i) => {
                collect_selects_from_pl_stmts(&i.then_stmts, out);
                for e in &i.elsifs {
                    collect_selects_from_pl_stmts(&e.stmts, out);
                }
                collect_selects_from_pl_stmts(&i.else_stmts, out);
            }
            PlStatement::Case(c) => {
                for w in &c.whens {
                    collect_selects_from_pl_stmts(&w.stmts, out);
                }
                collect_selects_from_pl_stmts(&c.else_stmts, out);
            }
            PlStatement::Loop(l) => {
                collect_selects_from_pl_stmts(&l.body, out);
            }
            PlStatement::While(w) => {
                collect_selects_from_pl_stmts(&w.body, out);
            }
            PlStatement::ForEach(fe) => {
                collect_selects_from_pl_stmts(&fe.body, out);
            }
            _ => {}
        }
    }
}
