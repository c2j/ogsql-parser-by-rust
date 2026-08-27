//! HTTP handler functions for the serve API.

use axum::body::Body;
use axum::extract::FromRequest;
use axum::extract::Request;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::error::ApiError;
use super::sarif;
use super::schema::*;

// ─── Health ──────────────────────────────────────────────────

/// Health check endpoint.
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "ogsql",
    responses((status = 200, description = "Service is healthy", body = HealthResponse))
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".to_string(), version: env!("CARGO_PKG_VERSION").to_string() })
}

// ─── Parse ───────────────────────────────────────────────────

/// Parse SQL into AST.
#[utoipa::path(
    post,
    path = "/api/parse",
    tag = "ogsql",
    request_body = ParseInput,
    responses(
        (status = 200, description = "Parsed AST result", body = ParseResponse),
        (status = 400, description = "Invalid request", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_parse(Json(input): Json<ParseInput>) -> Result<Json<ParseResponse>, ApiError> {
    let output = crate::parse_input(&input.sql, input.preserve_comments, input.mybatis);

    let output = match input.procedure {
        Some(ref proc) => match crate::filter_output_by_procedure(output, proc) {
            Ok(filtered) => filtered,
            Err(msg) => return Err(ApiError::NotFound(msg)),
        },
        None => output,
    };

    let schema_path = input.schema_json.clone();
    let stmt_values: Vec<serde_json::Value> = output
        .statements
        .iter()
        .map(|si| {
            let mut obj = serde_json::to_value(si).unwrap_or_default();
            if let Some(block) = crate::extract_pl_block(&si.statement) {
                let report = ogsql_parser::analyze_pl_block(block);
                if !report.execute_findings.is_empty() {
                    obj.as_object_mut()
                        .unwrap_or(&mut serde_json::Map::new())
                        .insert("dynamic_sql_analysis".to_string(), serde_json::json!(report));
                }
                let tx_report = ogsql_parser::analyze_transactions(block);
                if let Ok(tx_json) = serde_json::to_string_pretty(&tx_report) {
                    obj.as_object_mut()
                        .unwrap_or(&mut serde_json::Map::new())
                        .insert("transaction_analysis".to_string(), tx_json.into());
                }
                if let Some(ref schema_path) = schema_path {
                    match ogsql_parser::load_schema(schema_path) {
                        Ok(schema) => {
                            let schema_report = ogsql_parser::resolve_schema(block, &schema);
                            obj.as_object_mut()
                                .unwrap_or(&mut serde_json::Map::new())
                                .insert("schema_resolution".to_string(), serde_json::json!(schema_report));
                        }
                        Err(e) => {
                            obj.as_object_mut()
                                .unwrap_or(&mut serde_json::Map::new())
                                .insert("schema_resolution_error".to_string(), serde_json::json!(e.to_string()));
                        }
                    }
                }
            }
            if crate::has_routine_return_cursors(&si.statement) {
                if let Some(analysis) = crate::compute_routine_analysis(&si.statement) {
                    obj.as_object_mut()
                        .unwrap_or(&mut serde_json::Map::new())
                        .insert("routine_analysis".to_string(), analysis);
                }
            }
            obj
        })
        .collect();

    let all_stmts: Vec<_> = output.statements.iter().map(|si| si.statement.clone()).collect();
    let fingerprints = ogsql_parser::compute_query_fingerprints(&all_stmts);

    let errors: Vec<serde_json::Value> =
        output.errors.iter().map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null)).collect();

    let comments = if output.comments.is_empty() {
        None
    } else {
        Some(output.comments.iter().map(|c| serde_json::json!(c)).collect())
    };

    let mut response = ParseResponse {
        statements: stmt_values,
        errors,
        query_fingerprints: if fingerprints.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&fingerprints).unwrap_or_default())
        },
        comments,
        lint_warnings: None,
        lint_summary: None,
        extracted_sql: None,
    };

    if input.lint.unwrap_or(false) {
        let config = input.lint_config.as_ref().map(|c| c.to_lint_config()).unwrap_or_default();
        let lint_warnings = crate::run_lint(&output.statements, ogsql_parser::linter::Confidence::Full, &config, None);
        if !lint_warnings.is_empty() {
            response.lint_summary = Some(crate::format_warnings_summary(&lint_warnings));
            response.lint_warnings =
                Some(lint_warnings.iter().map(|w| serde_json::to_value(w).unwrap_or_default()).collect());
        }
    }

    if input.extract_sql {
        let mut extracted_rows: Vec<serde_json::Value> = Vec::new();
        for si in &output.statements {
            if let Some(block) = crate::extract_pl_block(&si.statement) {
                let vars = std::collections::HashMap::new();
                let out_cursors = std::collections::HashSet::new();
                let rows = crate::collect_block_sql_rows(block, "", si.start_line, &vars, &out_cursors, false);
                for row in rows {
                    extracted_rows.push(serde_json::json!({
                        "line": row.line,
                        "type": row.stmt_type,
                        "name": row.name,
                        "parent": row.parent,
                        "sql": row.sql,
                        "branch_path": row.branch_path,
                        "branch_condition": row.branch_condition,
                    }));
                }
            }
        }
        if !extracted_rows.is_empty() {
            response.extracted_sql = Some(extracted_rows);
        }
    }

    Ok(Json(response))
}

// ─── Format ──────────────────────────────────────────────────

/// Format SQL statements.
#[utoipa::path(
    post,
    path = "/api/format",
    tag = "ogsql",
    request_body = FormatInput,
    responses(
        (status = 200, description = "Formatted SQL result", body = FormatResponse),
        (status = 422, description = "Tokenization error", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_format(Json(input): Json<FormatInput>) -> Result<Json<FormatResponse>, ApiError> {
    let mut tokenizer = ogsql_parser::Tokenizer::new(&input.sql).preserve_comments(true);
    if input.mybatis {
        tokenizer = tokenizer.mybatis_params(true);
    }
    let tokens = tokenizer.tokenize().map_err(|e| ApiError::UnprocessableEntity(e.to_string()))?;

    let mut config = ogsql_parser::FormatConfig::default();
    if let Some(indent) = input.indent {
        config.indent_width = indent;
    }
    if let Some(ref kw) = input.keyword_case {
        config.keyword_case = match kw.as_str() {
            "upper" => ogsql_parser::token_formatter::KeywordCase::Upper,
            "lower" => ogsql_parser::token_formatter::KeywordCase::Lower,
            _ => ogsql_parser::token_formatter::KeywordCase::Preserve,
        };
    }
    if let Some(ref comma) = input.comma_style {
        config.comma_style = match comma.as_str() {
            "leading" => ogsql_parser::token_formatter::CommaStyle::Leading,
            _ => ogsql_parser::token_formatter::CommaStyle::Trailing,
        };
    }
    if let Some(line_width) = input.line_width {
        config.line_width = line_width;
    }
    if input.uppercase.unwrap_or(false) {
        config.uppercase_keywords = true;
    }
    if input.no_select_newline.unwrap_or(false) {
        config.select_newline = false;
    }
    if input.no_logical_newline.unwrap_or(false) {
        config.logical_operator_newline = false;
    }
    if input.no_semicolon_newline.unwrap_or(false) {
        config.semicolon_newline = false;
    }

    let formatted = ogsql_parser::token_formatter::TokenFormatter::with_config(&input.sql, tokens, config).format();
    Ok(Json(FormatResponse { formatted }))
}

// ─── Tokenize ────────────────────────────────────────────────

/// Tokenize SQL into tokens.
#[utoipa::path(
    post,
    path = "/api/tokenize",
    tag = "ogsql",
    request_body = TokenizeInput,
    responses(
        (status = 200, description = "Token list", body = TokenizeResponse),
        (status = 422, description = "Tokenization error", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_tokenize(Json(input): Json<TokenizeInput>) -> Result<Json<TokenizeResponse>, ApiError> {
    let mut tokenizer = ogsql_parser::Tokenizer::new(&input.sql);
    if input.preserve_comments {
        tokenizer = tokenizer.preserve_comments(true);
    }
    if input.mybatis {
        tokenizer = tokenizer.mybatis_params(true);
    }
    let tokens = tokenizer.tokenize().map_err(|e| ApiError::UnprocessableEntity(e.to_string()))?;

    let token_infos: Vec<TokenInfo> = tokens
        .iter()
        .map(|t| {
            let (token_type, value) = crate::token_display(t);
            TokenInfo { token_type, value, line: t.location.line, column: t.location.column }
        })
        .collect();

    Ok(Json(TokenizeResponse { tokens: token_infos }))
}

// ─── Validate ────────────────────────────────────────────────

/// Validate SQL syntax — accepts JSON body or multipart file upload.
///
/// Supports optional SARIF 2.1.0 output: set `"format": "sarif"` in request body
/// to receive a `application/sarif+json` response usable with GitHub Code Scanning.
#[utoipa::path(
    post,
    path = "/api/validate",
    tag = "ogsql",
    request_body = ValidateInput,
    responses(
        (status = 200, description = "Validation result (JSON)",
         body = ValidateResponse, content_type = "application/json"),
        (status = 200, description = "Validation result (SARIF 2.1.0)",
         content_type = "application/sarif+json"),
        (status = 400, description = "Invalid request", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_validate(req: Request) -> Result<Response, ApiError> {
    let ct = req.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/json");

    let input = if ct.starts_with("multipart/form-data") {
        parse_validate_multipart(req).await?
    } else {
        let Json(input) = Json::<ValidateInput>::from_request(req, &())
            .await
            .map_err(|_| ApiError::BadRequest("invalid JSON body".into()))?;
        input
    };

    do_validate(input)
}

fn do_validate(input: ValidateInput) -> Result<Response, ApiError> {
    let response = build_validate_response(&input)?;
    maybe_sarif_response(response, &input.sql, input.format.as_deref())
}

fn build_validate_response(input: &ValidateInput) -> Result<ValidateResponse, ApiError> {
    let output = crate::parse_input(&input.sql, false, input.mybatis);

    let pkg_errors = ogsql_parser::validate_package_consistency(&output.statements);
    let has_pkg_issues = !pkg_errors.is_empty();
    let mut errors = output.errors;
    if has_pkg_issues {
        for pe in &pkg_errors {
            let msg = match &pe.detail {
                Some(d) => format!("package {}: {} — {}", pe.package_name, pe.subprogram_name, d),
                None => format!("package {}: {} — {:?}", pe.package_name, pe.subprogram_name, pe.kind),
            };
            errors.push(ogsql_parser::ParserError::Warning {
                message: msg,
                location: ogsql_parser::SourceLocation::default(),
                level: ogsql_parser::linter::WarningLevel::Caution,
            });
        }
    }

    let merge_errors = ogsql_parser::validate_merge_semantics(&output.statements);
    if !merge_errors.is_empty() {
        for me in &merge_errors {
            errors.push(ogsql_parser::ParserError::UnsupportedSyntax {
                location: me.location,
                syntax: "MERGE".to_string(),
                hint: crate::merge_error_detail(me),
            });
        }
    }

    let strict = input.strict.unwrap_or(false);
    let var_errors = crate::validate_pl_variables_from_stmts(&output.statements, &[], strict);
    let has_var_errors = !var_errors.is_empty();
    let has_real_errors = errors.iter().any(|e| !crate::is_warning(e)) || has_var_errors;

    // Build per-statement validation entries with metadata.
    let formatter = ogsql_parser::SqlFormatter::new();
    let stmt_validations: Vec<StatementValidation> = output
        .statements
        .iter()
        .map(|si| {
            let sql = formatter.format_statement(&si.statement);
            let sql_type = serde_json::to_value(&si.statement)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::Object(map) => map.keys().next().cloned(),
                    _ => None,
                })
                .unwrap_or_else(|| "Unknown".to_string());
            let method = statement_method_name(&si.statement);

            StatementValidation {
                method,
                line: si.start_line,
                sql_type,
                sql,
                valid: true,
                error_count: 0,
                warning_count: 0,
                errors: vec![],
                warnings: vec![],
            }
        })
        .collect();

    let mut response = ValidateResponse {
        valid: !has_real_errors,
        error_count: errors.iter().filter(|e| !crate::is_warning(e)).count() + var_errors.len(),
        warning_count: errors.iter().filter(|e| crate::is_warning(e)).count(),
        errors: errors.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect(),
        undefined_variables: if has_var_errors {
            Some(var_errors.iter().map(|v| serde_json::to_value(v).unwrap_or_default()).collect())
        } else {
            None
        },
        package_consistency_errors: if has_pkg_issues {
            Some(serde_json::to_value(&pkg_errors).unwrap_or_default())
        } else {
            None
        },
        lint_warnings: None,
        lint_summary: None,
        strict_mode: if strict { Some(true) } else { None },
        statements: if stmt_validations.is_empty() { None } else { Some(stmt_validations) },
    };

    if input.lint.unwrap_or(false) {
        let config = input.lint_config.as_ref().map(|c| c.to_lint_config()).unwrap_or_default();
        let schema = input.schema_json.as_deref().and_then(|p| ogsql_parser::load_full_schema(p).ok());
        let lint_warnings =
            crate::run_lint(&output.statements, ogsql_parser::linter::Confidence::Full, &config, schema.as_ref());
        if !lint_warnings.is_empty() {
            response.lint_summary = Some(ogsql_parser::linter::build_lint_summary(&lint_warnings));
            response.lint_warnings =
                Some(lint_warnings.iter().map(|w| serde_json::to_value(w).unwrap_or_default()).collect());
        }
    }

    Ok(response)
}

// ─── json2sql ────────────────────────────────────────────────

/// Convert JSON (from /api/parse) back to SQL.
#[utoipa::path(
    post,
    path = "/api/json2sql",
    tag = "ogsql",
    request_body = JsonInput,
    responses(
        (status = 200, description = "Reconstructed SQL", body = Json2SqlResponse),
        (status = 400, description = "Invalid JSON input", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_json2sql(Json(input): Json<JsonInput>) -> Result<Json<Json2SqlResponse>, ApiError> {
    let json_value: serde_json::Value =
        serde_json::from_str(&input.json).map_err(|e| ApiError::BadRequest(format!("Invalid JSON: {}", e)))?;

    let statements: Vec<ogsql_parser::Statement> = if let Some(arr) = json_value.get("statements") {
        serde_json::from_value(arr.clone())
            .map_err(|e| ApiError::BadRequest(format!("Failed to deserialize statements: {}", e)))?
    } else {
        serde_json::from_value(json_value).map_err(|e| ApiError::BadRequest(format!("Failed to deserialize: {}", e)))?
    };

    let formatter = ogsql_parser::SqlFormatter::new();
    let formatted: Vec<String> = statements.iter().map(|s| formatter.format_statement(s)).collect();
    let count = formatted.len();

    Ok(Json(Json2SqlResponse { statements: formatted, count }))
}

// ─── parse-xml (feature = "ibatis") ──────────────────────────

#[cfg(feature = "ibatis")]
/// Parse iBatis/MyBatis XML mapper content.
#[utoipa::path(
    post,
    path = "/api/parse-xml",
    tag = "ogsql",
    request_body = ParseXmlInput,
    responses((status = 200, description = "Parsed iBatis XML result"))
)]
pub async fn handle_parse_xml(Json(input): Json<ParseXmlInput>) -> Result<Json<serde_json::Value>, ApiError> {
    #[cfg(feature = "java")]
    let java_roots: Vec<std::path::PathBuf> =
        input.java_src.as_deref().map(|p| vec![std::path::PathBuf::from(p)]).unwrap_or_default();
    #[cfg(not(feature = "java"))]
    let _java_roots: Vec<std::path::PathBuf> = vec![];

    let result = if input.structured.unwrap_or(false) {
        let parsed = ogsql_parser::ibatis::parse_mapper_bytes_structured(input.xml.as_bytes());
        serde_json::to_value(&parsed).map_err(|_| ApiError::Internal("serialization failed".to_string()))?
    } else {
        #[cfg(feature = "java")]
        {
            let r = ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(input.xml.as_bytes(), None, java_roots);
            serde_json::to_value(&r).map_err(|_| ApiError::Internal("serialization failed".to_string()))?
        }
        #[cfg(not(feature = "java"))]
        {
            let r = ogsql_parser::ibatis::parse_mapper_bytes(input.xml.as_bytes());
            serde_json::to_value(&r).map_err(|_| ApiError::Internal("serialization failed".to_string()))?
        }
    };

    Ok(Json(result))
}

// ─── parse-java (feature = "java") ───────────────────────────

#[cfg(feature = "java")]
/// Extract and parse SQL from Java source files.
#[utoipa::path(
    post,
    path = "/api/parse-java",
    tag = "ogsql",
    request_body = ParseJavaInput,
    responses((status = 200, description = "Extracted SQL from Java source"))
)]
pub async fn handle_parse_java(Json(input): Json<ParseJavaInput>) -> Result<Json<serde_json::Value>, ApiError> {
    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: input.extra_sql_methods.unwrap_or_default(),
        extra_sql_var_patterns: input.extra_sql_var_patterns.unwrap_or_default(),
    };
    let result = ogsql_parser::java::extract_sql_from_java(&input.source, "<api-input>", &config);
    let value = serde_json::to_value(&result).map_err(|_| ApiError::Internal("serialization failed".to_string()))?;
    Ok(Json(value))
}

// ─── validate-xml (feature = "ibatis") ──────────────────────

#[cfg(feature = "ibatis")]
/// Validate iBatis/MyBatis XML mapper — accepts JSON body or multipart file upload.
///
/// Supports optional SARIF 2.1.0 output: set `"format": "sarif"` in request body.
#[utoipa::path(
    post,
    path = "/api/validate-xml",
    tag = "ogsql",
    request_body = ValidateXmlInput,
    responses(
        (status = 200, description = "Validation result (JSON)",
         body = ValidateResponse, content_type = "application/json"),
        (status = 200, description = "Validation result (SARIF 2.1.0)",
         content_type = "application/sarif+json"),
        (status = 400, description = "Invalid request", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_validate_xml(req: Request) -> Result<Response, ApiError> {
    let ct = req.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/json");

    let input = if ct.starts_with("multipart/form-data") {
        parse_validate_xml_multipart(req).await?
    } else {
        let Json(input) = Json::<ValidateXmlInput>::from_request(req, &())
            .await
            .map_err(|_| ApiError::BadRequest("invalid JSON body".into()))?;
        input
    };

    do_validate_xml(input)
}

#[cfg(feature = "ibatis")]
fn do_validate_xml(input: ValidateXmlInput) -> Result<Response, ApiError> {
    let response = build_xml_validate_response(&input)?;
    maybe_sarif_response(response, &input.xml, input.format.as_deref())
}

#[cfg(feature = "ibatis")]
fn build_xml_validate_response(input: &ValidateXmlInput) -> Result<ValidateResponse, ApiError> {
    let xml_bytes = input.xml.as_bytes();

    #[cfg(feature = "java")]
    let java_roots: Vec<std::path::PathBuf> =
        input.java_src.as_deref().map(|p| vec![std::path::PathBuf::from(p)]).unwrap_or_default();
    #[cfg(not(feature = "java"))]
    let _java_roots: Vec<std::path::PathBuf> = vec![];

    #[cfg(feature = "java")]
    let result = if java_roots.is_empty() {
        ogsql_parser::ibatis::parse_mapper_bytes_with_path(xml_bytes, None)
    } else {
        ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(xml_bytes, None, java_roots)
    };
    #[cfg(not(feature = "java"))]
    let result = ogsql_parser::ibatis::parse_mapper_bytes_with_path(xml_bytes, None);

    build_xml_validation_response(&result, xml_bytes, input.strict, input.lint, &input.lint_config)
}

#[cfg(feature = "ibatis")]
fn build_xml_validation_response(
    result: &ogsql_parser::ibatis::types::ParsedMapper,
    xml_bytes: &[u8],
    strict: Option<bool>,
    lint_enabled: Option<bool>,
    lint_config_opt: &Option<LintConfigInput>,
) -> Result<ValidateResponse, ApiError> {
    let mut all_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
    let mut all_errors: Vec<ogsql_parser::ParserError> = Vec::new();
    let mut statement_validations: Vec<StatementValidation> = Vec::new();

    for stmt in &result.statements {
        let stmt_errors: Vec<ogsql_parser::ParserError> =
            stmt.parse_result.as_ref().map(|(_, errs)| errs.clone()).unwrap_or_default();
        let stmt_infos: Vec<ogsql_parser::StatementInfo> =
            stmt.parse_result.as_ref().map(|(infos, _)| infos.clone()).unwrap_or_default();

        all_stmts.extend(stmt_infos);
        all_errors.extend(stmt_errors.clone());

        let stmt_err: Vec<serde_json::Value> = stmt_errors
            .iter()
            .filter(|e| !crate::is_warning(e))
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect();
        let stmt_warn: Vec<serde_json::Value> = stmt_errors
            .iter()
            .filter(|e| crate::is_warning(e))
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect();

        statement_validations.push(StatementValidation {
            method: stmt.id.clone(),
            line: stmt.line,
            sql_type: format!("{:?}", stmt.kind),
            sql: stmt.flat_sql.trim().replace('\r', ""),
            valid: stmt_err.is_empty(),
            error_count: stmt_err.len(),
            warning_count: stmt_warn.len(),
            errors: stmt_err,
            warnings: stmt_warn,
        });
    }

    for e in &result.errors {
        all_errors.push(ogsql_parser::ParserError::UnexpectedToken {
            location: ogsql_parser::SourceLocation::default(),
            expected: "valid XML mapper".to_string(),
            got: format!("{}", e),
        });
    }

    let (core_errors, _pkg_errors, var_errors) = crate::validate_from_stmts(&all_stmts, &[], strict.unwrap_or(false));
    all_errors.extend(core_errors);

    let has_var_errors = !var_errors.is_empty();
    let has_real_errors = all_errors.iter().any(|e| !crate::is_warning(e)) || has_var_errors;

    let mut response = ValidateResponse {
        valid: !has_real_errors,
        error_count: all_errors.iter().filter(|e| !crate::is_warning(e)).count() + var_errors.len(),
        warning_count: all_errors.iter().filter(|e| crate::is_warning(e)).count(),
        errors: all_errors.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect(),
        undefined_variables: if has_var_errors {
            Some(var_errors.iter().map(|v| serde_json::to_value(v).unwrap_or_default()).collect())
        } else {
            None
        },
        package_consistency_errors: None,
        lint_warnings: None,
        lint_summary: None,
        strict_mode: if strict.unwrap_or(false) { Some(true) } else { None },
        statements: if statement_validations.is_empty() { None } else { Some(statement_validations) },
    };

    if lint_enabled.unwrap_or(false) {
        let config = lint_config_opt.as_ref().map(|c| c.to_lint_config()).unwrap_or_default();
        let mut lint_warnings: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();
        if !all_stmts.is_empty() {
            let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
            lint_warnings.extend(linter.lint(&all_stmts, None, ogsql_parser::linter::Confidence::Full));
        }
        let structured = ogsql_parser::ibatis::parse_mapper_bytes_structured(xml_bytes);
        let expand_ws = ogsql_parser::linter::structured::lint_structured_mapper(&structured, &config);
        lint_warnings.extend(expand_ws);
        if !lint_warnings.is_empty() {
            response.lint_summary = Some(ogsql_parser::linter::build_lint_summary(&lint_warnings));
            response.lint_warnings =
                Some(lint_warnings.iter().map(|w| serde_json::to_value(w).unwrap_or_default()).collect());
        }
    }

    Ok(response)
}

// ─── validate-java (feature = "java") ───────────────────────

#[cfg(feature = "java")]
/// Validate Java source — accepts JSON body or multipart file upload.
///
/// Supports optional SARIF 2.1.0 output: set `"format": "sarif"` in request body.
#[utoipa::path(
    post,
    path = "/api/validate-java",
    tag = "ogsql",
    request_body = ValidateJavaInput,
    responses(
        (status = 200, description = "Validation result (JSON)",
         body = ValidateResponse, content_type = "application/json"),
        (status = 200, description = "Validation result (SARIF 2.1.0)",
         content_type = "application/sarif+json"),
        (status = 400, description = "Invalid request", body = super::error::ApiErrorBody),
    )
)]
pub async fn handle_validate_java(req: Request) -> Result<Response, ApiError> {
    let ct = req.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("application/json");

    let input = if ct.starts_with("multipart/form-data") {
        parse_validate_java_multipart(req).await?
    } else {
        let Json(input) = Json::<ValidateJavaInput>::from_request(req, &())
            .await
            .map_err(|_| ApiError::BadRequest("invalid JSON body".into()))?;
        input
    };

    do_validate_java(input)
}

#[cfg(feature = "java")]
fn do_validate_java(input: ValidateJavaInput) -> Result<Response, ApiError> {
    let response = build_java_validate_response(&input)?;
    maybe_sarif_response(response, &input.source, input.format.as_deref())
}

#[cfg(feature = "java")]
fn build_java_validate_response(input: &ValidateJavaInput) -> Result<ValidateResponse, ApiError> {
    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: input.extra_sql_methods.clone().unwrap_or_default(),
        extra_sql_var_patterns: input.extra_sql_var_patterns.clone().unwrap_or_default(),
    };
    let result = ogsql_parser::java::extract_sql_from_java(&input.source, "<api-input>", &config);

    let mut all_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
    let mut all_errors: Vec<ogsql_parser::ParserError> = Vec::new();
    let mut has_parse_error = false;
    let mut extraction_validations: Vec<StatementValidation> = Vec::new();

    for je in &result.errors {
        all_errors.push(ogsql_parser::ParserError::Warning {
            message: format!("Java extraction error: {}", je),
            location: ogsql_parser::SourceLocation::default(),
            level: ogsql_parser::linter::WarningLevel::Caution,
        });
    }

    for ext in &result.extractions {
        let method = match (&ext.origin.class_name, &ext.origin.method_name) {
            (Some(cls), Some(m)) => format!("{}::{}", cls, m),
            (None, Some(m)) => m.clone(),
            (Some(cls), None) => cls.clone(),
            (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
        };

        let mut ext_errors: Vec<ogsql_parser::ParserError> = Vec::new();
        if let Some(ref parse_result) = ext.parse_result {
            all_stmts.extend(parse_result.statements.clone());
            for pe in &parse_result.errors {
                if !crate::is_warning(pe) {
                    has_parse_error = true;
                }
                all_errors.push(pe.clone());
                ext_errors.push(pe.clone());
            }
        }

        let ext_err: Vec<serde_json::Value> = ext_errors
            .iter()
            .filter(|e| !crate::is_warning(e))
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect();
        let ext_warn: Vec<serde_json::Value> = ext_errors
            .iter()
            .filter(|e| crate::is_warning(e))
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect();

        extraction_validations.push(StatementValidation {
            method,
            line: ext.origin.line,
            sql_type: format!("{:?}", ext.sql_kind),
            sql: ext.sql.trim().replace('\r', ""),
            valid: ext_err.is_empty(),
            error_count: ext_err.len(),
            warning_count: ext_warn.len(),
            errors: ext_err,
            warnings: ext_warn,
        });
    }

    let (core_errors, _pkg_errors, var_errors) =
        crate::validate_from_stmts(&all_stmts, &[], input.strict.unwrap_or(false));
    all_errors.extend(core_errors);

    let has_var_errors = !var_errors.is_empty();
    let has_real_errors = has_parse_error || has_var_errors;

    let mut response = ValidateResponse {
        valid: !has_real_errors,
        error_count: all_errors.iter().filter(|e| !crate::is_warning(e)).count() + var_errors.len(),
        warning_count: all_errors.iter().filter(|e| crate::is_warning(e)).count(),
        errors: all_errors.iter().map(|e| serde_json::to_value(e).unwrap_or_default()).collect(),
        undefined_variables: if has_var_errors {
            Some(var_errors.iter().map(|v| serde_json::to_value(v).unwrap_or_default()).collect())
        } else {
            None
        },
        package_consistency_errors: None,
        lint_warnings: None,
        lint_summary: None,
        strict_mode: if input.strict.unwrap_or(false) { Some(true) } else { None },
        statements: if extraction_validations.is_empty() { None } else { Some(extraction_validations) },
    };

    if input.lint.unwrap_or(false) {
        let lint_config = input.lint_config.as_ref().map(|c| c.to_lint_config()).unwrap_or_default();
        let linter = ogsql_parser::linter::SqlLinter::with_default_rules(lint_config);
        let lint_warnings = linter.lint(&all_stmts, None, ogsql_parser::linter::Confidence::Full);
        if !lint_warnings.is_empty() {
            response.lint_summary = Some(ogsql_parser::linter::build_lint_summary(&lint_warnings));
            response.lint_warnings =
                Some(lint_warnings.iter().map(|w| serde_json::to_value(w).unwrap_or_default()).collect());
        }
    }

    Ok(response)
}

// ─── SARIF output helper ────────────────────────────────────

/// If `format == "sarif"`, convert the response to SARIF 2.1.0 JSON.
/// Otherwise, serialize as the default JSON response.
fn maybe_sarif_response(
    response: ValidateResponse,
    source_text: &str,
    format: Option<&str>,
) -> Result<Response, ApiError> {
    match format {
        Some("sarif") => {
            let rules = ogsql_parser::linter::all_rules_metadata();
            let sarif =
                sarif::build_sarif_log(&response, source_text, &rules, "ogsql-parser", env!("CARGO_PKG_VERSION"));
            let body =
                serde_json::to_string(&sarif).map_err(|e| ApiError::Internal(format!("SARIF serialization: {e}")))?;
            Ok(Response::builder().header(CONTENT_TYPE, "application/sarif+json").body(Body::from(body)).unwrap())
        }
        _ => Ok(Json(response).into_response()),
    }
}

// ─── Multipart form-data helpers ────────────────────────────

async fn parse_validate_multipart(req: Request) -> Result<ValidateInput, ApiError> {
    let mut multipart = axum::extract::Multipart::from_request(req, &())
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart body".into()))?;

    let mut file_content = String::new();
    let mut config = ValidateMultipartConfig::default();

    while let Some(field) =
        multipart.next_field().await.map_err(|_| ApiError::BadRequest("failed to read multipart field".into()))?
    {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|_| ApiError::BadRequest("failed to read field bytes".into()))?;
        match name.as_str() {
            "file" => {
                file_content = String::from_utf8_lossy(&data).into_owned();
            }
            "config" => {
                if !data.is_empty() {
                    config = serde_json::from_slice(&data)
                        .map_err(|e| ApiError::BadRequest(format!("invalid config JSON: {}", e)))?;
                }
            }
            _ => {}
        }
    }

    if file_content.is_empty() {
        return Err(ApiError::BadRequest("missing 'file' field".into()));
    }

    Ok(ValidateInput {
        sql: file_content,
        mybatis: false,
        strict: config.strict,
        lint: config.lint,
        schema_json: None,
        lint_config: config.lint_config,
        format: config.format,
    })
}

#[cfg(feature = "ibatis")]
async fn parse_validate_xml_multipart(req: Request) -> Result<ValidateXmlInput, ApiError> {
    let mut multipart = axum::extract::Multipart::from_request(req, &())
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart body".into()))?;

    let mut file_content = String::new();
    let mut config = ValidateXmlMultipartConfig::default();

    while let Some(field) =
        multipart.next_field().await.map_err(|_| ApiError::BadRequest("failed to read multipart field".into()))?
    {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|_| ApiError::BadRequest("failed to read field bytes".into()))?;
        match name.as_str() {
            "file" => {
                file_content = String::from_utf8_lossy(&data).into_owned();
            }
            "config" => {
                if !data.is_empty() {
                    config = serde_json::from_slice(&data)
                        .map_err(|e| ApiError::BadRequest(format!("invalid config JSON: {}", e)))?;
                }
            }
            _ => {}
        }
    }

    if file_content.is_empty() {
        return Err(ApiError::BadRequest("missing 'file' field".into()));
    }

    Ok(ValidateXmlInput {
        xml: file_content,
        #[cfg(feature = "java")]
        java_src: None,
        strict: config.strict,
        lint: config.lint,
        lint_config: config.lint_config,
        format: config.format,
    })
}

#[cfg(feature = "java")]
async fn parse_validate_java_multipart(req: Request) -> Result<ValidateJavaInput, ApiError> {
    let mut multipart = axum::extract::Multipart::from_request(req, &())
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart body".into()))?;

    let mut file_content = String::new();
    let mut config = ValidateJavaMultipartConfig::default();

    while let Some(field) =
        multipart.next_field().await.map_err(|_| ApiError::BadRequest("failed to read multipart field".into()))?
    {
        let name = field.name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|_| ApiError::BadRequest("failed to read field bytes".into()))?;
        match name.as_str() {
            "file" => {
                file_content = String::from_utf8_lossy(&data).into_owned();
            }
            "config" => {
                if !data.is_empty() {
                    config = serde_json::from_slice(&data)
                        .map_err(|e| ApiError::BadRequest(format!("invalid config JSON: {}", e)))?;
                }
            }
            _ => {}
        }
    }

    if file_content.is_empty() {
        return Err(ApiError::BadRequest("missing 'file' field".into()));
    }

    Ok(ValidateJavaInput {
        source: file_content,
        extra_sql_methods: config.extra_sql_methods,
        extra_sql_var_patterns: config.extra_sql_var_patterns,
        strict: config.strict,
        lint: config.lint,
        lint_config: config.lint_config,
        format: config.format,
    })
}

// ── helpers ──────────────────────────────────────────────────

fn statement_method_name(stmt: &ogsql_parser::Statement) -> String {
    use ogsql_parser::Statement;
    match stmt {
        Statement::CreateProcedure(s) => s.name.join("."),
        Statement::CreateFunction(s) => s.name.join("."),
        Statement::CreatePackage(s) => s.name.join("."),
        Statement::CreatePackageBody(s) => s.name.join("."),
        _ => String::new(),
    }
}
