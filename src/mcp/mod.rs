//! MCP (Model Context Protocol) server support.
//!
//! Exposes ogsql-parser capabilities as MCP tools via the `rmcp` crate.
//! Supports stdio transport for integration with Claude Desktop, Cursor, etc.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::schemars::JsonSchema;
use rmcp::tool;
use rmcp::tool_router;
use serde::Deserialize;

use crate::linter::{build_lint_summary, Confidence, LintConfig, SqlLinter, SqlWarning};
use crate::token_formatter::{CommaStyle, FormatConfig, KeywordCase, TokenFormatter};

// ── Parameter types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ParseParams {
    /// SQL text to parse (mutually exclusive with `file_path`; exactly one must be set)
    #[serde(default)]
    pub sql: String,
    /// Path to a SQL file to parse instead of inline `sql` (mutually exclusive with `sql`).
    /// Supported extensions: .sql, .pck, .fnc, .prc
    #[serde(default)]
    pub file_path: Option<String>,
    /// Whether to preserve comments in output
    #[serde(default)]
    pub preserve_comments: bool,
    /// Enable SQL anti-pattern linting
    #[serde(default)]
    pub lint: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TokenizeParams {
    /// SQL text to tokenize
    pub sql: String,
    /// Return aggregated statistics instead of the full token list
    #[serde(default)]
    pub summary: bool,
}

fn default_indent() -> usize {
    2
}
fn default_line_width() -> usize {
    120
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FormatParams {
    /// SQL text to format (mutually exclusive with `file_path`; exactly one must be set)
    #[serde(default)]
    pub sql: String,
    /// Path to a SQL file to format instead of inline `sql` (mutually exclusive with `sql`).
    /// Supported extensions: .sql, .pck, .fnc, .prc
    #[serde(default)]
    pub file_path: Option<String>,
    /// Number of spaces per indentation level
    #[serde(default = "default_indent")]
    pub indent: usize,
    /// Keyword casing: "preserve", "upper", or "lower"
    #[serde(default)]
    pub keyword_case: String,
    /// Comma positioning: "trailing" or "leading"
    #[serde(default)]
    pub comma_style: String,
    /// Maximum line width before wrapping
    #[serde(default = "default_line_width")]
    pub line_width: usize,
    /// Convert keywords to uppercase (legacy compat, overrides keyword_case when true)
    #[serde(default)]
    pub uppercase: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidateParams {
    /// SQL text to validate (mutually exclusive with `file_path`; exactly one must be set)
    #[serde(default)]
    pub sql: String,
    /// Path to a SQL file to validate instead of inline `sql` (mutually exclusive with `sql`).
    /// Supported extensions: .sql, .pck, .fnc, .prc
    #[serde(default)]
    pub file_path: Option<String>,
    /// Enable SQL anti-pattern linting
    #[serde(default)]
    pub lint: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct Json2SqlParams {
    /// JSON string (output from parse tool) containing statements
    pub json: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ParseXmlParams {
    /// XML content of an iBatis/MyBatis mapper file
    pub xml: String,
    #[cfg(feature = "java")]
    /// Directory path containing Java source files for parameter type inference
    #[serde(default)]
    pub java_src: Option<String>,
    #[cfg(feature = "java")]
    /// Inline Java source map: {relative_path: source_code} for parameter type inference
    #[serde(default)]
    pub java_sources: Option<std::collections::HashMap<String, String>>,
    /// Enable SQL anti-pattern linting on extracted SQL statements
    #[serde(default)]
    pub lint: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ParseJavaParams {
    /// Java source file content
    pub source: String,
    /// Extra method names to treat as SQL-bearing (e.g. ["executeQuery", "nativeQuery"])
    #[serde(default)]
    pub extra_sql_methods: Vec<String>,
    /// Extra variable name patterns for SQL detection (e.g. ["QUERY", "STMT"])
    #[serde(default)]
    pub extra_sql_var_patterns: Vec<String>,
    /// Enable SQL anti-pattern linting on extracted SQL statements
    #[serde(default)]
    pub lint: bool,
}

const ALLOWED_SQL_EXTENSIONS: [&str; 4] = ["sql", "pck", "fnc", "prc"];

fn read_sql_file(path: &str) -> Result<String, String> {
    let p = std::path::Path::new(path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !ALLOWED_SQL_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "Unsupported file extension for file_path '{}': expected one of .sql, .pck, .fnc, .prc",
            path
        ));
    }
    std::fs::read_to_string(p).map_err(|e| format!("Failed to read file_path '{}': {}", path, e))
}

/// Resolve the effective SQL text from mutually exclusive `sql` / `file_path` params.
fn resolve_sql_input(sql: &str, file_path: &Option<String>) -> Result<String, String> {
    match file_path {
        Some(_) if !sql.is_empty() => Err("Provide either `sql` or `file_path`, not both".to_string()),
        Some(path) => read_sql_file(path),
        None if sql.is_empty() => Err("Must provide either `sql` or `file_path`".to_string()),
        None => Ok(sql.to_string()),
    }
}

// ── Server struct ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OgsqlServer;

// ── Tool implementations ─────────────────────────────────────────────────────

#[tool_router(server_handler)]
impl OgsqlServer {
    #[tool(description = "Parse SQL into structured AST JSON with error reports and query fingerprints")]
    fn parse(
        &self,
        Parameters(ParseParams { sql, file_path, preserve_comments, lint }): Parameters<ParseParams>,
    ) -> String {
        let sql = match resolve_sql_input(&sql, &file_path) {
            Ok(s) => s,
            Err(e) => return format!("{{\"error\": \"{}\"}}", e),
        };
        let options = crate::ParseOptions { preserve_comments, mybatis_params: false };
        let output = crate::Parser::parse_sql_with_options(&sql, options);

        let all_stmts: Vec<_> = output.statements.iter().map(|si| si.statement.clone()).collect();
        let fingerprints = crate::compute_query_fingerprints(&all_stmts);

        let stmt_values: Vec<serde_json::Value> = output
            .statements
            .iter()
            .map(|si| {
                let mut obj = serde_json::to_value(si).expect("StatementInfo is serializable");
                if let Some(analysis) = compute_routine_analysis(&si.statement) {
                    obj.as_object_mut()
                        .expect("to_value of struct yields object")
                        .insert("routine_analysis".to_string(), analysis);
                }
                obj
            })
            .collect();

        let mut out = serde_json::json!({
            "statements": stmt_values,
            "errors": output.errors,
        });
        if !fingerprints.is_empty() {
            out.as_object_mut()
                .expect("json! macro yields object")
                .insert("query_fingerprints".to_string(), serde_json::json!(fingerprints));
        }
        if !output.comments.is_empty() {
            out.as_object_mut()
                .expect("json! macro yields object")
                .insert("comments".to_string(), serde_json::json!(output.comments));
        }
        if lint {
            let config = LintConfig::default();
            let linter = SqlLinter::with_default_rules(config);
            let lint_warnings = linter.lint(&output.statements, None, Confidence::Full);
            out.as_object_mut()
                .expect("json! macro yields object")
                .insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            out.as_object_mut()
                .expect("json! macro yields object")
                .insert("lint_summary".to_string(), build_lint_summary(&lint_warnings));
        }
        serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(
        description = "Tokenize SQL into a list of typed tokens with line/column positions, or (summary=true) aggregated statistics"
    )]
    fn tokenize(&self, Parameters(TokenizeParams { sql, summary }): Parameters<TokenizeParams>) -> String {
        match crate::Tokenizer::new(&sql).tokenize() {
            Ok(tokens) => {
                let out = if summary {
                    build_token_summary(&tokens)
                } else {
                    let list: Vec<serde_json::Value> = tokens
                        .iter()
                        .map(|t| {
                            let (token_type, value) = token_display(t);
                            serde_json::json!({
                                "type": token_type,
                                "value": value,
                                "line": t.location.line,
                                "column": t.location.column,
                            })
                        })
                        .collect();
                    serde_json::json!({"tokens": list})
                };
                serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
            }
            Err(e) => format!("{{\"error\": \"{}\"}}", e),
        }
    }

    #[tool(description = "Format SQL with standardized keyword casing and indentation")]
    fn format(
        &self,
        Parameters(FormatParams { sql, file_path, indent, keyword_case, comma_style, line_width, uppercase }): Parameters<
            FormatParams,
        >,
    ) -> String {
        let sql = match resolve_sql_input(&sql, &file_path) {
            Ok(s) => s,
            Err(e) => return format!("{{\"error\": \"{}\"}}", e),
        };
        let tokens = match crate::Tokenizer::new(&sql).preserve_comments(true).tokenize() {
            Ok(t) => t,
            Err(e) => return format!("{{\"error\": \"{}\"}}", e),
        };

        let keyword_case = match keyword_case.to_lowercase().as_str() {
            "upper" => KeywordCase::Upper,
            "lower" => KeywordCase::Lower,
            _ => KeywordCase::Preserve,
        };

        let comma_style = match comma_style.to_lowercase().as_str() {
            "leading" => CommaStyle::Leading,
            _ => CommaStyle::Trailing,
        };

        let config = FormatConfig {
            indent_width: indent,
            keyword_case,
            comma_style,
            line_width,
            uppercase_keywords: uppercase,
            ..Default::default()
        };

        let formatter = TokenFormatter::with_config(&sql, tokens, config);
        let formatted = formatter.format();

        serde_json::to_string_pretty(&serde_json::json!({
            "formatted": formatted,
            "error_count": 0usize,
            "errors": Vec::<crate::ParserError>::new(),
        }))
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(description = "Validate SQL syntax and report errors and warnings")]
    fn validate(&self, Parameters(ValidateParams { sql, file_path, lint }): Parameters<ValidateParams>) -> String {
        let sql = match resolve_sql_input(&sql, &file_path) {
            Ok(s) => s,
            Err(e) => return format!("{{\"error\": \"{}\"}}", e),
        };
        let output = crate::Parser::parse_sql_with_options(
            &sql,
            crate::ParseOptions { preserve_comments: false, mybatis_params: false },
        );
        let pkg_errors = crate::validate_package_consistency(&output.statements);
        let mut errors = output.errors;
        if !pkg_errors.is_empty() {
            for pe in &pkg_errors {
                let msg = match &pe.detail {
                    Some(d) => format!("package {}: {} — {}", pe.package_name, pe.subprogram_name, d),
                    None => format!("package {}: {} — {:?}", pe.package_name, pe.subprogram_name, pe.kind),
                };
                errors.push(crate::ParserError::Warning {
                    message: msg,
                    location: crate::SourceLocation::default(),
                    level: crate::linter::WarningLevel::Caution,
                });
            }
        }
        let merge_errors = crate::validate_merge_semantics(&output.statements);
        if !merge_errors.is_empty() {
            for me in &merge_errors {
                errors.push(crate::ParserError::UnsupportedSyntax {
                    location: me.location,
                    syntax: "MERGE".to_string(),
                    hint: merge_error_detail(me),
                });
            }
        }
        let has_real_errors = errors.iter().any(|e| !is_warning(e));
        let mut result = serde_json::json!({
            "valid": !has_real_errors,
            "error_count": errors.iter().filter(|e| !is_warning(e)).count(),
            "warning_count": errors.iter().filter(|e| is_warning(e)).count(),
            "errors": errors,
        });
        if !pkg_errors.is_empty() {
            result
                .as_object_mut()
                .expect("json! macro yields object")
                .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
        }
        if !merge_errors.is_empty() {
            result
                .as_object_mut()
                .expect("json! macro yields object")
                .insert("merge_semantic_errors".to_string(), serde_json::json!(merge_errors));
        }
        if lint {
            let config = LintConfig::default();
            let linter = SqlLinter::with_default_rules(config);
            let lint_warnings = linter.lint(&output.statements, None, Confidence::Full);
            result
                .as_object_mut()
                .expect("json! macro yields object")
                .insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            result
                .as_object_mut()
                .expect("json! macro yields object")
                .insert("lint_summary".to_string(), build_lint_summary(&lint_warnings));
        }
        serde_json::to_string_pretty(&result).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(description = "Convert JSON AST (from parse tool output) back to SQL text")]
    fn json2sql(&self, Parameters(Json2SqlParams { json }): Parameters<Json2SqlParams>) -> String {
        let json_value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => return format!("{{\"error\": \"Invalid JSON: {}\"}}", e),
        };

        let statements: Vec<crate::Statement> = if let Some(arr) = json_value.get("statements") {
            match serde_json::from_value(arr.clone()) {
                Ok(s) => s,
                Err(e) => return format!("{{\"error\": \"Failed to deserialize statements: {}\"}}", e),
            }
        } else {
            match serde_json::from_value(json_value) {
                Ok(s) => s,
                Err(e) => return format!("{{\"error\": \"Failed to deserialize: {}\"}}", e),
            }
        };

        let formatter = crate::SqlFormatter::new();
        let formatted: Vec<String> = statements.iter().map(|s| formatter.format_statement(s)).collect();

        serde_json::to_string_pretty(&serde_json::json!({
            "statements": formatted,
            "count": formatted.len(),
        }))
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(
        description = "Parse iBatis/MyBatis XML mapper content and extract SQL statements. Optional java_src (directory path) or java_sources (inline {path: content} map) enables parameter type inference from Java source."
    )]
    fn parse_xml(
        &self,
        #[cfg(feature = "java")] Parameters(ParseXmlParams { xml, java_src, java_sources, lint }): Parameters<
            ParseXmlParams,
        >,
        #[cfg(not(feature = "java"))] Parameters(ParseXmlParams { xml, lint }): Parameters<ParseXmlParams>,
    ) -> String {
        #[cfg(feature = "java")]
        let (java_roots, tmp_dir) = {
            if let Some(ref sources) = java_sources {
                let tmp = std::env::temp_dir().join(format!("ogsql_mcp_{}", std::process::id()));
                for (path, content) in sources {
                    let full_path = tmp.join(path);
                    if let Some(parent) = full_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&full_path, content);
                }
                (vec![tmp.clone()], Some(tmp))
            } else if let Some(ref src) = java_src {
                let path = std::path::Path::new(src);
                if path.is_dir() {
                    (vec![path.to_path_buf()], None)
                } else {
                    let scan_dir = if path.is_dir() { path } else { std::path::Path::new(".") };
                    (crate::ibatis::detect_java_roots(scan_dir), None)
                }
            } else {
                let scan_dir = std::path::Path::new(".");
                (crate::ibatis::detect_java_roots(scan_dir), None)
            }
        };
        #[cfg(not(feature = "java"))]
        let java_roots: Vec<std::path::PathBuf> = vec![];

        #[cfg(feature = "java")]
        let result = crate::ibatis::parse_mapper_bytes_with_java_src(xml.as_bytes(), None, java_roots);
        #[cfg(not(feature = "java"))]
        let result = crate::ibatis::parse_mapper_bytes(xml.as_bytes());

        #[cfg(feature = "java")]
        if let Some(tmp) = tmp_dir {
            let _ = std::fs::remove_dir_all(&tmp);
        }

        let mut out = serde_json::to_value(&result).expect("ParsedMapper is serializable");
        if lint {
            attach_ibatis_lint(&mut out, &result.statements);
        }

        serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }

    #[tool(description = "Extract embedded SQL from Java source files (string literals, annotations, method calls)")]
    fn parse_java(
        &self,
        Parameters(ParseJavaParams { source, extra_sql_methods, extra_sql_var_patterns, lint }): Parameters<
            ParseJavaParams,
        >,
    ) -> String {
        let config = crate::java::JavaExtractConfig { extra_sql_methods, extra_sql_var_patterns };
        let result = crate::java::extract_sql_from_java(&source, "<mcp-input>", &config);

        let mut out = serde_json::to_value(&result).expect("JavaExtractResult is serializable");
        if lint {
            attach_java_lint(&mut out, &result.extractions);
        }

        serde_json::to_string_pretty(&out).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn attach_ibatis_lint(out: &mut serde_json::Value, statements: &[crate::ibatis::ParsedStatement]) {
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let mut all_warnings: Vec<SqlWarning> = Vec::new();

    if let Some(stmt_values) = out.get_mut("statements").and_then(|v| v.as_array_mut()) {
        for (stmt_value, orig) in stmt_values.iter_mut().zip(statements.iter()) {
            let Some((ref stmts, ref _errors)) = orig.parse_result else { continue };
            let warnings = linter.lint(stmts, None, Confidence::Full);
            // Keep the original parse_result tuple shape ([statements, errors])
            // and add lint_warnings as a separate field alongside it,
            // matching the pattern used by attach_java_lint.
            if let Some(stmt_obj) = stmt_value.as_object_mut() {
                stmt_obj.insert("lint_warnings".to_string(), serde_json::json!(warnings));
            }
            all_warnings.extend(warnings);
        }
    }

    if let Some(obj) = out.as_object_mut() {
        obj.insert("lint_summary".to_string(), build_lint_summary(&all_warnings));
    }
}

fn attach_java_lint(out: &mut serde_json::Value, extractions: &[crate::java::ExtractedSql]) {
    let linter = SqlLinter::with_default_rules(LintConfig::default());
    let mut all_warnings: Vec<SqlWarning> = Vec::new();

    if let Some(extraction_values) = out.get_mut("extractions").and_then(|v| v.as_array_mut()) {
        for (extraction_value, orig) in extraction_values.iter_mut().zip(extractions.iter()) {
            let Some(ref parse_result) = orig.parse_result else { continue };
            let warnings = linter.lint(&parse_result.statements, None, Confidence::Full);
            if let Some(pr_obj) = extraction_value.get_mut("parse_result").and_then(|v| v.as_object_mut()) {
                pr_obj.insert("lint_warnings".to_string(), serde_json::json!(warnings));
            }
            all_warnings.extend(warnings);
        }
    }

    if let Some(obj) = out.as_object_mut() {
        obj.insert("lint_summary".to_string(), build_lint_summary(&all_warnings));
    }
}

fn token_display(t: &crate::TokenWithSpan) -> (String, String) {
    use crate::Token;
    match &t.token {
        Token::Keyword(k) => ("Keyword".into(), format!("{:?}", k)),
        Token::Ident(s) => ("Ident".into(), s.clone()),
        Token::Integer(n) => ("Integer".into(), n.to_string()),
        Token::StringLiteral(s) => ("String".into(), s.clone()),
        Token::Float(s) => ("Float".into(), s.clone()),
        Token::Op(s) => ("Op".into(), s.clone()),
        Token::OpLe => ("Op".into(), "<=".into()),
        Token::OpNe => ("Op".into(), "<>".into()),
        Token::OpGe => ("Op".into(), ">=".into()),
        Token::OpShiftL => ("Op".into(), "<<".into()),
        Token::OpShiftR => ("Op".into(), ">>".into()),
        Token::OpArrow => ("Op".into(), "->".into()),
        Token::OpJsonArrow => ("Op".into(), "->>".into()),
        Token::OpNe2 => ("Op".into(), "!=".into()),
        Token::OpDblBang => ("Op".into(), "!!".into()),
        Token::OpConcat => ("Op".into(), "||".into()),
        Token::Comment(s) => ("Comment".into(), s.clone()),
        other => ("Other".into(), format!("{:?}", other)),
    }
}

fn build_token_summary(tokens: &[crate::TokenWithSpan]) -> serde_json::Value {
    use crate::{Keyword, Token};
    use std::collections::BTreeMap;

    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut tables: Vec<String> = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    let mut functions: Vec<String> = Vec::new();
    let mut has_subquery = false;
    let mut has_join = false;
    let mut prev_was_table_keyword = false;

    for (i, t) in tokens.iter().enumerate() {
        let (token_type, _) = token_display(t);
        *by_type.entry(token_type).or_insert(0) += 1;

        match &t.token {
            Token::Keyword(k) => {
                if matches!(k, Keyword::JOIN) {
                    has_join = true;
                }
                if matches!(k, Keyword::FROM | Keyword::JOIN) {
                    prev_was_table_keyword = true;
                    continue;
                }
                if matches!(tokens.get(i + 1).map(|next| &next.token), Some(Token::LParen)) {
                    let name = format!("{:?}", k).to_lowercase();
                    if !functions.contains(&name) {
                        functions.push(name);
                    }
                }
            }
            Token::Ident(s) => {
                if prev_was_table_keyword {
                    if !tables.contains(s) {
                        tables.push(s.clone());
                    }
                } else if !columns.contains(s) {
                    columns.push(s.clone());
                }
            }
            Token::LParen => {
                if matches!(tokens.get(i + 1).map(|next| &next.token), Some(Token::Keyword(Keyword::SELECT))) {
                    has_subquery = true;
                }
            }
            _ => {}
        }
        prev_was_table_keyword = false;
    }

    serde_json::json!({
        "total_tokens": tokens.len(),
        "by_type": by_type,
        "tables": tables,
        "columns": columns,
        "functions": functions,
        "has_subquery": has_subquery,
        "has_join": has_join,
    })
}

fn is_warning(e: &crate::ParserError) -> bool {
    crate::is_warning(e)
}

fn merge_error_detail(err: &crate::MergeSemanticError) -> String {
    match &err.detail {
        Some(d) => d.clone(),
        None => match err.kind {
            crate::MergeSemanticErrorKind::DeleteNotSupported => {
                "GaussDB does not support MERGE ... WHEN MATCHED THEN DELETE".to_string()
            }
            crate::MergeSemanticErrorKind::OnColumnUpdated => {
                "GaussDB does not allow updating columns referenced in the ON clause".to_string()
            }
        },
    }
}

fn compute_routine_analysis(stmt: &crate::Statement) -> Option<serde_json::Value> {
    use crate::ast::PackageItem;
    match stmt {
        crate::Statement::CreateProcedure(p) => {
            let block = p.block.as_ref()?;
            let analysis = crate::analyze_return_cursors(block, &p.parameters, &p.name.join("."), "Procedure", None);
            if analysis.return_cursors.is_empty() {
                return None;
            }
            Some(serde_json::json!(analysis))
        }
        crate::Statement::CreateFunction(f) => {
            let block = f.block.as_ref()?;
            let analysis = crate::analyze_return_cursors(
                block,
                &f.parameters,
                &f.name.join("."),
                "Function",
                f.return_type.as_deref(),
            );
            if analysis.return_cursors.is_empty() {
                return None;
            }
            Some(serde_json::json!(analysis))
        }
        crate::Statement::CreatePackageBody(pkg) => {
            let mut analyses = Vec::new();
            for item in &pkg.items {
                match item {
                    PackageItem::Procedure(p) => {
                        if let Some(ref block) = p.block {
                            let analysis = crate::analyze_return_cursors(
                                block,
                                &p.parameters,
                                &p.name.join("."),
                                "Procedure",
                                None,
                            );
                            if !analysis.return_cursors.is_empty() {
                                analyses.push(analysis);
                            }
                        }
                    }
                    PackageItem::Function(f) => {
                        if let Some(ref block) = f.block {
                            let analysis = crate::analyze_return_cursors(
                                block,
                                &f.parameters,
                                &f.name.join("."),
                                "Function",
                                f.return_type.as_deref(),
                            );
                            if !analysis.return_cursors.is_empty() {
                                analyses.push(analysis);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if analyses.is_empty() {
                None
            } else {
                Some(serde_json::json!(analyses))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
