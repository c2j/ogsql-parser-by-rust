use super::*;

// ── Parameter deserialization tests ────────────────────────────────────

#[test]
fn test_parse_params_deserialization() {
    let json = r#"{"sql": "SELECT 1", "preserve_comments": true}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.sql, "SELECT 1");
    assert!(params.preserve_comments);
}

#[test]
fn test_parse_params_default_preserve_comments() {
    let json = r#"{"sql": "SELECT 1"}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.sql, "SELECT 1");
    assert!(!params.preserve_comments);
}

#[test]
fn test_tokenize_params_deserialization() {
    let json = r#"{"sql": "SELECT * FROM t"}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.sql, "SELECT * FROM t");
}

#[test]
fn test_tokenize_params_default_summary_is_false() {
    let json = r#"{"sql": "SELECT * FROM t"}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    assert!(!params.summary);
}

#[test]
fn test_tokenize_params_summary_true() {
    let json = r#"{"sql": "SELECT * FROM t", "summary": true}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    assert!(params.summary);
}

#[test]
fn test_format_params_defaults() {
    let json = r#"{"sql": "select 1", "keyword_case": "", "comma_style": "", "uppercase": false}"#;
    let params: FormatParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.sql, "select 1");
    assert_eq!(params.indent, 2);
    assert_eq!(params.line_width, 120);
}

#[test]
fn test_format_params_custom() {
    let json = r#"{"sql": "select 1", "indent": 4, "keyword_case": "upper", "comma_style": "leading", "line_width": 80, "uppercase": true}"#;
    let params: FormatParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.indent, 4);
    assert_eq!(params.line_width, 80);
    assert_eq!(params.keyword_case, "upper");
    assert_eq!(params.comma_style, "leading");
}

#[test]
fn test_validate_params_deserialization() {
    let json = r#"{"sql": "SELECT * FROM"}"#;
    let params: ValidateParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.sql, "SELECT * FROM");
}

#[test]
fn test_json2sql_params_deserialization() {
    let json = r#"{"json": "[]"}"#;
    let params: Json2SqlParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.json, "[]");
}

#[test]
fn test_parse_xml_params_deserialization() {
    let json = r#"{"xml": "<mapper></mapper>"}"#;
    let params: ParseXmlParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.xml, "<mapper></mapper>");
    assert!(!params.lint);
}

#[test]
fn test_parse_java_params_deserialization() {
    let json = r#"{"source": "class Foo {}", "extra_sql_methods": [], "extra_sql_var_patterns": []}"#;
    let params: ParseJavaParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.source, "class Foo {}");
    assert!(params.extra_sql_methods.is_empty());
    assert!(params.extra_sql_var_patterns.is_empty());
    assert!(!params.lint);
}

// ── Tool functionality tests ───────────────────────────────────────────

#[test]
fn test_parse_tool_valid_sql() {
    let server = OgsqlServer;
    let json = r#"{"sql": "SELECT 1", "preserve_comments": false}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    let result = server.parse(Parameters(params));
    assert!(result.contains("\"statements\""));
    assert!(result.contains("\"errors\""));
}

#[test]
fn test_parse_tool_invalid_sql() {
    let server = OgsqlServer;
    let json = r#"{"sql": "BROKEN SYNTAX !!! @@@", "preserve_comments": false}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    let result = server.parse(Parameters(params));
    assert!(result.contains("\"statements\""));
    // Even invalid SQL should return a result (with errors array)
}

#[test]
fn test_tokenize_tool() {
    let server = OgsqlServer;
    let json = r#"{"sql": "SELECT id FROM users"}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    let result = server.tokenize(Parameters(params));
    assert!(result.contains("\"tokens\""));
    assert!(result.contains("\"type\""));
}

#[test]
fn test_tokenize_tool_summary_false_keeps_full_token_list() {
    let server = OgsqlServer;
    let json = r#"{"sql": "SELECT id FROM users", "summary": false}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    let result = server.tokenize(Parameters(params));
    assert!(result.contains("\"tokens\""));
    assert!(!result.contains("\"total_tokens\""));
}

#[test]
fn test_tokenize_tool_summary_true_returns_aggregated_stats() {
    let server = OgsqlServer;
    let sql = "SELECT id, email, COALESCE(bonus, 0) FROM employees e \
               JOIN departments d ON e.dept_id = d.id \
               WHERE d.id IN (SELECT id FROM archived)";
    let json = serde_json::json!({"sql": sql, "summary": true}).to_string();
    let params: TokenizeParams = serde_json::from_str(&json).unwrap();
    let result = server.tokenize(Parameters(params));

    let value: serde_json::Value = serde_json::from_str(&result).expect("summary output must be valid JSON");
    assert!(!result.contains("\"tokens\""), "summary mode must not include full token list");

    let total_tokens = value["total_tokens"].as_u64().expect("total_tokens must be an integer");
    assert!(total_tokens > 0);

    let by_type = value["by_type"].as_object().expect("by_type must be an object");
    assert!(by_type.contains_key("Keyword"));
    assert!(by_type.contains_key("Ident"));

    let tables: Vec<String> =
        value["tables"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(tables.contains(&"employees".to_string()));
    assert!(tables.contains(&"departments".to_string()));
    assert!(tables.contains(&"archived".to_string()));

    let columns: Vec<String> =
        value["columns"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(columns.contains(&"id".to_string()));
    assert!(columns.contains(&"email".to_string()));
    assert!(columns.contains(&"bonus".to_string()));

    let functions: Vec<String> =
        value["functions"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(functions.contains(&"coalesce".to_string()));

    assert!(value["has_subquery"].as_bool().unwrap());
    assert!(value["has_join"].as_bool().unwrap());
}

#[test]
fn test_tokenize_tool_summary_true_no_join_no_subquery() {
    let server = OgsqlServer;
    let json = r#"{"sql": "SELECT id FROM users", "summary": true}"#;
    let params: TokenizeParams = serde_json::from_str(json).unwrap();
    let result = server.tokenize(Parameters(params));
    let value: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(!value["has_join"].as_bool().unwrap());
    assert!(!value["has_subquery"].as_bool().unwrap());
}

#[test]
fn test_format_tool() {
    let server = OgsqlServer;
    let json = r#"{"sql": "select id,name from users where id=1", "indent": 2, "keyword_case": "upper", "comma_style": "trailing", "line_width": 120, "uppercase": false}"#;
    let params: FormatParams = serde_json::from_str(json).unwrap();
    let result = server.format(Parameters(params));
    assert!(result.contains("\"formatted\""));
}

#[test]
fn test_validate_tool_valid() {
    let server = OgsqlServer;
    let json = r#"{"sql": "SELECT 1"}"#;
    let params: ValidateParams = serde_json::from_str(json).unwrap();
    let result = server.validate(Parameters(params));
    assert!(result.contains("\"valid\""));
}

#[test]
fn test_validate_tool_invalid() {
    let server = OgsqlServer;
    let json = r#"{"sql": "BROKEN !!! @@@ SYNTAX"}"#;
    let params: ValidateParams = serde_json::from_str(json).unwrap();
    let result = server.validate(Parameters(params));
    assert!(result.contains("\"valid\""));
    assert!(result.contains("\"errors\""));
}

// ── file_path parameter tests ──────────────────────────────────────────

fn write_temp_sql_file(test_name: &str, ext: &str, content: &str) -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("ogsql_mcp_test_{}_{}_{}.{}", std::process::id(), test_name, n, ext));
    std::fs::write(&path, content).expect("write temp sql file");
    path
}

#[test]
fn test_parse_params_default_file_path_is_none() {
    let json = r#"{"sql": "SELECT 1"}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    assert!(params.file_path.is_none());
}

#[test]
fn test_parse_tool_file_path_reads_sql_file() {
    let path = write_temp_sql_file("parse", "sql", "SELECT 1");
    let server = OgsqlServer;
    let json = serde_json::json!({"file_path": path.to_str().unwrap()}).to_string();
    let params: ParseParams = serde_json::from_str(&json).unwrap();
    let result = server.parse(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"statements\""), "expected parsed statements: {}", result);
    assert!(!result.contains("\"error\""), "unexpected error: {}", result);
}

#[test]
fn test_parse_tool_file_path_nonexistent_file_returns_error() {
    let server = OgsqlServer;
    let missing = std::env::temp_dir().join("ogsql_mcp_test_does_not_exist_12345.sql");
    let json = serde_json::json!({"file_path": missing.to_str().unwrap()}).to_string();
    let params: ParseParams = serde_json::from_str(&json).unwrap();
    let result = server.parse(Parameters(params));
    assert!(result.contains("\"error\""), "expected error for missing file: {}", result);
}

#[test]
fn test_parse_tool_both_sql_and_file_path_returns_error() {
    let path = write_temp_sql_file("parse_both", "sql", "SELECT 1");
    let server = OgsqlServer;
    let json = serde_json::json!({"sql": "SELECT 2", "file_path": path.to_str().unwrap()}).to_string();
    let params: ParseParams = serde_json::from_str(&json).unwrap();
    let result = server.parse(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"error\""), "expected error when both sql and file_path set: {}", result);
}

#[test]
fn test_parse_tool_neither_sql_nor_file_path_returns_error() {
    let server = OgsqlServer;
    let json = r#"{}"#;
    let params: ParseParams = serde_json::from_str(json).unwrap();
    let result = server.parse(Parameters(params));
    assert!(result.contains("\"error\""), "expected error when neither sql nor file_path set: {}", result);
}

#[test]
fn test_parse_tool_file_path_unsupported_extension_returns_error() {
    let path = write_temp_sql_file("parse_ext", "txt", "SELECT 1");
    let server = OgsqlServer;
    let json = serde_json::json!({"file_path": path.to_str().unwrap()}).to_string();
    let params: ParseParams = serde_json::from_str(&json).unwrap();
    let result = server.parse(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"error\""), "expected error for unsupported extension: {}", result);
}

#[test]
fn test_validate_tool_file_path_reads_sql_file() {
    let path = write_temp_sql_file("validate", "pck", "SELECT 1");
    let server = OgsqlServer;
    let json = serde_json::json!({"file_path": path.to_str().unwrap()}).to_string();
    let params: ValidateParams = serde_json::from_str(&json).unwrap();
    let result = server.validate(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"valid\""), "expected validate output: {}", result);
}

#[test]
fn test_validate_tool_both_sql_and_file_path_returns_error() {
    let path = write_temp_sql_file("validate_both", "sql", "SELECT 1");
    let server = OgsqlServer;
    let json = serde_json::json!({"sql": "SELECT 2", "file_path": path.to_str().unwrap()}).to_string();
    let params: ValidateParams = serde_json::from_str(&json).unwrap();
    let result = server.validate(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"error\""), "expected error when both sql and file_path set: {}", result);
}

#[test]
fn test_validate_tool_neither_sql_nor_file_path_returns_error() {
    let server = OgsqlServer;
    let json = r#"{}"#;
    let params: ValidateParams = serde_json::from_str(json).unwrap();
    let result = server.validate(Parameters(params));
    assert!(result.contains("\"error\""), "expected error when neither sql nor file_path set: {}", result);
}

#[test]
fn test_format_tool_file_path_reads_sql_file() {
    let path = write_temp_sql_file("format", "fnc", "select id,name from users where id=1");
    let server = OgsqlServer;
    let json = serde_json::json!({"file_path": path.to_str().unwrap()}).to_string();
    let params: FormatParams = serde_json::from_str(&json).unwrap();
    let result = server.format(Parameters(params));
    let _ = std::fs::remove_file(&path);
    assert!(result.contains("\"formatted\""), "expected formatted output: {}", result);
}

#[test]
fn test_format_tool_file_path_nonexistent_file_returns_error() {
    let server = OgsqlServer;
    let missing = std::env::temp_dir().join("ogsql_mcp_test_does_not_exist_format.prc");
    let json = serde_json::json!({"file_path": missing.to_str().unwrap()}).to_string();
    let params: FormatParams = serde_json::from_str(&json).unwrap();
    let result = server.format(Parameters(params));
    assert!(result.contains("\"error\""), "expected error for missing file: {}", result);
}

#[test]
fn test_format_tool_neither_sql_nor_file_path_returns_error() {
    let server = OgsqlServer;
    let json = r#"{"indent": 2, "keyword_case": "", "comma_style": "", "uppercase": false}"#;
    let params: FormatParams = serde_json::from_str(json).unwrap();
    let result = server.format(Parameters(params));
    assert!(result.contains("\"error\""), "expected error when neither sql nor file_path set: {}", result);
}

#[test]
fn test_parse_java_tool_lint_true_reports_warnings() {
    let server = OgsqlServer;
    let json = r#"{
        "source": "public interface UserRepository { @Query(value = \"SELECT * FROM t1\", nativeQuery = true) List<User> findAll(); }",
        "extra_sql_methods": [],
        "extra_sql_var_patterns": [],
        "lint": true
    }"#;
    let params: ParseJavaParams = serde_json::from_str(json).unwrap();
    let result = server.parse_java(Parameters(params));
    assert!(result.contains("\"lint_warnings\""), "expected lint_warnings in output: {}", result);
    assert!(result.contains("\"R001\""), "expected R001 (SELECT *) warning: {}", result);
    assert!(result.contains("\"lint_summary\""), "expected top-level lint_summary: {}", result);
}

#[test]
fn test_parse_java_tool_lint_false_omits_warnings() {
    let server = OgsqlServer;
    let json = r#"{
        "source": "public interface UserRepository { @Query(value = \"SELECT * FROM t1\", nativeQuery = true) List<User> findAll(); }",
        "extra_sql_methods": [],
        "extra_sql_var_patterns": []
    }"#;
    let params: ParseJavaParams = serde_json::from_str(json).unwrap();
    let result = server.parse_java(Parameters(params));
    assert!(!result.contains("\"lint_warnings\""), "lint disabled but found lint_warnings: {}", result);
    assert!(!result.contains("\"lint_summary\""), "lint disabled but found lint_summary: {}", result);
}

#[test]
fn test_parse_xml_tool_lint_true_reports_warnings() {
    let server = OgsqlServer;
    let json = r#"{
        "xml": "<mapper namespace=\"test\"><select id=\"find\">SELECT * FROM t1</select></mapper>",
        "lint": true
    }"#;
    let params: ParseXmlParams = serde_json::from_str(json).unwrap();
    let result = server.parse_xml(Parameters(params));
    assert!(result.contains("\"lint_warnings\""), "expected lint_warnings in output: {}", result);
    assert!(result.contains("\"R001\""), "expected R001 (SELECT *) warning: {}", result);
    assert!(result.contains("\"lint_summary\""), "expected top-level lint_summary: {}", result);
}

#[test]
fn test_parse_xml_tool_lint_false_omits_warnings() {
    let server = OgsqlServer;
    let json = r#"{
        "xml": "<mapper namespace=\"test\"><select id=\"find\">SELECT * FROM t1</select></mapper>"
    }"#;
    let params: ParseXmlParams = serde_json::from_str(json).unwrap();
    let result = server.parse_xml(Parameters(params));
    assert!(!result.contains("\"lint_warnings\""), "lint disabled but found lint_warnings: {}", result);
    assert!(!result.contains("\"lint_summary\""), "lint disabled but found lint_summary: {}", result);
}

#[test]
fn test_json2sql_tool_bad_json() {
    let server = OgsqlServer;
    let json = r#"{"json": "not valid json at all {{{"}"#;
    let params: Json2SqlParams = serde_json::from_str(json).unwrap();
    let result = server.json2sql(Parameters(params));
    assert!(result.contains("\"error\""));
}

// ── Helper function tests ──────────────────────────────────────────────

#[test]
fn test_is_warning() {
    let warning = crate::ParserError::Warning {
        message: "test".to_string(),
        location: crate::SourceLocation::default(),
        level: crate::linter::WarningLevel::Suggestion,
    };
    assert!(is_warning(&warning));

    let error =
        crate::ParserError::UnexpectedEof { expected: "stmt".to_string(), location: crate::SourceLocation::default() };
    assert!(!is_warning(&error));
}

#[test]
fn test_token_display_keyword() {
    let tok = crate::TokenWithSpan {
        token: crate::Token::Keyword(crate::Keyword::SELECT),
        span: crate::Span { start: 0, end: 6 },
        location: crate::SourceLocation::default(),
    };
    let (ty, val) = token_display(&tok);
    assert_eq!(ty, "Keyword");
    assert!(val.contains("SELECT"));
}

#[test]
fn test_token_display_ident() {
    let tok = crate::TokenWithSpan {
        token: crate::Token::Ident("my_col".to_string()),
        span: crate::Span { start: 0, end: 6 },
        location: crate::SourceLocation::default(),
    };
    let (ty, val) = token_display(&tok);
    assert_eq!(ty, "Ident");
    assert_eq!(val, "my_col");
}

#[test]
fn test_token_display_integer() {
    let tok = crate::TokenWithSpan {
        token: crate::Token::Integer(42),
        span: crate::Span { start: 0, end: 2 },
        location: crate::SourceLocation::default(),
    };
    let (ty, val) = token_display(&tok);
    assert_eq!(ty, "Integer");
    assert_eq!(val, "42");
}
