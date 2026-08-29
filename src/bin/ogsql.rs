// Pre-existing code issues that became deny-by-warning in Rust 1.93. Fix gradually.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::if_same_then_else,
    dead_code,
    clippy::format_in_format_args
)]

use std::io::Read as _;

use clap::{Parser as ClapParser, Subcommand};
use ogsql_parser::token_formatter::{CommaStyle, FormatConfig, KeywordCase};
use ogsql_parser::*;
use serde::Serialize;

const OGSQL_LOGO: &str = r#"
  ██████╗  ██████╗ ███████╗ ██████╗ ██╗      ██████╗  █████╗ ██████╗ ███████╗███████╗██████╗
 ██╔═══██╗██╔════╝ ██╔════╝██╔═══██╗██║      ██╔══██╗██╔══██╗██╔══██╗██╔════╝██╔════╝██╔══██╗
 ██║   ██║██║  ███╗███████╗██║   ██║██║      ██████╔╝███████║██████╔╝███████╗███████╗██████╔╝
 ██║   ██║██║   ██║╚════██║██║   ██║██║      ██╔═══╝ ██╔══██║██╔══██╗╚════██║██╔═══╝ ██╔══██╗
 ╚██████╔╝╚██████╔╝███████║╚██████╔╝███████╗ ██║     ██║  ██║██║  ██║███████║███████╗██║  ██║
  ╚═════╝  ╚═════╝ ╚══════╝ ╚═════╝ ╚══════╝ ╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝"#;

#[derive(ClapParser)]
#[command(
    name = "ogsql",
    version,
    about = "openGauss/GaussDB SQL Parser",
    help_template = "\
{before-help}{name} {version}
{about-with-newline}\
{usage-heading} {usage}

{all-args}{after-help}\
",
    before_help = OGSQL_LOGO,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Read SQL from file(s) instead of stdin (can specify multiple times)
    /// 从文件读取 SQL（可多次指定）
    #[arg(short = 'f', long, global = true)]
    file: Vec<String>,

    #[arg(short = 'j', long, global = true)]
    json: bool,

    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    #[arg(long = "schema-json", global = true)]
    schema_json: Option<String>,

    #[arg(long, global = true)]
    comments: bool,

    #[arg(long, global = true)]
    /// Enable MyBatis #{param, jdbcType=...} and ${expr} placeholder support.
    /// Preserves MyBatis placeholders during formatting and tokenization.
    /// 启用 MyBatis #{param} 和 ${expr} 占位符支持，格式化时保留占位符
    mybatis: bool,

    /// Enable SQL anti-pattern linting on parse/validate/parse-xml/parse-java/validate-xml/validate-java output
    /// 启用 SQL 反模式检测（配合 parse/validate/parse-xml/parse-java/validate-xml/validate-java 使用）
    #[arg(long, global = true)]
    lint: bool,

    /// Minimum warning level to report: prohibition, performance, caution, suggestion
    /// 最低报告级别（配合 --lint 使用）
    #[arg(long = "min-level", global = true)]
    min_level: Option<String>,

    /// Minimum confidence to report: full, partial
    /// 最低可信度（配合 --lint 使用）
    #[arg(long = "min-confidence", global = true)]
    min_confidence: Option<String>,

    /// Suppress specific rule IDs (comma-separated)
    /// 禁用指定规则（逗号分隔，配合 --lint 使用）
    #[arg(long = "suppress", global = true, value_delimiter = ',')]
    suppress: Vec<String>,

    /// P003 IN list size threshold (default: 500)
    #[arg(long = "in-list-threshold", global = true)]
    in_list_threshold: Option<usize>,

    /// P014 subquery nesting depth limit (default: 3)
    #[arg(long = "subquery-depth-limit", global = true)]
    subquery_depth_limit: Option<usize>,

    /// P007 non-equi join count limit (default: 2)
    #[arg(long = "non-equi-join-limit", global = true)]
    non_equi_join_limit: Option<usize>,

    /// Path to .ogsql-lint.toml config file
    /// 配置文件路径
    #[arg(long = "lint-config", global = true)]
    lint_config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Format SQL statements with configurable indentation, keyword casing,
    /// comma style, and line width. Supports SELECT, INSERT, DELETE, UPDATE,
    /// MERGE, WITH (CTE), CREATE TABLE, and PL/pgSQL.
    /// 格式化 SQL 语句，支持缩进、关键字大小写、逗号风格、行宽等配置
    Format {
        /// Indentation width in spaces
        #[arg(short = 'i', long, default_value_t = 2)]
        indent: usize,
        /// Keyword casing: preserve, upper, lower
        #[arg(short = 'k', long, default_value = "preserve")]
        keyword_case: String,
        /// Comma style: trailing, leading
        #[arg(long, default_value = "trailing")]
        comma: String,
        /// Maximum line width (0 = unlimited)
        #[arg(short = 'w', long, default_value_t = 120)]
        line_width: usize,
        /// Shorthand for --keyword-case upper
        #[arg(short = 'u', long)]
        uppercase: bool,
        /// Don't put each SELECT column on its own line
        #[arg(long)]
        no_select_newline: bool,
        /// Don't put AND/OR on new lines
        #[arg(long)]
        no_logical_newline: bool,
        /// Don't put semicolons on their own line
        #[arg(long)]
        no_semicolon_newline: bool,
    },
    /// Parse SQL into AST and print the abstract syntax tree / 解析 SQL 为 AST
    Parse {
        /// Recursively scan directory for SQL files (can specify multiple times)
        /// 递归扫描目录中的 SQL 文件（可多次指定）
        #[arg(short = 'd', long = "dir")]
        dir: Vec<String>,
        /// File extensions to scan, comma-separated (default: sql) / 扫描的文件扩展名，逗号分隔
        #[arg(short = 'e', long = "ext", value_delimiter = ',', default_value = "sql")]
        ext: Vec<String>,
        /// Output in CSV format (flat, one row per statement; packages expanded) / 以 CSV 格式输出
        #[arg(long = "csv")]
        csv: bool,
        /// Target directory for JSON output files (required with --dir -j; preserves directory structure)
        /// JSON 文件输出目录（配合 --dir -j 使用，保留目录层次结构）
        #[arg(short = 'o', long = "output-dir")]
        output_dir: Option<String>,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
        /// Only parse the specified stored procedure/function (requires --file)
        /// 仅解析指定的存储过程/函数（需要指定 --file）
        #[arg(long)]
        procedure: Option<String>,
        /// Extract SQL statements from stored procedures (one row per SQL; variables → __SQL_PARAM_Type_Name__ / __SQL_RAW_Type_Name__)
        /// 从存储过程中提取 SQL 语句（每行一条 SQL；变量转为参数占位符）
        #[arg(long = "extract-sql")]
        extract_sql: bool,
    },
    /// Convert JSON (from `parse -j`) back to SQL / 将 JSON（parse -j 的输出）还原为 SQL
    #[command(name = "json2sql")]
    JsonToSql,
    /// Tokenize SQL into a list of tokens / 将 SQL 分词为 token 列表
    Tokenize,
    /// Validate SQL syntax and report errors / 校验 SQL 语法
    Validate {
        /// Recursively scan directory for SQL files (can specify multiple times)
        /// 递归扫描目录中的 SQL 文件（可多次指定）
        #[arg(short = 'd', long = "dir")]
        dir: Vec<String>,
        /// File extensions to scan, comma-separated (default: sql) / 扫描的文件扩展名，逗号分隔
        #[arg(short = 'e', long = "ext", value_delimiter = ',', default_value = "sql")]
        ext: Vec<String>,
        /// Output validation results in CSV format / 以 CSV 格式输出校验结果
        #[arg(long = "csv")]
        csv: bool,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
        /// Enable strict mode: detect undefined function calls in PL blocks / 启用严格模式：检测 PL 块中的未定义函数调用
        #[arg(long)]
        strict: bool,
    },
    #[cfg(feature = "serve")]
    /// Start an HTTP API server for parsing SQL / 启动 HTTP API 服务器
    Serve {
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// Start a stdio line-delimited JSON protocol server. Reads one JSON request per
    /// line on stdin and writes one JSON response per line on stdout — a long-lived
    /// connection for embedded clients (Java/Python/Node). No extra features required.
    /// 启动 stdio 长连接 NDJSON 协议服务（每行一个 JSON 请求/响应，供嵌入客户端使用）
    #[command(name = "serve-stdio")]
    ServeStdio,
    #[cfg(feature = "tui")]
    /// Launch an interactive terminal UI playground / 启动交互式终端演练场
    Playground,
    #[cfg(feature = "ibatis")]
    /// Parse iBatis/MyBatis XML mapper file / 解析 iBatis XML mapper 文件
    #[command(name = "parse-xml")]
    ParseXml {
        /// Recursively scan directory for XML files / 递归扫描目录中的 XML 文件
        #[arg(short = 'd', long = "dir")]
        dir: Option<String>,
        /// Output in CSV format / 以 CSV 格式输出
        #[arg(long = "csv")]
        csv: bool,
        #[cfg(feature = "java")]
        /// Java source root directory for parameter type inference / Java 源码根目录，用于参数类型推断
        #[arg(long = "java-src")]
        java_src: Option<String>,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
        /// Output structured dynamic SQL AST (preserves SqlNode tree instead of flattening)
        #[arg(long)]
        structured: bool,
    },
    #[cfg(feature = "ibatis")]
    /// Validate iBatis/MyBatis XML mapper — parse + semantic checks + lint
    /// 校验 iBatis/MyBatis XML mapper 文件（解析 + 语义校验 + lint）
    #[command(name = "validate-xml")]
    ValidateXml {
        /// Recursively scan directory for XML files / 递归扫描目录
        #[arg(short = 'd', long = "dir")]
        dir: Option<String>,
        /// Output in CSV format / CSV 格式输出
        #[arg(long = "csv")]
        csv: bool,
        #[cfg(feature = "java")]
        /// Java source root directory for parameter type inference
        #[arg(long = "java-src")]
        java_src: Option<String>,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
        /// Enable strict mode: detect undefined function calls in PL blocks
        #[arg(long)]
        strict: bool,
    },
    #[cfg(feature = "mcp")]
    /// Start MCP server on stdio (for Claude Desktop, Cursor, etc.) / 启动 MCP 服务器（stdio 模式）
    Mcp,
    #[cfg(feature = "java")]
    /// Extract and parse SQL from Java source files / 从 Java 源文件中提取并解析 SQL
    #[command(name = "parse-java")]
    ParseJava {
        #[arg(long = "extra-sql-methods", value_delimiter = ',')]
        extra_sql_methods: Vec<String>,
        #[arg(long = "extra-sql-var-patterns", value_delimiter = ',')]
        extra_sql_var_patterns: Vec<String>,
        /// Recursively scan directory for Java files / 递归扫描目录中的 Java 文件
        #[arg(short = 'd', long = "dir")]
        dir: Option<String>,
        /// Output in CSV format / 以 CSV 格式输出
        #[arg(long = "csv")]
        csv: bool,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
    },
    #[cfg(feature = "java")]
    /// Validate Java source — extract SQL + semantic checks + lint
    /// 校验 Java 源码（提取 SQL + 语义校验 + lint）
    #[command(name = "validate-java")]
    ValidateJava {
        #[arg(long = "extra-sql-methods", value_delimiter = ',')]
        extra_sql_methods: Vec<String>,
        #[arg(long = "extra-sql-var-patterns", value_delimiter = ',')]
        extra_sql_var_patterns: Vec<String>,
        /// Recursively scan directory for Java files / 递归扫描目录
        #[arg(short = 'd', long = "dir")]
        dir: Option<String>,
        /// Output in CSV format / CSV 格式输出
        #[arg(long = "csv")]
        csv: bool,
        /// Print statistics after directory processing
        #[arg(long)]
        stats: bool,
        /// Enable strict mode: detect undefined function calls in PL blocks
        #[arg(long)]
        strict: bool,
    },
}

macro_rules! die {
    ($($t:tt)*) => {{ eprintln!($($t)*); std::process::exit(1); }};
}

fn annotate_builtin_functions(_value: &mut serde_json::Value) {}

fn build_lint_config(cli: &Cli) -> ogsql_parser::linter::LintConfig {
    use ogsql_parser::linter::{Confidence, LintConfig, WarningLevel};

    // 1. Load from config file if specified via --lint-config
    // 2. Otherwise try standard paths (.ogsql-lint.toml, XDG)
    // 3. Fall back to defaults
    let mut config = if let Some(ref path) = cli.lint_config {
        LintConfig::load_from_file(std::path::Path::new(path))
            .unwrap_or_else(|e| die!("Error loading lint config '{path}': {e}"))
    } else {
        LintConfig::find_and_load().unwrap_or_else(|e| die!("Error searching lint config: {e}")).unwrap_or_default()
    };

    // CLI overrides take precedence over config file
    if let Some(ref level) = cli.min_level {
        config.min_level = match level.to_lowercase().as_str() {
            "prohibition" => WarningLevel::Prohibition,
            "performance" => WarningLevel::Performance,
            "caution" => WarningLevel::Caution,
            _ => WarningLevel::Suggestion,
        };
    }
    if let Some(ref conf) = cli.min_confidence {
        config.min_confidence = match conf.to_lowercase().as_str() {
            "full" => Confidence::Full,
            _ => Confidence::Partial,
        };
    }
    if !cli.suppress.is_empty() {
        config.suppress = cli.suppress.clone();
    }
    if let Some(t) = cli.in_list_threshold {
        config.in_list_threshold = t;
    }
    if let Some(t) = cli.subquery_depth_limit {
        config.subquery_depth_limit = t;
    }
    if let Some(t) = cli.non_equi_join_limit {
        config.non_equi_join_limit = t;
    }
    config
}

fn load_lint_schema(cli: &Cli) -> Option<ogsql_parser::FullSchema> {
    cli.schema_json.as_deref().and_then(|p| ogsql_parser::load_full_schema(p).ok())
}

fn run_lint(
    stmts: &[ogsql_parser::StatementInfo],
    confidence: ogsql_parser::linter::Confidence,
    config: &ogsql_parser::linter::LintConfig,
    schema: Option<&ogsql_parser::FullSchema>,
) -> Vec<ogsql_parser::linter::SqlWarning> {
    let mut linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
    let mut columns = None;

    if let Some(fs) = schema {
        if !fs.indexes.is_empty() {
            linter.set_index_info(fs.indexes.clone());
        }
        columns = Some(&fs.columns);
    }

    linter.lint(stmts, columns, confidence)
}

fn format_warnings_text(warnings: &[ogsql_parser::linter::SqlWarning]) {
    use ogsql_parser::linter::WarningLevel;
    for w in warnings {
        let icon = match w.level {
            WarningLevel::Prohibition => "\u{1f534}",
            WarningLevel::Performance => "\u{1f7e1}",
            WarningLevel::Caution => "\u{1f535}",
            WarningLevel::Suggestion => "\u{26aa}",
        };
        let conf = match w.confidence {
            ogsql_parser::linter::Confidence::Partial => " [partial]",
            ogsql_parser::linter::Confidence::Full => "",
        };
        eprintln!("  {icon} {} {}{conf}", w.rule_id, w.message);
        if let Some(ref s) = w.suggestion {
            eprintln!("          \u{2192} {s}");
        }
    }
}

fn format_warnings_summary(warnings: &[ogsql_parser::linter::SqlWarning]) -> serde_json::Value {
    ogsql_parser::linter::build_lint_summary(warnings)
}

#[cfg(feature = "ibatis")]
fn lint_xml_statements(
    parsed: &[ogsql_parser::ibatis::types::ParsedStatement],
    config: &ogsql_parser::linter::LintConfig,
) -> Vec<ogsql_parser::linter::SqlWarning> {
    let mut all_warnings = Vec::new();
    let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
    for stmt in parsed {
        if let Some((ref stmts, _)) = stmt.parse_result {
            let mut ws = linter.lint(stmts, None, ogsql_parser::linter::Confidence::Partial);
            all_warnings.append(&mut ws);
        }
    }
    all_warnings
}

#[cfg(feature = "java")]
fn lint_java_extractions(
    extractions: &[ogsql_parser::java::types::ExtractedSql],
    config: &ogsql_parser::linter::LintConfig,
) -> Vec<ogsql_parser::linter::SqlWarning> {
    let mut all_warnings = Vec::new();
    let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
    for ext in extractions {
        if let Some(ref parse_result) = ext.parse_result {
            let mut ws = linter.lint(&parse_result.statements, None, ogsql_parser::linter::Confidence::Partial);
            all_warnings.append(&mut ws);
        }
    }
    all_warnings
}

fn read_input(file: Option<&str>) -> String {
    match file {
        Some(path) => {
            let bytes = std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e));
            token::decode_sql_file(&bytes).unwrap_or_else(|e| die!("Error decoding {}: {}", path, e)).0
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            buf
        }
    }
}

fn parse_input(sql: &str, preserve_comments: bool, mybatis_params: bool) -> ogsql_parser::ParseOutput {
    let options = ogsql_parser::ParseOptions { preserve_comments, mybatis_params };
    ogsql_parser::Parser::parse_sql_with_options(sql, options)
}

fn token_display(t: &TokenWithSpan) -> (String, String) {
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
        Token::MyBatisParam(s) => ("MyBatisParam".into(), format!("#{{{}}}", s)),
        Token::MyBatisRawExpr(s) => ("MyBatisRawExpr".into(), format!("${{{}}}", s)),
        Token::JdbcParam => ("JdbcParam".into(), "?".to_string()),
        other => ("Other".into(), format!("{:?}", other)),
    }
}

#[derive(Serialize)]
struct TokenInfo {
    #[serde(rename = "type")]
    token_type: String,
    value: String,
    line: usize,
    column: usize,
}

fn cmd_format(
    cli: &Cli,
    indent: usize,
    keyword_case: String,
    comma: String,
    line_width: usize,
    uppercase: bool,
    no_select_newline: bool,
    no_logical_newline: bool,
    no_semicolon_newline: bool,
) {
    if cli.file.len() > 1 {
        die!("Error: format command accepts at most one --file");
    }
    let sql = read_input(cli.file.first().map(|s| s.as_str()));
    let formatted = format_sql_to_string(
        &sql,
        cli.mybatis,
        indent,
        &keyword_case,
        &comma,
        line_width,
        uppercase,
        no_select_newline,
        no_logical_newline,
        no_semicolon_newline,
    )
    .unwrap_or_else(|e| die!("{}", e));

    if cli.json {
        let out = serde_json::json!({
            "formatted": formatted,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("{}", formatted);
        if !formatted.ends_with('\n') {
            println!();
        }
    }
}

/// Shared format logic for the CLI and the stdio protocol server.
fn format_sql_to_string(
    sql: &str,
    mybatis: bool,
    indent: usize,
    keyword_case: &str,
    comma: &str,
    line_width: usize,
    uppercase: bool,
    no_select_newline: bool,
    no_logical_newline: bool,
    no_semicolon_newline: bool,
) -> Result<String, String> {
    let mut tokenizer = Tokenizer::new(sql).preserve_comments(true);
    if mybatis {
        tokenizer = tokenizer.mybatis_params(true);
    }
    let tokens = match tokenizer.tokenize() {
        Ok(t) => t,
        Err(e) => return Err(format!("Tokenization error: {}", e)),
    };
    let kw_case = match keyword_case {
        "upper" => KeywordCase::Upper,
        "lower" => KeywordCase::Lower,
        _ => KeywordCase::Preserve,
    };
    let comma_style = match comma {
        "leading" => CommaStyle::Leading,
        _ => CommaStyle::Trailing,
    };

    // Try AST-based formatting first; fall back to token-based formatting
    // when parsing fails (syntax errors, unsupported constructs).
    // AST path tokenizes without comments (AST formatter drops comments anyway);
    // token fallback path uses the comment-preserving tokens from above.
    let ast_tokens = Tokenizer::new(sql).tokenize();
    let ast_ok = if let Ok(ref at) = ast_tokens {
        let stmts = Parser::new(at.clone()).parse();
        !stmts.iter().any(|s| matches!(s, Statement::Empty))
    } else {
        false
    };

    let formatted = if ast_ok && !no_select_newline {
        let upper = uppercase || kw_case == KeywordCase::Upper;
        let fmt = SqlFormatter::new().uppercase_keywords(upper).pretty_print(true).indent(0);
        let stmts = Parser::new(ast_tokens.unwrap()).parse();
        stmts.iter().map(|s| fmt.format_statement(s)).collect::<Vec<_>>().join(";\n")
    } else {
        let config = FormatConfig {
            indent_width: indent,
            keyword_case: kw_case,
            comma_style,
            line_width,
            uppercase_keywords: uppercase,
            select_newline: !no_select_newline,
            logical_operator_newline: !no_logical_newline,
            semicolon_newline: !no_semicolon_newline,
            ..FormatConfig::default()
        };
        token_formatter::TokenFormatter::with_config(sql, tokens, config).format()
    };
    Ok(formatted)
}

fn has_routine_return_cursors(stmt: &ogsql_parser::Statement) -> bool {
    use ogsql_parser::Statement;
    match stmt {
        Statement::CreateProcedure(p) => ogsql_parser::has_return_cursors(&p.parameters, None),
        Statement::CreateFunction(f) => ogsql_parser::has_return_cursors(&f.parameters, f.return_type.as_deref()),
        Statement::CreatePackageBody(pkg) => pkg.items.iter().any(|item| match item {
            ogsql_parser::ast::PackageItem::Procedure(p) => ogsql_parser::has_return_cursors(&p.parameters, None),
            ogsql_parser::ast::PackageItem::Function(f) => {
                ogsql_parser::has_return_cursors(&f.parameters, f.return_type.as_deref())
            }
            _ => false,
        }),
        _ => false,
    }
}

fn compute_routine_analysis(stmt: &ogsql_parser::Statement) -> Option<serde_json::Value> {
    use ogsql_parser::Statement;
    match stmt {
        Statement::CreateProcedure(p) => {
            let block = p.block.as_ref()?;
            let analysis =
                ogsql_parser::analyze_return_cursors(block, &p.parameters, &p.name.join("."), "Procedure", None);
            if analysis.return_cursors.is_empty() {
                return None;
            }
            Some(serde_json::json!(analysis))
        }
        Statement::CreateFunction(f) => {
            let block = f.block.as_ref()?;
            let analysis = ogsql_parser::analyze_return_cursors(
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
        Statement::CreatePackageBody(pkg) => {
            let mut analyses = Vec::new();
            for item in &pkg.items {
                match item {
                    ogsql_parser::ast::PackageItem::Procedure(p) => {
                        if let Some(ref block) = p.block {
                            let analysis = ogsql_parser::analyze_return_cursors(
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
                    ogsql_parser::ast::PackageItem::Function(f) => {
                        if let Some(ref block) = f.block {
                            let analysis = ogsql_parser::analyze_return_cursors(
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

/// Filter parse output to only include the specified stored procedure/function.
/// Also filters errors/warnings to only those belonging to the matched statements.
/// Returns the filtered output, or an error message if the procedure is not found.
fn filter_output_by_procedure(
    output: ogsql_parser::ParseOutput,
    proc_name: &str,
) -> Result<ogsql_parser::ParseOutput, String> {
    use ogsql_parser::ast::PackageItem;
    use ogsql_parser::Statement;

    let proc_lower = proc_name.to_ascii_lowercase();

    fn item_matches(name: &[ogsql_parser::Ident], target: &str) -> bool {
        let full = name.join(".").to_ascii_lowercase();
        let short = name.last().map(|n| n.to_ascii_lowercase()).unwrap_or_default();
        full == target || short == target
    }

    fn matches_package_item(item: &PackageItem, target: &str) -> bool {
        match item {
            PackageItem::Procedure(p) => item_matches(&p.name, target),
            PackageItem::Function(f) => item_matches(&f.name, target),
            _ => false,
        }
    }

    let mut filtered: Vec<ogsql_parser::ast::StatementInfo> = Vec::new();
    let mut valid_line_ranges: Vec<(usize, usize)> = Vec::new();

    for si in output.statements {
        let line_range = (si.start_line, si.end_line);
        match &si.statement {
            Statement::CreateProcedure(s) if item_matches(&s.name, &proc_lower) => {
                valid_line_ranges.push(line_range);
                filtered.push(si);
            }
            Statement::CreateFunction(s) if item_matches(&s.name, &proc_lower) => {
                valid_line_ranges.push(line_range);
                filtered.push(si);
            }
            Statement::CreatePackageBody(spanned) => {
                let pkg_full = spanned.name.join(".").to_ascii_lowercase();
                let pkg_short = spanned.name.last().map(|n| n.to_ascii_lowercase());
                let pkg_matches = pkg_full == proc_lower || pkg_short == Some(proc_lower.clone());

                let matching: Vec<PackageItem> =
                    spanned.items.iter().filter(|item| matches_package_item(item, &proc_lower)).cloned().collect();

                if pkg_matches {
                    valid_line_ranges.push(line_range);
                    filtered.push(si);
                } else if !matching.is_empty() {
                    let item_ranges: Vec<_> = matching
                        .iter()
                        .map(|item| match item {
                            PackageItem::Procedure(p) => (p.start_line, p.end_line),
                            PackageItem::Function(f) => (f.start_line, f.end_line),
                            _ => (0, 0),
                        })
                        .collect();
                    valid_line_ranges.extend(item_ranges);
                    let new_body = ogsql_parser::ast::CreatePackageBodyStatement {
                        replace: spanned.replace,
                        name: spanned.name.clone(),
                        items: matching,
                    };
                    filtered.push(ogsql_parser::ast::StatementInfo {
                        sql_text: si.sql_text.clone(),
                        start_line: si.start_line,
                        start_col: si.start_col,
                        end_line: si.end_line,
                        end_col: si.end_col,
                        statement: Statement::CreatePackageBody(ogsql_parser::ast::Spanned::new(
                            new_body,
                            spanned.span.clone(),
                        )),
                    });
                }
            }
            Statement::CreatePackage(spanned) => {
                let pkg_full = spanned.name.join(".").to_ascii_lowercase();
                let pkg_short = spanned.name.last().map(|n| n.to_ascii_lowercase());
                let pkg_matches = pkg_full == proc_lower || pkg_short == Some(proc_lower.clone());

                let matching: Vec<PackageItem> =
                    spanned.items.iter().filter(|item| matches_package_item(item, &proc_lower)).cloned().collect();

                if pkg_matches {
                    valid_line_ranges.push(line_range);
                    filtered.push(si);
                } else if !matching.is_empty() {
                    let item_ranges: Vec<_> = matching
                        .iter()
                        .map(|item| match item {
                            PackageItem::Procedure(p) => (p.start_line, p.end_line),
                            PackageItem::Function(f) => (f.start_line, f.end_line),
                            _ => (0, 0),
                        })
                        .collect();
                    valid_line_ranges.extend(item_ranges);
                    let new_spec = ogsql_parser::ast::CreatePackageStatement {
                        replace: spanned.replace,
                        name: spanned.name.clone(),
                        authid: spanned.authid.clone(),
                        items: matching,
                    };
                    filtered.push(ogsql_parser::ast::StatementInfo {
                        sql_text: si.sql_text.clone(),
                        start_line: si.start_line,
                        start_col: si.start_col,
                        end_line: si.end_line,
                        end_col: si.end_col,
                        statement: Statement::CreatePackage(ogsql_parser::ast::Spanned::new(
                            new_spec,
                            spanned.span.clone(),
                        )),
                    });
                }
            }
            _ => {}
        }
    }

    if filtered.is_empty() {
        return Err(format!("Procedure '{}' not found in the input file", proc_name));
    }

    // Filter errors: keep those whose line falls within any valid range,
    // and always keep errors with line==0 (tokenizer/general errors).
    let filtered_errors: Vec<_> = output
        .errors
        .into_iter()
        .filter(|err| {
            let line = error_line(err);
            line == 0 || valid_line_ranges.iter().any(|(s, e)| line >= *s && line <= *e)
        })
        .collect();

    Ok(ogsql_parser::ParseOutput { statements: filtered, errors: filtered_errors, comments: output.comments })
}

fn cmd_parse(cli: &Cli, csv: bool, proc_name: Option<&str>, extract_sql: bool) {
    if cli.file.len() <= 1 {
        cmd_parse_single(cli, cli.file.first().map(|s| s.as_str()), csv, proc_name, extract_sql);
    } else {
        cmd_parse_files(cli, csv, extract_sql);
    }
}

fn cmd_parse_single(cli: &Cli, file_path: Option<&str>, csv: bool, proc_name: Option<&str>, extract_sql: bool) {
    let sql = read_input(file_path);
    let output = parse_input(&sql, cli.comments, cli.mybatis);

    let output = match proc_name {
        Some(name) => match filter_output_by_procedure(output, name) {
            Ok(filtered) => filtered,
            Err(msg) => die!("{}", msg),
        },
        None => output,
    };

    if csv || extract_sql {
        let file_name = file_path.unwrap_or("<stdin>");
        if csv {
            output_csv_parse_header();
        }
        let schema = if extract_sql { load_schema_for_extract(cli) } else { None };
        if csv {
            output_csv_parse_rows(
                &output.statements,
                file_name,
                ".",
                &output.errors,
                cli.mybatis,
                extract_sql,
                schema.as_ref(),
            );
        } else {
            output_extract_rows_text(&output.statements, true, schema.as_ref());
        }
    } else if cli.json {
        let stmt_values: Vec<serde_json::Value> = output
            .statements
            .iter()
            .map(|si| {
                let mut obj = serde_json::to_value(si).unwrap();
                if let Some(block) = extract_pl_block(&si.statement) {
                    let report = ogsql_parser::analyze_pl_block(block);
                    if !report.execute_findings.is_empty() {
                        obj.as_object_mut()
                            .unwrap()
                            .insert("dynamic_sql_analysis".to_string(), serde_json::json!(report));
                    }
                    let tx_report = ogsql_parser::analyze_transactions(block);
                    obj.as_object_mut().unwrap().insert(
                        "transaction_analysis".to_string(),
                        serde_json::to_string_pretty(&tx_report).unwrap().into(),
                    );
                    if let Some(ref schema_path) = cli.schema_json {
                        match ogsql_parser::load_schema(schema_path) {
                            Ok(schema) => {
                                let schema_report = ogsql_parser::resolve_schema(block, &schema);
                                obj.as_object_mut()
                                    .unwrap()
                                    .insert("schema_resolution".to_string(), serde_json::json!(schema_report));
                            }
                            Err(e) => eprintln!("Warning: {}", e),
                        }
                    }
                }
                annotate_builtin_functions(&mut obj);
                if has_routine_return_cursors(&si.statement) {
                    if let Some(analysis) = compute_routine_analysis(&si.statement) {
                        obj.as_object_mut().unwrap().insert("routine_analysis".to_string(), analysis);
                    }
                }
                obj
            })
            .collect();

        let all_stmts: Vec<_> = output.statements.iter().map(|si| si.statement.clone()).collect();
        let fingerprints = ogsql_parser::compute_query_fingerprints(&all_stmts);

        let mut out = serde_json::json!({
            "statements": stmt_values,
            "errors": output.errors,
        });
        if !fingerprints.is_empty() {
            out.as_object_mut().unwrap().insert("query_fingerprints".to_string(), serde_json::json!(fingerprints));
        }
        if !output.comments.is_empty() {
            out.as_object_mut().unwrap().insert("comments".to_string(), serde_json::json!(output.comments));
        }
        if cli.lint {
            let config = build_lint_config(cli);
            let lint_warnings = run_lint(
                &output.statements,
                ogsql_parser::linter::Confidence::Full,
                &config,
                load_lint_schema(cli).as_ref(),
            );
            if !lint_warnings.is_empty() {
                out.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
                out.as_object_mut()
                    .unwrap()
                    .insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
            }
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        for stmt in &output.statements {
            println!("{:#?}", stmt);
        }
        if !output.errors.is_empty() {
            let warnings: Vec<_> = output.errors.iter().filter(|e| is_warning(e)).collect();
            let real_errors: Vec<_> = output.errors.iter().filter(|e| !is_warning(e)).collect();
            if !real_errors.is_empty() {
                eprintln!("\n{} error(s):", real_errors.len());
                for e in &real_errors {
                    eprintln!("  {}", e);
                }
                if cli.verbose {
                    write_error_log(&sql, file_path, &output.statements, &real_errors);
                }
            }
            if !warnings.is_empty() {
                eprintln!("\n{} warning(s):", warnings.len());
                for w in &warnings {
                    eprintln!("  {}", w);
                }
            }
        }
        if cli.lint {
            let config = build_lint_config(cli);
            let lint_warnings = run_lint(
                &output.statements,
                ogsql_parser::linter::Confidence::Full,
                &config,
                load_lint_schema(cli).as_ref(),
            );
            if !lint_warnings.is_empty() {
                eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
                format_warnings_text(&lint_warnings);
            }
        }
    }
}

fn cmd_parse_files(cli: &Cli, csv: bool, extract_sql: bool) {
    let mut files_with_errors: HashSet<String> = HashSet::new();
    let mut files_with_warnings: HashSet<String> = HashSet::new();
    let mut stmt_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();

    if csv || extract_sql {
        if csv {
            output_csv_parse_header();
        }
        let schema = if extract_sql { load_schema_for_extract(cli) } else { None };
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let output = parse_input(&sql, cli.comments, cli.mybatis);
            if csv {
                output_csv_parse_rows(
                    &output.statements,
                    file_path,
                    ".",
                    &output.errors,
                    cli.mybatis,
                    extract_sql,
                    schema.as_ref(),
                );
            } else {
                output_extract_rows_text(&output.statements, true, schema.as_ref());
            }
            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_path.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_path.clone());
                } else {
                    files_with_errors.insert(file_path.clone());
                }
            }
        }
    } else if cli.json {
        let mut all_results = Vec::new();
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let output = parse_input(&sql, cli.comments, cli.mybatis);

            let stmt_values: Vec<serde_json::Value> = output
                .statements
                .iter()
                .map(|si| {
                    let mut obj = serde_json::to_value(si).unwrap();
                    if let Some(block) = extract_pl_block(&si.statement) {
                        let report = ogsql_parser::analyze_pl_block(block);
                        if !report.execute_findings.is_empty() {
                            obj.as_object_mut()
                                .unwrap()
                                .insert("dynamic_sql_analysis".to_string(), serde_json::json!(report));
                        }
                        let tx_report = ogsql_parser::analyze_transactions(block);
                        obj.as_object_mut().unwrap().insert(
                            "transaction_analysis".to_string(),
                            serde_json::to_string_pretty(&tx_report).unwrap().into(),
                        );
                        if let Some(ref schema_path) = cli.schema_json {
                            match ogsql_parser::load_schema(schema_path) {
                                Ok(schema) => {
                                    let schema_report = ogsql_parser::resolve_schema(block, &schema);
                                    obj.as_object_mut()
                                        .unwrap()
                                        .insert("schema_resolution".to_string(), serde_json::json!(schema_report));
                                }
                                Err(e) => eprintln!("Warning: {}", e),
                            }
                        }
                    }
                    annotate_builtin_functions(&mut obj);
                    if has_routine_return_cursors(&si.statement) {
                        if let Some(analysis) = compute_routine_analysis(&si.statement) {
                            obj.as_object_mut().unwrap().insert("routine_analysis".to_string(), analysis);
                        }
                    }
                    obj
                })
                .collect();

            let all_stmts: Vec<_> = output.statements.iter().map(|si| si.statement.clone()).collect();
            let fingerprints = ogsql_parser::compute_query_fingerprints(&all_stmts);

            let mut file_result = serde_json::json!({
                "file": file_path,
                "statements": stmt_values,
                "errors": output.errors,
            });
            if !fingerprints.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("query_fingerprints".to_string(), serde_json::json!(fingerprints));
            }
            if !output.comments.is_empty() {
                file_result.as_object_mut().unwrap().insert("comments".to_string(), serde_json::json!(output.comments));
            }
            all_results.push(file_result);

            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_path.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_path.clone());
                } else {
                    files_with_errors.insert(file_path.clone());
                }
            }
        }
        let out = serde_json::json!({
            "files": all_results,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let output = parse_input(&sql, cli.comments, cli.mybatis);

            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_path.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_path.clone());
                } else {
                    files_with_errors.insert(file_path.clone());
                }
            }

            println!("═══ {} ({} statement(s)) ═══", file_path, output.statements.len());
            for stmt in &output.statements {
                println!("{:#?}", stmt);
            }
            if !output.errors.is_empty() {
                let warnings: Vec<_> = output.errors.iter().filter(|e| is_warning(e)).collect();
                let real_errors: Vec<_> = output.errors.iter().filter(|e| !is_warning(e)).collect();
                if !real_errors.is_empty() {
                    eprintln!("[{}] {} error(s):", file_path, real_errors.len());
                    for e in &real_errors {
                        eprintln!("  {}", e);
                    }
                    if cli.verbose {
                        write_error_log(&sql, Some(file_path), &output.statements, &real_errors);
                    }
                }
                if !warnings.is_empty() {
                    eprintln!("[{}] {} warning(s):", file_path, warnings.len());
                    for w in &warnings {
                        eprintln!("  {}", w);
                    }
                }
            }
            println!();
        }
    }
}

fn cmd_parse_dir(
    cli: &Cli,
    dir_paths: &[String],
    exts: &[String],
    csv: bool,
    output_dir: Option<&str>,
    stats: bool,
    extract_sql: bool,
) {
    use std::path::Path;

    for dir_path in dir_paths {
        if !Path::new(dir_path).is_dir() {
            die!("Error: '{}' is not a directory", dir_path);
        }
    }

    if cli.json && output_dir.is_none() {
        die!("Error: --output-dir (-o) is required when using --dir with -j");
    }

    let normalized_exts: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();

    let mut files: Vec<(String, String, std::path::PathBuf)> = Vec::new();
    for dir_path in dir_paths {
        let root = Path::new(dir_path);
        for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !normalized_exts.contains(&file_ext) {
                continue;
            }

            let rel_dir = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| {
                    let s = p.to_str().unwrap_or(".");
                    if s.is_empty() {
                        "."
                    } else {
                        s
                    }
                })
                .unwrap_or(".")
                .to_string();

            let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

            files.push((file_name, rel_dir, path.to_path_buf()));
        }
    }

    files.sort_by(|a, b| a.2.cmp(&b.2));
    files.dedup_by(|a, b| a.2 == b.2);

    if files.is_empty() {
        eprintln!("No files found with extension(s): {}", exts.join(", "));
        return;
    }

    let mut files_with_errors: HashSet<String> = HashSet::new();
    let mut files_with_warnings: HashSet<String> = HashSet::new();
    let mut stmt_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();

    if csv || extract_sql {
        if csv {
            output_csv_parse_header();
        }
        let schema = if extract_sql {
            let mut s = load_schema_for_extract(cli).unwrap_or_default();
            // Auto-collect DDL schema from directory files when --schema-json is not provided
            if cli.schema_json.is_none() {
                for (_, _, abs_path) in &files {
                    let ddl_sql = read_file_path(abs_path);
                    let ddl_output = parse_input(&ddl_sql, false, cli.mybatis);
                    let fs = ogsql_parser::collect_ddl_schema(&ddl_output.statements);
                    s.columns.extend(fs.columns);
                    for (table, idxs) in fs.indexes {
                        s.indexes.entry(table).or_default().extend(idxs);
                    }
                }
            }
            if s.columns.is_empty() {
                None
            } else {
                Some(s)
            }
        } else {
            None
        };
        for (file_name, rel_dir, abs_path) in &files {
            let sql = read_file_path(abs_path);
            let output = parse_input(&sql, cli.comments, cli.mybatis);
            if csv {
                output_csv_parse_rows(
                    &output.statements,
                    file_name,
                    rel_dir,
                    &output.errors,
                    cli.mybatis,
                    extract_sql,
                    schema.as_ref(),
                );
            } else {
                output_extract_rows_text(&output.statements, true, schema.as_ref());
            }
            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_name.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_name.clone());
                } else {
                    files_with_errors.insert(file_name.clone());
                }
            }
            if cli.verbose {
                let real_errors: Vec<_> = output.errors.iter().filter(|e| !is_warning(e)).collect();
                if !real_errors.is_empty() {
                    write_error_log(&sql, Some(file_name), &output.statements, &real_errors);
                }
            }
        }
    } else if cli.json {
        let out_root = Path::new(output_dir.unwrap());
        std::fs::create_dir_all(out_root)
            .unwrap_or_else(|e| die!("Error creating output directory '{}': {}", output_dir.unwrap(), e));

        let mut total_files = 0usize;
        let mut total_stmts = 0usize;
        let mut all_errors: Vec<(String, String, String, Vec<ogsql_parser::ParserError>)> = Vec::new();
        for (file_name, rel_dir, abs_path) in &files {
            let sql = read_file_path(abs_path);
            let output = parse_input(&sql, cli.comments, cli.mybatis);

            let target_dir = out_root.join(rel_dir);
            std::fs::create_dir_all(&target_dir)
                .unwrap_or_else(|e| die!("Error creating directory '{}': {}", target_dir.display(), e));

            let json_name = Path::new(file_name).with_extension("json");
            let target_path = target_dir.join(&json_name);
            let json_str = serde_json::to_string_pretty(&output).unwrap();
            std::fs::write(&target_path, json_str)
                .unwrap_or_else(|e| die!("Error writing '{}': {}", target_path.display(), e));

            total_stmts += output.statements.len();
            total_files += 1;

            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_name.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_name.clone());
                } else {
                    files_with_errors.insert(file_name.clone());
                }
            }

            let real_errors: Vec<_> = output.errors.iter().filter(|e| !is_warning(e)).cloned().collect();
            if !real_errors.is_empty() {
                eprintln!("[{}/{}] {} error(s)", rel_dir, file_name, real_errors.len());
                all_errors.push((file_name.clone(), rel_dir.clone(), sql, real_errors));
            }
        }
        if !all_errors.is_empty() {
            write_dir_error_log(out_root, &all_errors);
        }
        eprintln!("Wrote {} file(s), {} statement(s) to {}", total_files, total_stmts, out_root.display());
        if stats {
            print_parse_stats(
                total_files,
                &files_with_errors,
                &files_with_warnings,
                total_stmts,
                &stmt_counts,
                &error_kinds,
                &warning_kinds,
                "parse -j",
            );
        }
    } else {
        let mut total_files_txt = 0usize;
        let mut total_stmts_txt = 0usize;
        for (file_name, _rel_dir, abs_path) in &files {
            let sql = read_file_path(abs_path);
            let output = parse_input(&sql, cli.comments, cli.mybatis);

            total_files_txt += 1;
            total_stmts_txt += output.statements.len();

            for si in &output.statements {
                *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
            }
            for err in &output.errors {
                let kind = parser_error_kind(err);
                let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
                let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                entry.0 += 1;
                entry.1.insert(file_name.clone());
                if is_warning(err) {
                    files_with_warnings.insert(file_name.clone());
                } else {
                    files_with_errors.insert(file_name.clone());
                }
            }

            println!("═══ {} ({} statement(s)) ═══", file_name, output.statements.len());
            for stmt in &output.statements {
                println!("{:#?}", stmt);
            }
            if !output.errors.is_empty() {
                let warnings: Vec<_> = output.errors.iter().filter(|e| is_warning(e)).collect();
                let real_errors: Vec<_> = output.errors.iter().filter(|e| !is_warning(e)).collect();
                if !real_errors.is_empty() {
                    eprintln!("[{}] {} error(s):", file_name, real_errors.len());
                    for e in &real_errors {
                        eprintln!("  {}", e);
                    }
                    if cli.verbose {
                        write_error_log(&sql, Some(file_name), &output.statements, &real_errors);
                    }
                }
                if !warnings.is_empty() {
                    eprintln!("[{}] {} warning(s):", file_name, warnings.len());
                    for w in &warnings {
                        eprintln!("  {}", w);
                    }
                }
            }
            println!();
        }
        println!("Total: {} statement(s) from {} file(s)", total_stmts_txt, files.len());
        if stats {
            print_parse_stats(
                total_files_txt,
                &files_with_errors,
                &files_with_warnings,
                total_stmts_txt,
                &stmt_counts,
                &error_kinds,
                &warning_kinds,
                "parse",
            );
        }
    }
}

fn read_file_path(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path.display(), e));
    token::decode_sql_file(&bytes).unwrap_or_else(|e| die!("Error decoding {}: {}", path.display(), e)).0
}

fn output_csv_parse_header() {
    print!("\u{FEFF}");
    println!(
        "file,directory,line,type,name,parent,parameters,return_type,sql,error,warning,branch_path,branch_condition"
    );
}

struct ParseCsvRow {
    line: usize,
    end_line: usize,
    stmt_type: String,
    name: String,
    parent: String,
    parameters: String,
    return_type: String,
    sql: String,
    branch_path: String,
    branch_condition: String,
}

fn format_params(params: &[ogsql_parser::ast::RoutineParam]) -> String {
    params
        .iter()
        .map(|p| match &p.mode {
            Some(mode) => format!("{} {} {}", p.name, mode, p.data_type),
            None => format!("{} {}", p.name, p.data_type),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone)]
enum ConcatPart {
    Literal(String),
    Variable(String),
    Unresolved(String),
}

/// Evaluate a PL/pgSQL string-concatenation expression (`||`) into concrete parts.
/// String literals are extracted as their raw content; variables/other expressions
/// are left as named placeholders. This turns `'SELECT * FROM ' || v_table || ' WHERE 1=1'`
/// into `["SELECT * FROM ", "${v_table}", " WHERE 1=1"]`.
fn eval_concat_expr(expr: &ogsql_parser::ast::Expr) -> Vec<ConcatPart> {
    use ogsql_parser::ast::{Expr, Literal};

    match expr {
        Expr::BinaryOp { op, left, right, .. } if op == "||" => {
            let mut parts = eval_concat_expr(left);
            parts.extend(eval_concat_expr(right));
            parts
        }
        Expr::Literal(Literal::String(s)) => vec![ConcatPart::Literal(s.clone())],
        Expr::Literal(Literal::DollarString { body, .. }) => vec![ConcatPart::Literal(body.clone())],
        Expr::Literal(Literal::EscapeString(s)) => vec![ConcatPart::Literal(s.clone())],
        Expr::PlVariable(names) if names.len() == 1 => {
            vec![ConcatPart::Variable(names[0].to_string())]
        }
        Expr::ColumnRef(names) if names.len() == 1 => {
            let name = &names[0];
            if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                vec![ConcatPart::Variable(name.to_string())]
            } else {
                vec![ConcatPart::Literal(name.to_string())]
            }
        }
        Expr::Parenthesized(inner) => eval_concat_expr(inner),
        _ => vec![ConcatPart::Unresolved(format_pl_expr(expr))],
    }
}

/// Join evaluated concat parts into a single SQL string, replacing variables via `replace_fn`.
fn join_concat_parts(
    parts: &[ConcatPart],
    replace_fn: &dyn Fn(&str, &std::collections::HashMap<String, Option<String>>) -> String,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
) -> String {
    join_concat_parts_inner(parts, replace_fn, vars, assigns, &mut std::collections::HashSet::new())
}

fn join_concat_parts_inner(
    parts: &[ConcatPart],
    replace_fn: &dyn Fn(&str, &std::collections::HashMap<String, Option<String>>) -> String,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
    visited: &mut std::collections::HashSet<String>,
) -> String {
    let vars_empty = vars.is_empty();
    let mut sql = String::new();
    for part in parts {
        match part {
            ConcatPart::Literal(s) => sql.push_str(s),
            ConcatPart::Variable(name) => {
                let key = name.to_ascii_lowercase();
                if !visited.contains(&key) {
                    if let Some(traced_parts) = assigns.get(&key) {
                        let has_inner_vars = traced_parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
                        if has_inner_vars {
                            visited.insert(key.clone());
                            sql.push_str(&join_concat_parts_inner(traced_parts, replace_fn, vars, assigns, visited));
                            visited.remove(&key);
                            continue;
                        } else {
                            let flat: String = traced_parts
                                .iter()
                                .map(|p| match p {
                                    ConcatPart::Literal(s) => s.as_str(),
                                    _ => "",
                                })
                                .collect();
                            sql.push_str(&flat);
                            continue;
                        }
                    }
                }
                if vars_empty {
                    sql.push_str(name);
                } else {
                    let single_var: std::collections::HashMap<String, Option<String>> = {
                        let mut m = std::collections::HashMap::new();
                        m.insert(key.clone(), vars.get(&key).cloned().flatten());
                        m
                    };
                    let replaced = replace_fn(name, &single_var);
                    sql.push_str(&replaced);
                }
            }
            ConcatPart::Unresolved(s) => {
                if vars_empty {
                    sql.push_str(s);
                } else {
                    let replaced = replace_fn(s, vars);
                    sql.push_str(&replaced);
                }
            }
        }
    }
    sql
}

fn try_parse_sql_assignment(sql: &str) -> Option<(&str, &str)> {
    let s = sql.trim();
    let eq_pos = s.find(":=").or_else(|| {
        let bytes = s.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'=' && i > 0 {
                let prev = bytes[i - 1];
                if prev != b'!' && prev != b'<' && prev != b'>' && prev != b'=' && prev != b':' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                        continue;
                    }
                    return Some(i);
                }
            }
        }
        None
    })?;
    let (lhs, rhs) = if s.get(eq_pos..eq_pos + 2) == Some(":=") {
        (&s[..eq_pos], &s[eq_pos + 2..])
    } else {
        (&s[..eq_pos], &s[eq_pos + 1..])
    };
    let var = lhs.trim();
    if var.is_empty() || !var.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false) {
        return None;
    }
    Some((var, rhs.trim()))
}

fn extract_execute_sql_content(exec: &ogsql_parser::ast::plpgsql::PlExecuteStmt) -> String {
    use ogsql_parser::ast::{Expr, Literal};

    if let Some(ref parsed) = exec.parsed_query {
        let formatter = ogsql_parser::SqlFormatter::new();
        return formatter.format_statement(parsed);
    }

    match &exec.string_expr {
        Expr::Literal(Literal::String(s)) => return s.clone(),
        Expr::Literal(Literal::DollarString { body, .. }) => return body.clone(),
        Expr::Literal(Literal::EscapeString(s)) => return s.clone(),
        _ => {}
    }

    let parts = eval_concat_expr(&exec.string_expr);
    let all_literal = parts.iter().all(|p| matches!(p, ConcatPart::Literal(_)));
    if all_literal {
        return parts
            .iter()
            .map(|p| match p {
                ConcatPart::Literal(s) => s.as_str(),
                _ => "",
            })
            .collect();
    }
    format_pl_expr(&exec.string_expr)
}

/// Build the expanded SQL for an EXECUTE row in CSV output.
/// Handles string concatenation by flattening `'...' || var || '...'` into
/// a single SQL string with variables replaced as placeholders.
fn build_execute_csv_sql(
    exec: &ogsql_parser::ast::plpgsql::PlExecuteStmt,
    vars: &std::collections::HashMap<String, Option<String>>,
    raw: bool,
) -> String {
    use ogsql_parser::ast::{Expr, Literal};

    if let Some(ref parsed) = exec.parsed_query {
        let formatter = ogsql_parser::SqlFormatter::new();
        let formatted = formatter.format_statement(parsed);
        return if raw {
            replace_pl_vars_in_sql_raw(&formatted, vars)
        } else {
            replace_pl_vars_in_sql(&formatted, vars)
        };
    }

    match &exec.string_expr {
        Expr::Literal(Literal::String(s)) => {
            let sql = s.trim();
            return if raw { replace_pl_vars_in_sql_raw(sql, vars) } else { replace_pl_vars_in_sql(sql, vars) };
        }
        Expr::Literal(Literal::DollarString { body, .. }) => {
            let sql = body.trim();
            return if raw { replace_pl_vars_in_sql_raw(sql, vars) } else { replace_pl_vars_in_sql(sql, vars) };
        }
        Expr::Literal(Literal::EscapeString(s)) => {
            let sql = s.trim();
            return if raw { replace_pl_vars_in_sql_raw(sql, vars) } else { replace_pl_vars_in_sql(sql, vars) };
        }
        _ => {}
    }

    let empty_assigns: std::collections::HashMap<String, Vec<ConcatPart>> = std::collections::HashMap::new();
    let parts = eval_concat_expr(&exec.string_expr);
    let has_vars = parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
    if has_vars {
        let replace_fn: &dyn Fn(&str, &std::collections::HashMap<String, Option<String>>) -> String =
            if raw { &replace_pl_vars_in_sql_raw } else { &replace_pl_vars_in_sql };
        return join_concat_parts(&parts, replace_fn, vars, &empty_assigns);
    }

    let plain = extract_execute_sql_content(exec);
    if raw {
        replace_pl_vars_in_sql_raw(&plain, vars)
    } else {
        replace_pl_vars_in_sql(&plain, vars)
    }
}

fn build_execute_csv_sql_with_trace(
    exec: &ogsql_parser::ast::plpgsql::PlExecuteStmt,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
) -> String {
    use ogsql_parser::ast::{Expr, Literal};

    if let Some(ref parsed) = exec.parsed_query {
        let formatter = ogsql_parser::SqlFormatter::new();
        let formatted = formatter.format_statement(parsed);
        return replace_pl_vars_in_sql_raw(&formatted, vars);
    }

    match &exec.string_expr {
        Expr::Literal(Literal::String(s)) => {
            return replace_pl_vars_in_sql_raw(s.trim(), vars);
        }
        Expr::Literal(Literal::DollarString { body, .. }) => {
            return replace_pl_vars_in_sql_raw(body.trim(), vars);
        }
        Expr::Literal(Literal::EscapeString(s)) => {
            return replace_pl_vars_in_sql_raw(s.trim(), vars);
        }
        Expr::ColumnRef(names) | Expr::PlVariable(names) if names.len() == 1 => {
            let var_name = &names[0];
            if let Some(traced_parts) = assigns.get(&var_name.to_ascii_lowercase()) {
                let has_vars = traced_parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
                if has_vars {
                    return join_concat_parts(traced_parts, &replace_pl_vars_in_sql_raw, vars, assigns);
                } else {
                    let flat: String = traced_parts
                        .iter()
                        .map(|p| match p {
                            ConcatPart::Literal(s) => s.as_str(),
                            _ => "",
                        })
                        .collect();
                    return replace_pl_vars_in_sql_raw(flat.trim(), vars);
                }
            }
        }
        _ => {}
    }

    let parts = eval_concat_expr(&exec.string_expr);
    let has_vars = parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
    if has_vars {
        return join_concat_parts(&parts, &replace_pl_vars_in_sql_raw, vars, assigns);
    }

    let plain = extract_execute_sql_content(exec);
    replace_pl_vars_in_sql_raw(&plain, vars)
}

fn format_pl_expr(expr: &ogsql_parser::ast::Expr) -> String {
    use ogsql_parser::ast::{Expr, Literal};

    match expr {
        Expr::Literal(Literal::String(s)) => format!("'{}'", s),
        Expr::Literal(Literal::DollarString { tag: None, body }) => format!("$${}$$", body),
        Expr::Literal(Literal::DollarString { tag: Some(t), body }) => format!("${}${}", t, body),
        Expr::Literal(Literal::EscapeString(s)) => format!("E'{}'", s),
        Expr::Literal(Literal::Integer(n)) => n.to_string(),
        Expr::Literal(Literal::Float(s)) => s.clone(),
        Expr::Literal(Literal::Boolean(b)) => b.to_string(),
        Expr::Literal(Literal::Null) => "NULL".into(),
        Expr::Literal(Literal::BitString(s)) => format!("B'{}'", s),
        Expr::Literal(Literal::HexString(s)) => format!("X'{}'", s),
        Expr::Literal(Literal::NationalString(s)) => format!("N'{}'", s),
        Expr::ColumnRef(names) | Expr::PlVariable(names) => names.join("."),
        Expr::BinaryOp { op, left, right, .. } => {
            format!("{} {} {}", format_pl_expr(left), op, format_pl_expr(right))
        }
        Expr::Parenthesized(inner) => format!("({})", format_pl_expr(inner)),
        Expr::FunctionCall { name, args, .. } => {
            let formatted_args: Vec<String> = args.iter().map(format_pl_expr).collect();
            format!("{}({})", name.join("."), formatted_args.join(", "))
        }
        _ => format!("{:?}", expr),
    }
}

fn extract_block_sql(block: &ogsql_parser::ast::plpgsql::PlBlock) -> String {
    use ogsql_parser::ast::plpgsql::PlStatement;
    let mut parts: Vec<String> = Vec::new();
    for stmt in &block.body {
        match stmt {
            PlStatement::SqlStatement { sql_text, .. } => {
                if !sql_text.is_empty() {
                    parts.push(sql_text.clone());
                }
            }
            PlStatement::Sql(text) => {
                if !text.is_empty() {
                    parts.push(text.clone());
                }
            }
            PlStatement::Execute(spanned) => {
                let sql = extract_execute_sql_content(&spanned.node);
                if !sql.is_empty() {
                    parts.push(sql);
                }
            }
            PlStatement::Perform { query, .. } => {
                parts.push(format!("PERFORM {}", query));
            }
            _ => {}
        }
    }
    parts.join("\\n")
}

fn format_using_args_pl(args: &[ogsql_parser::ast::plpgsql::PlUsingArg]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let mode_prefix = match a.mode {
                ogsql_parser::ast::plpgsql::PlUsingMode::In => "",
                ogsql_parser::ast::plpgsql::PlUsingMode::Out => "OUT ",
                ogsql_parser::ast::plpgsql::PlUsingMode::InOut => "INOUT ",
            };
            format!("{}{}", mode_prefix, format_pl_expr(&a.argument))
        })
        .collect();
    format!("USING {}", parts.join(", "))
}

fn format_using_args_exprs(args: &[ogsql_parser::ast::Expr]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = args.iter().map(format_pl_expr).collect();
    parts.join(", ")
}

fn build_dynamic_sql_from_expr(
    expr: &ogsql_parser::ast::Expr,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
) -> String {
    use ogsql_parser::ast::{Expr, Literal};

    match expr {
        Expr::Literal(Literal::String(s)) => replace_pl_vars_in_sql(s.trim(), vars),
        Expr::Literal(Literal::DollarString { body, .. }) => replace_pl_vars_in_sql(body.trim(), vars),
        Expr::Literal(Literal::EscapeString(s)) => replace_pl_vars_in_sql(s.trim(), vars),
        Expr::ColumnRef(names) | Expr::PlVariable(names) if names.len() == 1 => {
            let var_name = &names[0];
            if let Some(traced_parts) = assigns.get(&var_name.to_ascii_lowercase()) {
                let has_vars = traced_parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
                if has_vars {
                    return join_concat_parts(traced_parts, &replace_pl_vars_in_sql_raw, vars, assigns);
                } else {
                    let flat: String = traced_parts
                        .iter()
                        .map(|p| match p {
                            ConcatPart::Literal(s) => s.as_str(),
                            _ => "",
                        })
                        .collect();
                    return replace_pl_vars_in_sql_raw(flat.trim(), vars);
                }
            }
            format_pl_expr(expr)
        }
        Expr::BinaryOp { op, .. } if op == "||" => {
            let parts = eval_concat_expr(expr);
            join_concat_parts(&parts, &replace_pl_vars_in_sql_raw, vars, assigns)
        }
        _ => format_pl_expr(expr),
    }
}

fn resolve_for_query_text(
    query: &str,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
) -> (String, String) {
    let q = query.trim();
    if q.is_empty() {
        return ("Embedded/Select".into(), String::new());
    }

    let is_execute_prefix = q.to_ascii_lowercase().starts_with("execute ");
    let looks_like_static_sql = ["select ", "insert ", "update ", "delete ", "merge ", "with "]
        .iter()
        .any(|kw| q.to_ascii_lowercase().starts_with(kw));

    if is_execute_prefix {
        let after_execute = q[8..].trim_start();
        let after_execute = if after_execute.to_ascii_lowercase().starts_with("immediate ") {
            after_execute[9..].trim_start()
        } else {
            after_execute
        };
        let inner = strip_trailing_using(after_execute);
        let sql = resolve_dynamic_query_text(inner.trim(), vars, assigns);
        let using_part = extract_trailing_using_text(after_execute);
        let full_sql = if using_part.is_empty() {
            sql
        } else {
            format!("{}\nUSING {}", sql, replace_pl_vars_in_sql(&using_part, vars))
        };
        return ("Embedded/Execute".into(), full_sql);
    }

    if looks_like_static_sql {
        let (sql_part, using_part) = split_query_and_using(q);
        let sql = replace_pl_vars_in_sql(sql_part.trim(), vars);
        let full_sql = if using_part.is_empty() {
            sql
        } else {
            format!("{}\nUSING {}", sql, replace_pl_vars_in_sql(&using_part, vars))
        };
        return ("Embedded/Select".into(), full_sql);
    }

    let inner = strip_trailing_using(q);
    let using_part = extract_trailing_using_text(q);
    let sql = resolve_dynamic_query_text(inner.trim(), vars, assigns);
    let full_sql = if using_part.is_empty() {
        sql
    } else {
        format!("{}\nUSING {}", sql, replace_pl_vars_in_sql(&using_part, vars))
    };
    ("Embedded/Execute".into(), full_sql)
}

fn split_query_and_using(text: &str) -> (String, String) {
    if let Some(pos) = find_using_keyword_pos(text) {
        (text[..pos].to_string(), text[pos + 5..].trim().to_string())
    } else {
        (text.to_string(), String::new())
    }
}

fn strip_trailing_using(text: &str) -> String {
    if let Some(pos) = find_using_keyword_pos(text) {
        text[..pos].to_string()
    } else {
        text.to_string()
    }
}

fn extract_trailing_using_text(text: &str) -> String {
    if let Some(pos) = find_using_keyword_pos(text) {
        text[pos + 5..].trim().to_string()
    } else {
        String::new()
    }
}

fn find_using_keyword_pos(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let mut in_string_single = false;
    let mut in_string_double = false;
    for (i, c) in lower.char_indices() {
        if c == '\'' && !in_string_double {
            in_string_single = !in_string_single;
            continue;
        }
        if c == '"' && !in_string_single {
            in_string_double = !in_string_double;
            continue;
        }
        if !in_string_single && !in_string_double && lower[i..].starts_with("using ") {
            return Some(i);
        }
    }
    None
}

fn resolve_dynamic_query_text(
    text: &str,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &std::collections::HashMap<String, Vec<ConcatPart>>,
) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.contains("||") {
        let (stmts, _) = ogsql_parser::parser::Parser::parse_sql(&format!("SELECT {}", t));
        if let Some(si) = stmts.first() {
            if let ogsql_parser::Statement::Select(ref sel) = si.statement {
                if let Some(ref first_item) = sel.node.targets.first() {
                    if let ogsql_parser::ast::SelectTarget::Expr(expr, _) = first_item {
                        let parts = eval_concat_expr(expr);
                        return join_concat_parts(&parts, &replace_pl_vars_in_sql_raw, vars, assigns);
                    }
                }
            }
        }
    }
    if let Some(rest) = t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return replace_pl_vars_in_sql(rest.trim(), vars);
    }
    if let Some(rest) = t.strip_prefix("E'").and_then(|s| s.strip_suffix('\'')) {
        return replace_pl_vars_in_sql(rest.trim(), vars);
    }
    if t.starts_with("$$") {
        if let Some(rest) = t.strip_prefix("$$").and_then(|s| s.strip_suffix("$$")) {
            return replace_pl_vars_in_sql(rest.trim(), vars);
        }
    }
    if let Some(traced_parts) = assigns.get(&t.to_ascii_lowercase()) {
        let has_vars = traced_parts.iter().any(|p| !matches!(p, ConcatPart::Literal(_)));
        if has_vars {
            return join_concat_parts(traced_parts, &replace_pl_vars_in_sql_raw, vars, assigns);
        } else {
            let flat: String = traced_parts
                .iter()
                .map(|p| match p {
                    ConcatPart::Literal(s) => s.as_str(),
                    _ => "",
                })
                .collect();
            return replace_pl_vars_in_sql_raw(flat.trim(), vars);
        }
    }
    replace_pl_vars_in_sql(t, vars)
}

fn join_branch(parent: &str, segment: &str) -> String {
    if parent.is_empty() {
        segment.to_string()
    } else {
        format!("{}.{}", parent, segment)
    }
}

fn extract_out_cursor_set(params: &[ogsql_parser::ast::RoutineParam]) -> std::collections::HashSet<String> {
    params
        .iter()
        .filter(|p| {
            let is_out = p.mode.as_deref() == Some("OUT")
                || p.mode.as_deref() == Some("INOUT")
                || p.mode.as_deref() == Some("IN OUT");
            let is_cursor = p.data_type.to_uppercase().contains("REFCURSOR");
            is_out && is_cursor
        })
        .map(|p| p.name.to_lowercase())
        .collect()
}

/// Recursively collect SQL rows from a PL/pgSQL block into individual CSV rows.
fn collect_block_sql_rows(
    block: &ogsql_parser::ast::plpgsql::PlBlock,
    parent_name: &str,
    fallback_line: usize,
    vars: &std::collections::HashMap<String, Option<String>>,
    out_cursors: &std::collections::HashSet<String>,
    all_opens_are_returns: bool,
) -> Vec<ParseCsvRow> {
    use ogsql_parser::ast::plpgsql::PlDeclaration;
    let mut rows = Vec::new();
    let mut assigns: std::collections::HashMap<String, Vec<ConcatPart>> = std::collections::HashMap::new();
    for decl in &block.declarations {
        if let PlDeclaration::Variable(ref v) = decl {
            if let Some(ref expr) = v.default {
                let parts = eval_concat_expr(expr);
                if !parts.is_empty() {
                    assigns.insert(v.name.to_ascii_lowercase(), parts);
                }
            }
        }
    }
    for stmt in &block.body {
        collect_pl_stmt_rows(
            stmt,
            parent_name,
            fallback_line,
            vars,
            &mut assigns,
            &mut rows,
            out_cursors,
            all_opens_are_returns,
            "",
            "",
        );
    }
    if let Some(ref exc) = block.exception_block {
        for handler in &exc.handlers {
            for stmt in &handler.statements {
                collect_pl_stmt_rows(
                    stmt,
                    parent_name,
                    fallback_line,
                    vars,
                    &mut assigns,
                    &mut rows,
                    out_cursors,
                    all_opens_are_returns,
                    "",
                    "",
                );
            }
        }
    }
    rows
}

/// Extract the start line from a Spanned PlStatement variant, if available.
fn spanned_line(span: &Option<ogsql_parser::ast::SourceSpan>) -> usize {
    span.as_ref().map_or(0, |s| s.start.line)
}

fn spanned_end_line(span: &Option<ogsql_parser::ast::SourceSpan>, fallback: usize) -> usize {
    span.as_ref().map_or(fallback, |s| s.end.line.max(fallback))
}

fn sql_end_line(start_line: usize, sql: &str) -> usize {
    if sql.is_empty() {
        start_line
    } else {
        start_line + sql.lines().count().saturating_sub(1)
    }
}

/// Derive a descriptive type string and target name from an embedded SQL Statement.
fn sql_statement_type_and_name(stmt: &ogsql_parser::Statement) -> (String, String) {
    use ogsql_parser::Statement;
    match stmt {
        Statement::Select(s) => (
            "SqlStatement/Select".into(),
            s.from
                .first()
                .and_then(|f| {
                    if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                        Some(name.join("."))
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        ),
        Statement::Insert(s) => ("SqlStatement/Insert".into(), s.table.join(".")),
        Statement::Update(s) => (
            "SqlStatement/Update".into(),
            s.tables
                .first()
                .and_then(|f| {
                    if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                        Some(name.join("."))
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        ),
        Statement::Delete(s) => (
            "SqlStatement/Delete".into(),
            s.tables
                .first()
                .and_then(|f| {
                    if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                        Some(name.join("."))
                    } else {
                        None
                    }
                })
                .unwrap_or_default(),
        ),
        Statement::Merge(s) => (
            "SqlStatement/Merge".into(),
            match &s.target {
                ogsql_parser::ast::TableRef::Table { name, .. } => name.join("."),
                _ => String::new(),
            },
        ),
        Statement::CreateTable(s) => ("SqlStatement/CreateTable".into(), s.name.join(".")),
        _ => {
            let type_name = serde_json::to_value(stmt)
                .ok()
                .and_then(|v| if let serde_json::Value::Object(map) = v { map.keys().next().cloned() } else { None })
                .unwrap_or_else(|| "Unknown".to_string());
            (format!("SqlStatement/{}", type_name), String::new())
        }
    }
}

/// Recursively walk a single PlStatement, collecting SQL-bearing nodes as CSV rows.
fn collect_pl_stmt_rows(
    pl_stmt: &ogsql_parser::ast::plpgsql::PlStatement,
    parent_name: &str,
    fallback_line: usize,
    vars: &std::collections::HashMap<String, Option<String>>,
    assigns: &mut std::collections::HashMap<String, Vec<ConcatPart>>,
    rows: &mut Vec<ParseCsvRow>,
    out_cursors: &std::collections::HashSet<String>,
    all_opens_are_returns: bool,
    branch_path: &str,
    branch_condition: &str,
) {
    use ogsql_parser::ast::plpgsql::PlStatement;

    match pl_stmt {
        PlStatement::Assignment { target, expression } => {
            let target_name: String = match target {
                ogsql_parser::ast::Expr::PlVariable(n) | ogsql_parser::ast::Expr::ColumnRef(n) => {
                    n.last().map(|i| i.to_string()).unwrap_or_default()
                }
                _ => String::new(),
            };
            if !target_name.is_empty() {
                let key = target_name.to_ascii_lowercase();
                let mut parts = eval_concat_expr(expression);
                if let Some(prev) = assigns.get(&key).cloned() {
                    let mut resolved = Vec::with_capacity(parts.len());
                    for part in &parts {
                        match part {
                            ConcatPart::Variable(name) if name.to_ascii_lowercase() == key => {
                                resolved.extend(prev.iter().cloned());
                            }
                            other => resolved.push(other.clone()),
                        }
                    }
                    parts = resolved;
                }
                assigns.insert(key, parts);
            }
        }

        PlStatement::SqlStatement { span, sql_text, statement } => {
            let (stmt_type, name) = sql_statement_type_and_name(statement);
            let sql = replace_pl_vars_in_sql(sql_text.trim(), vars);
            let line = span.as_ref().map(|s| s.start.line).unwrap_or(fallback_line).max(1);
            rows.push(ParseCsvRow {
                line,
                end_line: spanned_end_line(span, line),
                stmt_type,
                name,
                parent: parent_name.to_string(),
                parameters: String::new(),
                return_type: String::new(),
                sql,
                branch_path: branch_path.to_string(),
                branch_condition: branch_condition.to_string(),
            });
        }
        PlStatement::Sql(text) => {
            if !text.is_empty() {
                if let Some((var, rhs)) = try_parse_sql_assignment(text) {
                    let rhs_sql = rhs.trim();
                    if rhs_sql.starts_with('"') && rhs_sql.ends_with('"') {
                        let inner = &rhs_sql[1..rhs_sql.len() - 1];
                        assigns.insert(var.to_ascii_lowercase(), vec![ConcatPart::Literal(inner.to_string())]);
                    } else if rhs_sql.starts_with('\'') && rhs_sql.ends_with('\'') {
                        let inner = &rhs_sql[1..rhs_sql.len() - 1];
                        assigns.insert(var.to_ascii_lowercase(), vec![ConcatPart::Literal(inner.to_string())]);
                    }
                }
                if is_sql_statement(text) {
                    let sql = replace_pl_vars_in_sql(text.trim(), vars);
                    rows.push(ParseCsvRow {
                        line: fallback_line,
                        end_line: sql_end_line(fallback_line, &sql),
                        stmt_type: "Sql".into(),
                        name: String::new(),
                        parent: parent_name.to_string(),
                        parameters: String::new(),
                        return_type: String::new(),
                        sql,
                        branch_path: branch_path.to_string(),
                        branch_condition: branch_condition.to_string(),
                    });
                }
            }
        }
        PlStatement::Execute(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let sql = build_execute_csv_sql_with_trace(&spanned.node, vars, assigns);
            rows.push(ParseCsvRow {
                line,
                end_line: sql_end_line(line, &sql),
                stmt_type: "Execute".into(),
                name: String::new(),
                parent: parent_name.to_string(),
                parameters: String::new(),
                return_type: String::new(),
                sql,
                branch_path: branch_path.to_string(),
                branch_condition: branch_condition.to_string(),
            });
        }
        PlStatement::Perform { span, query, .. } => {
            let sql = replace_pl_vars_in_sql(&format!("PERFORM {}", query), vars);
            let line = span.as_ref().map(|s| s.start.line).unwrap_or(fallback_line).max(1);
            rows.push(ParseCsvRow {
                line,
                end_line: spanned_end_line(span, line),
                stmt_type: "Perform".into(),
                name: String::new(),
                parent: parent_name.to_string(),
                parameters: String::new(),
                return_type: String::new(),
                sql,
                branch_path: branch_path.to_string(),
                branch_condition: branch_condition.to_string(),
            });
        }
        PlStatement::Block(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let bp = join_branch(branch_path, "Block");
            for s in &spanned.node.body {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &bp,
                    branch_condition,
                );
            }
            if let Some(ref exc) = spanned.node.exception_block {
                for handler in &exc.handlers {
                    for s in &handler.statements {
                        collect_pl_stmt_rows(
                            s,
                            parent_name,
                            line,
                            vars,
                            assigns,
                            rows,
                            out_cursors,
                            all_opens_are_returns,
                            &bp,
                            branch_condition,
                        );
                    }
                }
            }
        }
        PlStatement::If(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let if_stmt = &spanned.node;
            let cond_str = format_pl_expr(&if_stmt.condition);
            for s in &if_stmt.then_stmts {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &join_branch(branch_path, "IF.then"),
                    &cond_str,
                );
            }
            for (i, elsif) in if_stmt.elsifs.iter().enumerate() {
                let elsif_cond = format_pl_expr(&elsif.condition);
                for s in &elsif.stmts {
                    collect_pl_stmt_rows(
                        s,
                        parent_name,
                        line,
                        vars,
                        assigns,
                        rows,
                        out_cursors,
                        all_opens_are_returns,
                        &join_branch(branch_path, &format!("IF.elsif#{}.then", i + 1)),
                        &elsif_cond,
                    );
                }
            }
            for s in &if_stmt.else_stmts {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &join_branch(branch_path, "IF.else"),
                    &cond_str,
                );
            }
        }
        PlStatement::Case(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let case_stmt = &spanned.node;
            for (i, when) in case_stmt.whens.iter().enumerate() {
                let when_cond = format_pl_expr(&when.condition);
                for s in &when.stmts {
                    collect_pl_stmt_rows(
                        s,
                        parent_name,
                        line,
                        vars,
                        assigns,
                        rows,
                        out_cursors,
                        all_opens_are_returns,
                        &join_branch(branch_path, &format!("CASE.when#{}", i + 1)),
                        &when_cond,
                    );
                }
            }
            for s in &case_stmt.else_stmts {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &join_branch(branch_path, "CASE.else"),
                    "",
                );
            }
        }
        PlStatement::Loop(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let bp = join_branch(branch_path, "LOOP");
            for s in &spanned.node.body {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &bp,
                    branch_condition,
                );
            }
        }
        PlStatement::While(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let while_cond = format_pl_expr(&spanned.node.condition);
            let bp = join_branch(branch_path, "WHILE");
            for s in &spanned.node.body {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &bp,
                    &while_cond,
                );
            }
        }
        PlStatement::For(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let for_stmt = &spanned.node;
            let bp = join_branch(branch_path, "FOR");
            match &for_stmt.kind {
                ogsql_parser::ast::plpgsql::PlForKind::Query { query, parsed_query, using_args: _ } => {
                    rows.push(ParseCsvRow {
                        line,
                        end_line: line,
                        stmt_type: "ForQuery".into(),
                        name: for_stmt.variable.clone(),
                        parent: parent_name.to_string(),
                        parameters: String::new(),
                        return_type: String::new(),
                        sql: String::new(),
                        branch_path: bp.clone(),
                        branch_condition: branch_condition.to_string(),
                    });
                    if let Some(ref stmt) = parsed_query {
                        let formatter = ogsql_parser::SqlFormatter::new();
                        let formatted = formatter.format_statement(stmt);
                        let sql = replace_pl_vars_in_sql(&formatted, vars);
                        rows.push(ParseCsvRow {
                            line,
                            end_line: sql_end_line(line, &sql),
                            stmt_type: "Embedded/Select".into(),
                            name: String::new(),
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: String::new(),
                            sql,
                            branch_path: bp.clone(),
                            branch_condition: branch_condition.to_string(),
                        });
                    } else {
                        let (embedded_type, sql) = resolve_for_query_text(query, vars, assigns);
                        rows.push(ParseCsvRow {
                            line,
                            end_line: sql_end_line(line, &sql),
                            stmt_type: embedded_type,
                            name: String::new(),
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: String::new(),
                            sql,
                            branch_path: bp.clone(),
                            branch_condition: branch_condition.to_string(),
                        });
                    }
                }
                ogsql_parser::ast::plpgsql::PlForKind::Cursor { cursor_name: _, arguments } => {
                    let args_str: Vec<String> = arguments.iter().map(format_pl_expr).collect();
                    rows.push(ParseCsvRow {
                        line,
                        end_line: line,
                        stmt_type: "ForCursor".into(),
                        name: for_stmt.variable.clone(),
                        parent: parent_name.to_string(),
                        parameters: args_str.join(", "),
                        return_type: String::new(),
                        sql: String::new(),
                        branch_path: bp.clone(),
                        branch_condition: branch_condition.to_string(),
                    });
                }
                _ => {}
            }
            for s in &for_stmt.body {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &bp,
                    branch_condition,
                );
            }
        }
        PlStatement::ForEach(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let bp = join_branch(branch_path, "FOREACH");
            for s in &spanned.node.body {
                collect_pl_stmt_rows(
                    s,
                    parent_name,
                    line,
                    vars,
                    assigns,
                    rows,
                    out_cursors,
                    all_opens_are_returns,
                    &bp,
                    branch_condition,
                );
            }
        }
        PlStatement::ForAll(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let body = &spanned.node.body;
            if !body.is_empty() {
                rows.push(ParseCsvRow {
                    line,
                    end_line: line,
                    stmt_type: "ForAll".into(),
                    name: String::new(),
                    parent: parent_name.to_string(),
                    parameters: String::new(),
                    return_type: String::new(),
                    sql: body.trim().to_string(),
                    branch_path: branch_path.to_string(),
                    branch_condition: branch_condition.to_string(),
                });
            }
        }
        PlStatement::Open(spanned) => {
            let line = spanned_line(&spanned.span).max(fallback_line);
            let end_line = spanned_end_line(&spanned.span, line);
            let open_stmt = &spanned.node;
            let cursor_name = format_pl_expr(&open_stmt.cursor);
            let is_out = out_cursors.contains(&cursor_name.to_lowercase()) || all_opens_are_returns;
            match &open_stmt.kind {
                ogsql_parser::ast::plpgsql::PlOpenKind::ForQuery { scroll: _, query, parsed_query } => {
                    if is_out {
                        let sql = if let Some(ref stmt) = parsed_query {
                            let formatter = ogsql_parser::SqlFormatter::new();
                            let formatted = formatter.format_statement(stmt);
                            replace_pl_vars_in_sql(&formatted, vars)
                        } else if query.trim().is_empty() {
                            String::new()
                        } else {
                            let (_, resolved) = resolve_for_query_text(query, vars, assigns);
                            resolved
                        };
                        rows.push(ParseCsvRow {
                            line,
                            end_line,
                            stmt_type: "ReturnCursorSQL".into(),
                            name: cursor_name,
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: "REFCURSOR".into(),
                            sql,
                            branch_path: branch_path.to_string(),
                            branch_condition: branch_condition.to_string(),
                        });
                    } else {
                        rows.push(ParseCsvRow {
                            line,
                            end_line: line,
                            stmt_type: "Open/ForQuery".into(),
                            name: cursor_name,
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: String::new(),
                            sql: String::new(),
                            branch_path: branch_path.to_string(),
                            branch_condition: branch_condition.to_string(),
                        });
                        if !query.trim().is_empty() {
                            let sql = replace_pl_vars_in_sql(query.trim(), vars);
                            rows.push(ParseCsvRow {
                                line,
                                end_line,
                                stmt_type: "Embedded/Select".into(),
                                name: String::new(),
                                parent: parent_name.to_string(),
                                parameters: String::new(),
                                return_type: String::new(),
                                sql,
                                branch_path: branch_path.to_string(),
                                branch_condition: branch_condition.to_string(),
                            });
                        } else {
                            let (embedded_type, sql) = resolve_for_query_text(query, vars, assigns);
                            rows.push(ParseCsvRow {
                                line,
                                end_line,
                                stmt_type: embedded_type,
                                name: String::new(),
                                parent: parent_name.to_string(),
                                parameters: String::new(),
                                return_type: String::new(),
                                sql,
                                branch_path: branch_path.to_string(),
                                branch_condition: branch_condition.to_string(),
                            });
                        }
                    }
                }
                ogsql_parser::ast::plpgsql::PlOpenKind::ForExecute { query, using_args } => {
                    let dynamic_sql = build_dynamic_sql_from_expr(query, vars, assigns);
                    let using_suffix = format_using_args_exprs(using_args);
                    let full_sql = if using_suffix.is_empty() {
                        dynamic_sql
                    } else {
                        format!("{}\nUSING {}", dynamic_sql, using_suffix)
                    };
                    if is_out {
                        rows.push(ParseCsvRow {
                            line,
                            end_line,
                            stmt_type: "ReturnCursorSQL".into(),
                            name: cursor_name,
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: "REFCURSOR".into(),
                            sql: full_sql,
                            branch_path: branch_path.to_string(),
                            branch_condition: branch_condition.to_string(),
                        });
                    } else {
                        rows.push(ParseCsvRow {
                            line,
                            end_line: line,
                            stmt_type: "Open/ForExecute".into(),
                            name: cursor_name,
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: String::new(),
                            sql: String::new(),
                            branch_path: branch_path.to_string(),
                            branch_condition: branch_condition.to_string(),
                        });
                        rows.push(ParseCsvRow {
                            line,
                            end_line,
                            stmt_type: "Embedded/Execute".into(),
                            name: String::new(),
                            parent: parent_name.to_string(),
                            parameters: String::new(),
                            return_type: String::new(),
                            sql: full_sql,
                            branch_path: branch_path.to_string(),
                            branch_condition: branch_condition.to_string(),
                        });
                    }
                }
                ogsql_parser::ast::plpgsql::PlOpenKind::ForUsing { expressions } => {
                    rows.push(ParseCsvRow {
                        line,
                        end_line: line,
                        stmt_type: "Open/ForUsing".into(),
                        name: cursor_name,
                        parent: parent_name.to_string(),
                        parameters: String::new(),
                        return_type: String::new(),
                        sql: String::new(),
                        branch_path: branch_path.to_string(),
                        branch_condition: branch_condition.to_string(),
                    });
                    let exprs: Vec<String> = expressions.iter().map(format_pl_expr).collect();
                    rows.push(ParseCsvRow {
                        line,
                        end_line: line,
                        stmt_type: "Embedded/Execute".into(),
                        name: String::new(),
                        parent: parent_name.to_string(),
                        parameters: String::new(),
                        return_type: String::new(),
                        sql: exprs.join(", "),
                        branch_path: branch_path.to_string(),
                        branch_condition: branch_condition.to_string(),
                    });
                }
                ogsql_parser::ast::plpgsql::PlOpenKind::Simple { arguments } => {
                    let args_str: Vec<String> = arguments.iter().map(format_pl_expr).collect();
                    rows.push(ParseCsvRow {
                        line,
                        end_line: line,
                        stmt_type: "Open/Simple".into(),
                        name: cursor_name,
                        parent: parent_name.to_string(),
                        parameters: args_str.join(", "),
                        return_type: String::new(),
                        sql: String::new(),
                        branch_path: branch_path.to_string(),
                        branch_condition: branch_condition.to_string(),
                    });
                }
            }
        }
        _ => {}
    }
}

fn collect_package_item_vars(
    items: &[ogsql_parser::ast::PackageItem],
) -> std::collections::HashMap<String, Option<String>> {
    let mut vars = std::collections::HashMap::new();
    for item in items {
        match item {
            ogsql_parser::ast::PackageItem::Variable(v) => {
                let t = pl_data_type_to_string(&v.data_type);
                vars.insert(v.name.to_ascii_lowercase(), t);
            }
            ogsql_parser::ast::PackageItem::Cursor(c) => {
                vars.insert(c.name.to_ascii_lowercase(), None);
            }
            ogsql_parser::ast::PackageItem::Type(t) => {
                let name = match t {
                    ogsql_parser::ast::plpgsql::PlTypeDecl::Record { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::TableOf { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::VarrayOf { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::RefCursor { name } => name,
                };
                vars.insert(name.to_ascii_lowercase(), None);
            }
            _ => {}
        }
    }
    vars
}

/// Collect package-level variable declarations from `CreatePackage` (spec) statements
/// so that `CreatePackageBody` procedures can reference them.
fn collect_pkg_spec_vars_from_stmts(
    stmts: &[ogsql_parser::StatementInfo],
) -> std::collections::HashMap<String, Option<String>> {
    let mut vars = std::collections::HashMap::new();
    for si in stmts {
        if let ogsql_parser::Statement::CreatePackage(s) = &si.statement {
            let spec_vars = collect_package_item_vars(&s.items);
            for (k, v) in spec_vars {
                vars.entry(k).or_insert(v);
            }
        }
    }
    vars
}

fn flatten_statement(
    si: &ogsql_parser::StatementInfo,
    mybatis: bool,
    extract_sql: bool,
    schema: Option<&ogsql_parser::FullSchema>,
    pkg_spec_vars: &std::collections::HashMap<String, Option<String>>,
) -> Vec<ParseCsvRow> {
    use ogsql_parser::Statement;
    let mut rows = Vec::new();
    let do_vars = mybatis || extract_sql;

    match &si.statement {
        Statement::CreatePackageBody(s) => {
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "CreatePackageBody".into(),
                    name: s.name.join("."),
                    parent: String::new(),
                    parameters: String::new(),
                    return_type: String::new(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            // Collect package-level variables defined in this body (private vars)
            let body_pkg_vars = collect_package_item_vars(&s.items);
            for item in &s.items {
                match item {
                    ogsql_parser::ast::PackageItem::Procedure(p) => {
                        if !extract_sql {
                            rows.push(ParseCsvRow {
                                line: p.start_line.max(si.start_line),
                                end_line: p.start_line.max(si.start_line),
                                stmt_type: "Procedure".into(),
                                name: p.name.join("."),
                                parent: s.name.join("."),
                                parameters: format_params(&p.parameters),
                                return_type: String::new(),
                                sql: String::new(),
                                branch_path: String::new(),
                                branch_condition: String::new(),
                            });
                        }
                        if let Some(ref block) = p.block {
                            let mut vars = if do_vars {
                                collect_block_vars(block, &p.parameters, schema)
                            } else {
                                std::collections::HashMap::new()
                            };
                            // Merge package-level vars: spec → body-level → procedure-local (last wins)
                            for (k, v) in pkg_spec_vars {
                                vars.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                            for (k, v) in &body_pkg_vars {
                                vars.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                            let out_cursors = extract_out_cursor_set(&p.parameters);
                            rows.extend(collect_block_sql_rows(
                                block,
                                &p.name.join("."),
                                p.start_line.max(si.start_line),
                                &vars,
                                &out_cursors,
                                false,
                            ));
                        }
                    }
                    ogsql_parser::ast::PackageItem::Function(f) => {
                        if !extract_sql {
                            rows.push(ParseCsvRow {
                                line: f.start_line.max(si.start_line),
                                end_line: f.start_line.max(si.start_line),
                                stmt_type: "Function".into(),
                                name: f.name.join("."),
                                parent: s.name.join("."),
                                parameters: format_params(&f.parameters),
                                return_type: f.return_type.clone().unwrap_or_default(),
                                sql: String::new(),
                                branch_path: String::new(),
                                branch_condition: String::new(),
                            });
                        }
                        if let Some(ref block) = f.block {
                            let mut vars = if do_vars {
                                collect_block_vars(block, &f.parameters, schema)
                            } else {
                                std::collections::HashMap::new()
                            };
                            for (k, v) in pkg_spec_vars {
                                vars.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                            for (k, v) in &body_pkg_vars {
                                vars.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                            let out_cursors = extract_out_cursor_set(&f.parameters);
                            let is_return_cursor =
                                f.return_type.as_ref().is_some_and(|rt| rt.to_uppercase().contains("REFCURSOR"));
                            rows.extend(collect_block_sql_rows(
                                block,
                                &f.name.join("."),
                                f.start_line.max(si.start_line),
                                &vars,
                                &out_cursors,
                                is_return_cursor,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::CreatePackage(s) => {
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "CreatePackage".into(),
                    name: s.name.join("."),
                    parent: String::new(),
                    parameters: String::new(),
                    return_type: String::new(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            if extract_sql {
                let spec_vars = collect_package_item_vars(&s.items);
                for item in &s.items {
                    if let ogsql_parser::ast::PackageItem::Cursor(c) = item {
                        let sql = if let Some(ref parsed) = c.parsed_query {
                            let formatter = ogsql_parser::SqlFormatter::new();
                            replace_pl_vars_in_sql(&formatter.format_statement(parsed), &spec_vars)
                        } else if !c.query.trim().is_empty() {
                            replace_pl_vars_in_sql(c.query.trim(), &spec_vars)
                        } else {
                            continue;
                        };
                        if !sql.trim().is_empty() {
                            rows.push(ParseCsvRow {
                                line: si.start_line,
                                end_line: si.start_line,
                                stmt_type: "SpecCursor".into(),
                                name: c.name.clone(),
                                parent: s.name.join("."),
                                parameters: String::new(),
                                return_type: String::new(),
                                sql,
                                branch_path: String::new(),
                                branch_condition: String::new(),
                            });
                        }
                    }
                }
            }
            for item in &s.items {
                match item {
                    ogsql_parser::ast::PackageItem::Procedure(p) => {
                        rows.push(ParseCsvRow {
                            line: p.start_line.max(si.start_line),
                            end_line: p.start_line.max(si.start_line),
                            stmt_type: "Procedure".into(),
                            name: p.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&p.parameters),
                            return_type: String::new(),
                            sql: String::new(),
                            branch_path: String::new(),
                            branch_condition: String::new(),
                        });
                    }
                    ogsql_parser::ast::PackageItem::Function(f) => {
                        rows.push(ParseCsvRow {
                            line: f.start_line.max(si.start_line),
                            end_line: f.start_line.max(si.start_line),
                            stmt_type: "Function".into(),
                            name: f.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&f.parameters),
                            return_type: f.return_type.clone().unwrap_or_default(),
                            sql: String::new(),
                            branch_path: String::new(),
                            branch_condition: String::new(),
                        });
                    }
                    _ => {}
                }
            }
        }
        Statement::CreateProcedure(s) => {
            let proc_name = s.name.join(".");
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "CreateProcedure".into(),
                    name: proc_name.clone(),
                    parent: String::new(),
                    parameters: format_params(&s.parameters),
                    return_type: String::new(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            if let Some(ref block) = s.block {
                let vars = if do_vars {
                    collect_block_vars(block, &s.parameters, schema)
                } else {
                    std::collections::HashMap::new()
                };
                let out_cursors = extract_out_cursor_set(&s.parameters);
                rows.extend(collect_block_sql_rows(block, &proc_name, si.start_line, &vars, &out_cursors, false));
            }
        }
        Statement::CreateFunction(s) => {
            let func_name = s.name.join(".");
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "CreateFunction".into(),
                    name: func_name.clone(),
                    parent: String::new(),
                    parameters: format_params(&s.parameters),
                    return_type: s.return_type.clone().unwrap_or_default(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            if let Some(ref block) = s.block {
                let vars = if do_vars {
                    collect_block_vars(block, &s.parameters, schema)
                } else {
                    std::collections::HashMap::new()
                };
                let out_cursors = extract_out_cursor_set(&s.parameters);
                let is_return_cursor = s.return_type.as_ref().is_some_and(|rt| rt.to_uppercase().contains("REFCURSOR"));
                rows.extend(collect_block_sql_rows(
                    block,
                    &func_name,
                    si.start_line,
                    &vars,
                    &out_cursors,
                    is_return_cursor,
                ));
            }
        }
        Statement::Do(s) => {
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "Do".into(),
                    name: String::new(),
                    parent: String::new(),
                    parameters: String::new(),
                    return_type: String::new(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            if let Some(ref block) = s.block {
                let vars =
                    if do_vars { collect_block_vars(block, &[], schema) } else { std::collections::HashMap::new() };
                let out_cursors = std::collections::HashSet::new();
                rows.extend(collect_block_sql_rows(block, "", si.start_line, &vars, &out_cursors, false));
            }
        }
        Statement::AnonyBlock(s) => {
            if !extract_sql {
                rows.push(ParseCsvRow {
                    line: si.start_line,
                    end_line: si.start_line,
                    stmt_type: "AnonyBlock".into(),
                    name: String::new(),
                    parent: String::new(),
                    parameters: String::new(),
                    return_type: String::new(),
                    sql: String::new(),
                    branch_path: String::new(),
                    branch_condition: String::new(),
                });
            }
            let vars =
                if do_vars { collect_block_vars(&s.block, &[], schema) } else { std::collections::HashMap::new() };
            let out_cursors = std::collections::HashSet::new();
            rows.extend(collect_block_sql_rows(&s.block, "", si.start_line, &vars, &out_cursors, false));
        }
        _ if extract_sql => {
            return rows; // skip non-procedure statements in extract mode
        }
        Statement::Select(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "Select".into(),
                name: s
                    .from
                    .first()
                    .and_then(|f| {
                        if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                            Some(name.join("."))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        Statement::Insert(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "Insert".into(),
                name: s.table.join("."),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        Statement::Update(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "Update".into(),
                name: s
                    .tables
                    .first()
                    .and_then(|f| {
                        if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                            Some(name.join("."))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        Statement::Delete(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "Delete".into(),
                name: s
                    .tables
                    .first()
                    .and_then(|f| {
                        if let ogsql_parser::ast::TableRef::Table { name, .. } = f {
                            Some(name.join("."))
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        Statement::Merge(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "Merge".into(),
                name: match &s.target {
                    ogsql_parser::ast::TableRef::Table { name, .. } => name.join("."),
                    _ => String::new(),
                },
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        Statement::CreateTable(s) => {
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: "CreateTable".into(),
                name: s.name.join("."),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
        _ => {
            let type_name = serde_json::to_value(&si.statement)
                .ok()
                .and_then(|v| if let serde_json::Value::Object(map) = v { map.keys().next().cloned() } else { None })
                .unwrap_or_else(|| "Unknown".to_string());
            rows.push(ParseCsvRow {
                line: si.start_line,
                end_line: sql_end_line(si.start_line, &si.sql_text),
                stmt_type: type_name,
                name: String::new(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: si.sql_text.clone(),
                branch_path: String::new(),
                branch_condition: String::new(),
            });
        }
    }
    rows
}

fn output_csv_parse_rows(
    statements: &[ogsql_parser::StatementInfo],
    file_name: &str,
    rel_dir: &str,
    errors: &[ogsql_parser::ParserError],
    mybatis: bool,
    extract_sql: bool,
    schema: Option<&ogsql_parser::FullSchema>,
) {
    let pkg_spec_vars = collect_pkg_spec_vars_from_stmts(statements);
    for si in statements {
        let stmt_start = si.start_line;
        let stmt_end = si.end_line;

        let stmt_errors: Vec<&ogsql_parser::ParserError> = errors
            .iter()
            .filter(|e| {
                let eline = error_line(e);
                eline == 0 || (eline >= stmt_start && eline <= stmt_end)
            })
            .collect();

        let rows = flatten_statement(si, mybatis, extract_sql, schema, &pkg_spec_vars);
        for row in rows {
            if extract_sql && row.sql.trim().is_empty() {
                continue;
            }
            let (row_err, row_warn) = filter_errors_for_row(&stmt_errors, row.line, row.end_line);

            let sql = row.sql.trim().replace('\r', "");
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(file_name),
                csv_escape(rel_dir),
                row.line,
                csv_escape(&row.stmt_type),
                csv_escape(&row.name),
                csv_escape(&row.parent),
                csv_escape(&row.parameters),
                csv_escape(&row.return_type),
                csv_escape(&sql),
                csv_escape(&row_err),
                csv_escape(&row_warn),
                csv_escape(&row.branch_path),
                csv_escape(&row.branch_condition),
            );
        }
    }
}

fn output_extract_rows_text(
    statements: &[ogsql_parser::StatementInfo],
    mybatis: bool,
    schema: Option<&ogsql_parser::FullSchema>,
) {
    let pkg_spec_vars = collect_pkg_spec_vars_from_stmts(statements);
    for si in statements {
        let rows = flatten_statement(si, mybatis, true, schema, &pkg_spec_vars);
        for row in rows {
            if row.sql.trim().is_empty() {
                continue;
            }
            println!("[{}] L{}: {} | {}", row.stmt_type, row.line, row.name, row.sql);
        }
    }
}

fn output_csv_validate_header() {
    print!("\u{FEFF}");
    println!(
        "file,directory,line,type,name,parent,parameters,return_type,sql,valid,error_count,warning_count,errors,warnings"
    );
}

struct ValidateCsvRow {
    line: usize,
    row_type: String,
    name: String,
    parent: String,
    parameters: String,
    return_type: String,
    sql: String,
    start_line: usize,
    end_line: usize,
}

fn collect_validate_routine_rows(si: &ogsql_parser::StatementInfo) -> Vec<ValidateCsvRow> {
    use ogsql_parser::Statement;
    match &si.statement {
        Statement::CreatePackageBody(s) => {
            let mut rows = vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "CreatePackageBody".into(),
                name: s.name.join("."),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }];
            for item in &s.items {
                match item {
                    ogsql_parser::ast::PackageItem::Procedure(p) => {
                        rows.push(ValidateCsvRow {
                            line: p.start_line.max(si.start_line),
                            row_type: "Procedure".into(),
                            name: p.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&p.parameters),
                            return_type: String::new(),
                            sql: String::new(),
                            start_line: p.start_line.max(si.start_line),
                            end_line: if p.end_line > 0 { p.end_line } else { si.end_line },
                        });
                    }
                    ogsql_parser::ast::PackageItem::Function(f) => {
                        rows.push(ValidateCsvRow {
                            line: f.start_line.max(si.start_line),
                            row_type: "Function".into(),
                            name: f.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&f.parameters),
                            return_type: f.return_type.clone().unwrap_or_default(),
                            sql: String::new(),
                            start_line: f.start_line.max(si.start_line),
                            end_line: if f.end_line > 0 { f.end_line } else { si.end_line },
                        });
                    }
                    _ => {}
                }
            }
            rows
        }
        Statement::CreatePackage(s) => {
            let mut rows = vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "CreatePackage".into(),
                name: s.name.join("."),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }];
            for item in &s.items {
                match item {
                    ogsql_parser::ast::PackageItem::Procedure(p) => {
                        rows.push(ValidateCsvRow {
                            line: p.start_line.max(si.start_line),
                            row_type: "Procedure".into(),
                            name: p.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&p.parameters),
                            return_type: String::new(),
                            sql: String::new(),
                            start_line: p.start_line.max(si.start_line),
                            end_line: if p.end_line > 0 { p.end_line } else { si.end_line },
                        });
                    }
                    ogsql_parser::ast::PackageItem::Function(f) => {
                        rows.push(ValidateCsvRow {
                            line: f.start_line.max(si.start_line),
                            row_type: "Function".into(),
                            name: f.name.join("."),
                            parent: s.name.join("."),
                            parameters: format_params(&f.parameters),
                            return_type: f.return_type.clone().unwrap_or_default(),
                            sql: String::new(),
                            start_line: f.start_line.max(si.start_line),
                            end_line: if f.end_line > 0 { f.end_line } else { si.end_line },
                        });
                    }
                    _ => {}
                }
            }
            rows
        }
        Statement::CreateProcedure(s) => {
            vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "CreateProcedure".into(),
                name: s.name.join("."),
                parent: String::new(),
                parameters: format_params(&s.parameters),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }]
        }
        Statement::CreateFunction(s) => {
            vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "CreateFunction".into(),
                name: s.name.join("."),
                parent: String::new(),
                parameters: format_params(&s.parameters),
                return_type: s.return_type.clone().unwrap_or_default(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }]
        }
        Statement::Do(_) => {
            vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "Do".into(),
                name: String::new(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }]
        }
        Statement::AnonyBlock(_) => {
            vec![ValidateCsvRow {
                line: si.start_line,
                row_type: "AnonyBlock".into(),
                name: String::new(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }]
        }
        _ => {
            let type_name = serde_json::to_value(&si.statement)
                .ok()
                .and_then(|v| if let serde_json::Value::Object(map) = v { map.keys().next().cloned() } else { None })
                .unwrap_or_else(|| "Unknown".to_string());
            vec![ValidateCsvRow {
                line: si.start_line,
                row_type: type_name,
                name: String::new(),
                parent: String::new(),
                parameters: String::new(),
                return_type: String::new(),
                sql: String::new(),
                start_line: si.start_line,
                end_line: si.end_line,
            }]
        }
    }
}

fn output_csv_validate_rows(
    stmts: &[ogsql_parser::StatementInfo],
    errors: &[ogsql_parser::ParserError],
    var_errors: &[ogsql_parser::UndefinedVariableError],
    lint_warnings: &[ogsql_parser::linter::SqlWarning],
    file_name: &str,
    rel_dir: &str,
    min_level: Option<ogsql_parser::linter::WarningLevel>,
) {
    for si in stmts {
        let stmt_start = si.start_line;
        let stmt_end = si.end_line;

        let stmt_parse_errors: Vec<&ogsql_parser::ParserError> = errors
            .iter()
            .filter(|e| {
                let eline = error_line(e);
                eline == 0 || (eline >= stmt_start && eline <= stmt_end)
            })
            .collect();

        let stmt_var_errors: Vec<&ogsql_parser::UndefinedVariableError> = var_errors
            .iter()
            .filter(|ve| {
                ve.location.as_ref().map_or(true, |sp| sp.start.line >= stmt_start && sp.start.line <= stmt_end)
            })
            .collect();

        let stmt_lint_warnings: Vec<&ogsql_parser::linter::SqlWarning> = lint_warnings
            .iter()
            .filter(|w| {
                let wline = w.location.line;
                wline == 0 || (wline >= stmt_start && wline <= stmt_end)
            })
            .collect();

        let rows = collect_validate_routine_rows(si);
        let row_count = rows.len();
        let child_ranges: Vec<(usize, usize)> =
            if row_count > 1 { rows[1..].iter().map(|r| (r.start_line, r.end_line)).collect() } else { Vec::new() };
        let error_in_child = |eline: usize| -> bool { child_ranges.iter().any(|&(s, e)| eline >= s && eline <= e) };
        for (row_idx, row) in rows.iter().enumerate() {
            let is_parent_with_children = row_idx == 0 && row_count > 1;

            let mut row_parse_errors: Vec<&ogsql_parser::ParserError> = stmt_parse_errors
                .iter()
                .filter(|e| {
                    let eline = error_line(e);
                    let in_row = eline == 0 || (eline >= row.start_line && eline <= row.end_line);
                    if !in_row {
                        return false;
                    }
                    if is_parent_with_children {
                        eline == 0 || !error_in_child(eline)
                    } else {
                        true
                    }
                })
                .copied()
                .collect();

            // Filter parser warnings by min_level (when --lint --min-level is specified)
            if let Some(ml) = min_level {
                row_parse_errors.retain(|e| match e {
                    ogsql_parser::ParserError::Warning { level, .. } => *level >= ml,
                    _ => true,
                });
            }

            let row_parse_err = merge_error_messages(&row_parse_errors, false);
            let row_parse_warn = merge_error_messages(&row_parse_errors, true);

            let row_var_errs: Vec<&&ogsql_parser::UndefinedVariableError> = stmt_var_errors
                .iter()
                .filter(|ve| {
                    let in_row = ve.location.as_ref().map_or(row_idx == row_count - 1, |sp| {
                        sp.start.line >= row.start_line && sp.start.line <= row.end_line
                    });
                    if !in_row {
                        return false;
                    }
                    if is_parent_with_children {
                        ve.location.as_ref().is_some_and(|sp| !error_in_child(sp.start.line))
                    } else {
                        true
                    }
                })
                .collect();

            let row_lint: Vec<&&ogsql_parser::linter::SqlWarning> = stmt_lint_warnings
                .iter()
                .filter(|w| {
                    let wline = w.location.line;
                    let in_row = wline == 0 || (wline >= row.start_line && wline <= row.end_line);
                    if !in_row {
                        return false;
                    }
                    if is_parent_with_children {
                        wline == 0 || !error_in_child(wline)
                    } else {
                        true
                    }
                })
                .collect();

            let mut warn_parts: Vec<String> = Vec::new();
            if !row_parse_warn.is_empty() {
                warn_parts.push(row_parse_warn);
            }
            let merged_lint = merge_lint_warnings(&row_lint);
            if !merged_lint.is_empty() {
                warn_parts.push(merged_lint);
            }

            let mut err_parts: Vec<String> = Vec::new();
            if !row_parse_err.is_empty() {
                err_parts.push(row_parse_err);
            }
            for ve in &row_var_errs {
                let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
                let kind_label = match ve.kind {
                    ogsql_parser::UndefinedRefKind::Function => "undefined function",
                    ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
                };
                err_parts.push(format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info));
            }

            let parse_warn_count = row_parse_errors.iter().filter(|e| is_warning(e)).count();
            let error_count = {
                let real_parse_err_count = row_parse_errors.iter().filter(|e| !is_warning(e)).count();
                real_parse_err_count + row_var_errs.len()
            };
            let warning_count = parse_warn_count + row_lint.len();

            let all_err = err_parts.join("; ");
            let all_warn = warn_parts.join("; ");
            let is_valid = error_count == 0;
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                csv_escape(file_name),
                csv_escape(rel_dir),
                row.line,
                csv_escape(&row.row_type),
                csv_escape(&row.name),
                csv_escape(&row.parent),
                csv_escape(&row.parameters),
                csv_escape(&row.return_type),
                csv_escape(&row.sql),
                if is_valid { "VALID" } else { "INVALID" },
                error_count,
                warning_count,
                csv_escape(&all_err),
                csv_escape(&all_warn),
            );
        }
    }
}

fn merge_lint_warnings(warnings: &[&&ogsql_parser::linter::SqlWarning]) -> String {
    use std::collections::BTreeMap;

    struct Group {
        message: String,
        lines: Vec<usize>,
    }
    let mut groups: BTreeMap<&str, Group> = BTreeMap::new();
    for w in warnings {
        groups
            .entry(&w.rule_id)
            .or_insert_with(|| Group { message: w.message.clone(), lines: Vec::new() })
            .lines
            .push(w.location.line);
    }

    groups
        .iter()
        .map(|(rule_id, group)| {
            let label = format!("{}: {}", rule_id, group.message);
            if group.lines.len() > 1 {
                let mut unique_lines: Vec<usize> = group.lines.iter().copied().filter(|l| *l > 0).collect();
                unique_lines.sort();
                unique_lines.dedup();
                if unique_lines.is_empty() {
                    format!("{} (\u{00d7}{})", label, group.lines.len())
                } else {
                    let line_strs: Vec<String> = unique_lines.iter().map(|l| l.to_string()).collect();
                    format!("{} (\u{00d7}{}, lines {})", label, group.lines.len(), line_strs.join(", "))
                }
            } else {
                label
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Merge same-type error/warning messages, deduplicating identical text
/// and appending occurrence count + line numbers.
/// "msg; msg; msg" → "msg (×3, lines 1, 4, 10)"
fn merge_error_messages(errors: &[&ogsql_parser::ParserError], warn: bool) -> String {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for e in errors {
        if is_warning(e) != warn {
            continue;
        }
        let line = error_line(e);
        groups.entry(display_key(e)).or_default().push(line);
    }

    groups
        .iter()
        .map(|(msg, lines)| {
            if lines.len() > 1 {
                let mut unique_lines: Vec<usize> = lines.iter().copied().filter(|l| *l > 0).collect();
                unique_lines.sort();
                unique_lines.dedup();
                if unique_lines.is_empty() {
                    format!("{} (\u{00d7}{})", msg, lines.len())
                } else {
                    let line_strs: Vec<String> = unique_lines.iter().map(|l| l.to_string()).collect();
                    format!("{} (\u{00d7}{}, lines {})", msg, lines.len(), line_strs.join(", "))
                }
            } else {
                msg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Extract the display message from a ParserError without location info,
/// so that same-type errors at different positions are grouped together.
fn display_key(e: &ogsql_parser::ParserError) -> String {
    match e {
        ogsql_parser::ParserError::Warning { message, .. } => message.clone(),
        ogsql_parser::ParserError::ReservedKeywordAsIdentifier { keyword, .. } => {
            format!("reserved keyword \"{}\" cannot be used as identifier", keyword)
        }
        ogsql_parser::ParserError::UnexpectedToken { expected, got, .. } => {
            format!("expected {}, got {}", expected, got)
        }
        ogsql_parser::ParserError::UnexpectedEof { expected, .. } => {
            format!("unexpected end of input: expected {}", expected)
        }
        ogsql_parser::ParserError::UnsupportedSyntax { syntax, hint, .. } => {
            format!("{} ({})", syntax, hint)
        }
        _ => e.to_string(),
    }
}

fn filter_errors_for_row(
    errors: &[&ogsql_parser::ParserError],
    row_start_line: usize,
    row_end_line: usize,
) -> (String, String) {
    let in_range: Vec<&ogsql_parser::ParserError> = errors
        .iter()
        .filter(|e| {
            let eline = error_line(e);
            eline == 0 || (eline >= row_start_line && eline <= row_end_line)
        })
        .copied()
        .collect();

    let row_err = merge_error_messages(&in_range, false);
    let row_warn = merge_error_messages(&in_range, true);
    (row_err, row_warn)
}

fn extract_pl_block(stmt: &ogsql_parser::Statement) -> Option<&ogsql_parser::ast::plpgsql::PlBlock> {
    use ogsql_parser::Statement;
    match stmt {
        Statement::Do(d) => d.block.as_ref(),
        Statement::AnonyBlock(ab) => Some(&ab.block),
        Statement::CreateFunction(cf) => cf.block.as_ref(),
        Statement::CreateProcedure(cp) => cp.block.as_ref(),
        _ => None,
    }
}

fn cmd_tokenize(cli: &Cli) {
    if cli.file.len() > 1 {
        die!("Error: tokenize command accepts at most one --file");
    }
    let sql = read_input(cli.file.first().map(|s| s.as_str()));
    let mut tokenizer = Tokenizer::new(&sql);
    if cli.comments {
        tokenizer = tokenizer.preserve_comments(true);
    }
    if cli.mybatis {
        tokenizer = tokenizer.mybatis_params(true);
    }
    let tokens = match tokenizer.tokenize() {
        Ok(t) => t,
        Err(e) => die!("Tokenizer error: {}", e),
    };

    if cli.json {
        let info: Vec<TokenInfo> = tokens
            .iter()
            .map(|t| {
                let (token_type, value) = token_display(t);
                TokenInfo { token_type, value, line: t.location.line, column: t.location.column }
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&info).unwrap());
    } else {
        for t in &tokens {
            println!("{:?}", t.token);
        }
    }
}

fn cmd_json2sql(cli: &Cli) {
    if cli.file.len() > 1 {
        die!("Error: json2sql command accepts at most one --file");
    }
    let input = read_input(cli.file.first().map(|s| s.as_str()));
    let formatted = json2sql_to_strings(&input).unwrap_or_else(|e| die!("{}", e));

    if cli.json {
        let out = serde_json::json!({
            "statements": formatted,
            "count": formatted.len(),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        println!("{}", formatted.join(";\n"));
        println!(";");
    }
}

/// Shared JSON → SQL logic for the CLI and the stdio protocol server.
fn json2sql_to_strings(input: &str) -> Result<Vec<String>, String> {
    let json_value: serde_json::Value = serde_json::from_str(input).map_err(|e| format!("Invalid JSON: {}", e))?;

    let arr =
        json_value.get("statements").ok_or_else(|| "Expected JSON object with \"statements\" array".to_string())?;

    let statements: Vec<Statement> = if let serde_json::Value::Array(items) = arr {
        items
            .iter()
            .filter_map(|v| {
                if v.get("sql_text").is_some() {
                    serde_json::from_value::<StatementInfo>(v.clone()).ok().map(|si| si.statement)
                } else {
                    serde_json::from_value::<Statement>(v.clone()).ok()
                }
            })
            .collect()
    } else {
        return Err("\"statements\" must be an array".to_string());
    };

    if statements.is_empty() {
        return Err("No valid statements found in JSON".to_string());
    }

    let formatter = SqlFormatter::new();
    Ok(statements.iter().map(|s| formatter.format_statement(s)).collect())
}

fn is_warning(e: &ogsql_parser::ParserError) -> bool {
    ogsql_parser::is_warning(e)
}

fn merge_error_detail(err: &ogsql_parser::MergeSemanticError) -> String {
    match &err.detail {
        Some(d) => d.clone(),
        None => match err.kind {
            ogsql_parser::MergeSemanticErrorKind::DeleteNotSupported => {
                "GaussDB does not support MERGE ... WHEN MATCHED THEN DELETE".to_string()
            }
            ogsql_parser::MergeSemanticErrorKind::OnColumnUpdated => {
                "GaussDB does not allow updating columns referenced in the ON clause".to_string()
            }
        },
    }
}

/// Run PACKAGE consistency, MERGE semantics, and PL variable validation on
/// already-parsed statements. Returns errors to merge into the caller's error list.
/// Used by validate_sql (SQL files), validate-xml (iBatis XML), and validate-java
/// (Java source) to share the same validation pipeline.
///
/// This is now a thin wrapper over [`ogsql_parser::validate_statements`]; the
/// `ParserError` formatting stays here because it is a CLI output concern.
fn validate_from_stmts(
    stmts: &[ogsql_parser::StatementInfo],
    extra_funcs: &[String],
    strict: bool,
) -> (
    Vec<ogsql_parser::ParserError>,
    Vec<ogsql_parser::PackageConsistencyError>,
    Vec<ogsql_parser::UndefinedVariableError>,
) {
    // Delegate to the public library orchestrator (preserves typed errors).
    let report = ogsql_parser::validate_statements(stmts, extra_funcs, strict);

    // Format typed errors into ParserError for CLI display. This conversion
    // lives in the binary because ParserError formatting is a CLI concern.
    let mut errors = Vec::new();

    for pe in &report.package_errors {
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

    for me in &report.merge_errors {
        errors.push(ogsql_parser::ParserError::UnsupportedSyntax {
            location: me.location,
            syntax: "MERGE".to_string(),
            hint: merge_error_detail(me),
        });
    }

    (errors, report.package_errors, report.undefined_variable_errors)
}

fn validate_sql(
    sql: &str,
    mybatis: bool,
    extra_funcs: &[String],
    strict: bool,
) -> (
    Vec<ogsql_parser::StatementInfo>,
    Vec<ogsql_parser::ParserError>,
    Vec<ogsql_parser::PackageConsistencyError>,
    Vec<ogsql_parser::UndefinedVariableError>,
) {
    let output = parse_input(sql, false, mybatis);
    let mut errors = output.errors;

    let (core_errors, pkg_errors, var_errors) = validate_from_stmts(&output.statements, extra_funcs, strict);
    errors.extend(core_errors);

    (output.statements, errors, pkg_errors, var_errors)
}

fn cmd_validate(cli: &Cli, csv: bool, strict: bool) {
    if cli.file.len() <= 1 {
        cmd_validate_single(cli, cli.file.first().map(|s| s.as_str()), csv, strict);
    } else {
        cmd_validate_files(cli, csv, strict);
    }
}

fn cmd_validate_single(cli: &Cli, file_path: Option<&str>, csv: bool, strict: bool) {
    let sql = read_input(file_path);
    let (stmts, errors, pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &[], strict);

    let min_level = cli.min_level.as_ref().map(|s| match s.to_lowercase().as_str() {
        "prohibition" => ogsql_parser::linter::WarningLevel::Prohibition,
        "performance" => ogsql_parser::linter::WarningLevel::Performance,
        "caution" => ogsql_parser::linter::WarningLevel::Caution,
        _ => ogsql_parser::linter::WarningLevel::Suggestion,
    });

    let lint_warnings = if cli.lint {
        let config = build_lint_config(cli);
        run_lint(&stmts, ogsql_parser::linter::Confidence::Full, &config, load_lint_schema(cli).as_ref())
    } else {
        vec![]
    };

    let format_var_err = |ve: &ogsql_parser::UndefinedVariableError| -> String {
        let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
        let kind_label = match ve.kind {
            ogsql_parser::UndefinedRefKind::Function => "undefined function",
            ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
        };
        format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info)
    };
    if csv {
        let file_name = file_path.unwrap_or("<stdin>");
        let mut all_errors = errors.clone();
        for pe in &pkg_errors {
            let msg = match &pe.detail {
                Some(d) => format!("package {}: {} — {}", pe.package_name, pe.subprogram_name, d),
                None => format!("package {}: {} — {:?}", pe.package_name, pe.subprogram_name, pe.kind),
            };
            all_errors.push(ogsql_parser::ParserError::Warning {
                message: msg,
                location: ogsql_parser::SourceLocation::default(),
                level: ogsql_parser::linter::WarningLevel::Caution,
            });
        }
        output_csv_validate_header();
        output_csv_validate_rows(&stmts, &all_errors, &var_errors, &lint_warnings, file_name, ".", min_level);
        let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
        let has_errors = !real_errors.is_empty() || !var_errors.is_empty();
        if has_errors {
            std::process::exit(1);
        }
        return;
    }

    if cli.json {
        let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
        let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
        let mut out = serde_json::json!({
            "valid": real_errors.is_empty() && var_errors.is_empty(),
            "error_count": real_errors.len() + var_errors.len(),
            "warning_count": warnings.len(),
            "errors": errors,
        });
        if strict {
            out.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
        }
        if !pkg_errors.is_empty() {
            out.as_object_mut()
                .unwrap()
                .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
        }
        if !var_errors.is_empty() {
            out.as_object_mut().unwrap().insert("undefined_variables".to_string(), serde_json::json!(var_errors));
        }
        if !lint_warnings.is_empty() {
            out.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            out.as_object_mut().unwrap().insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
        let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
        let has_var_errors = !var_errors.is_empty();
        if real_errors.is_empty() && warnings.is_empty() && !has_var_errors {
            println!("VALID");
        } else if real_errors.is_empty() && !has_var_errors {
            println!("VALID ({} warning(s)):", warnings.len());
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        } else {
            let total_errors = real_errors.len() + var_errors.len();
            if strict {
                println!("INVALID ({} error(s), {} warning(s)) [strict mode]:", total_errors, warnings.len());
            } else {
                println!("INVALID ({} error(s), {} warning(s)):", total_errors, warnings.len());
            }
            for e in &real_errors {
                eprintln!("  error: {}", e);
            }
            for ve in &var_errors {
                eprintln!("  error: {}", format_var_err(ve));
            }
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        }
        if !lint_warnings.is_empty() {
            eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
            format_warnings_text(&lint_warnings);
            eprintln!("\n── Summary ──");
            let summary = format_warnings_summary(&lint_warnings);
            for (level, count) in summary.get("by_level").unwrap().as_object().unwrap() {
                eprintln!("  {}: {}", level, count);
            }
            eprintln!("  Total: {} lint warnings", lint_warnings.len());
        }
        if !real_errors.is_empty() && cli.verbose {
            write_error_log(&sql, file_path, &stmts, &real_errors);
        }
        if !real_errors.is_empty() || !var_errors.is_empty() {
            std::process::exit(1);
        }
    }
}

fn cmd_validate_files(cli: &Cli, csv: bool, strict: bool) {
    let format_var_err = |ve: &ogsql_parser::UndefinedVariableError| -> String {
        let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
        let kind_label = match ve.kind {
            ogsql_parser::UndefinedRefKind::Function => "undefined function",
            ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
        };
        format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info)
    };

    let auto_schema_files = if cli.schema_json.is_none() && cli.lint {
        let mut fs = ogsql_parser::FullSchema::default();
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let output = parse_input(&sql, false, cli.mybatis);
            let ddl = ogsql_parser::collect_ddl_schema(&output.statements);
            fs.columns.extend(ddl.columns);
            for (t, idxs) in ddl.indexes {
                fs.indexes.entry(t).or_default().extend(idxs);
            }
        }
        fs
    } else {
        ogsql_parser::FullSchema::default()
    };

    let min_level = cli.min_level.as_ref().map(|s| match s.to_lowercase().as_str() {
        "prohibition" => ogsql_parser::linter::WarningLevel::Prohibition,
        "performance" => ogsql_parser::linter::WarningLevel::Performance,
        "caution" => ogsql_parser::linter::WarningLevel::Caution,
        _ => ogsql_parser::linter::WarningLevel::Suggestion,
    });

    if csv {
        output_csv_validate_header();
        let mut any_errors = false;
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let (stmts, errors, pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &[], strict);
            let mut all_errors = errors.clone();
            for pe in &pkg_errors {
                let msg = match &pe.detail {
                    Some(d) => format!("package {}: {} — {}", pe.package_name, pe.subprogram_name, d),
                    None => format!("package {}: {} — {:?}", pe.package_name, pe.subprogram_name, pe.kind),
                };
                all_errors.push(ogsql_parser::ParserError::Warning {
                    message: msg,
                    location: ogsql_parser::SourceLocation::default(),
                    level: ogsql_parser::linter::WarningLevel::Caution,
                });
            }
            let lint_warnings = if cli.lint {
                let config = build_lint_config(cli);
                let schema = load_lint_schema(cli).unwrap_or_else(|| auto_schema_files.clone());
                let schema_ref =
                    if schema.columns.is_empty() && schema.indexes.is_empty() { None } else { Some(&schema) };
                run_lint(&stmts, ogsql_parser::linter::Confidence::Full, &config, schema_ref)
            } else {
                vec![]
            };
            output_csv_validate_rows(&stmts, &all_errors, &var_errors, &lint_warnings, file_path, ".", min_level);
            let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
            if !real_errors.is_empty() || !var_errors.is_empty() {
                any_errors = true;
            }
        }
        if any_errors {
            std::process::exit(1);
        }
        return;
    }

    if cli.json {
        let mut all_results = Vec::new();
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let (_stmts, errors, pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &[], strict);

            let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
            let mut file_result = serde_json::json!({
                "file": file_path,
                "valid": real_errors.is_empty() && var_errors.is_empty(),
                "error_count": real_errors.len() + var_errors.len(),
                "warning_count": warnings.len(),
                "errors": errors,
            });
            if strict {
                file_result.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
            }
            if !pkg_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
            }
            if !var_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("undefined_variables".to_string(), serde_json::json!(var_errors));
            }
            all_results.push(file_result);
        }
        let out = serde_json::json!({
            "files": all_results,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        let mut any_invalid = false;
        for file_path in &cli.file {
            let sql = read_input(Some(file_path.as_str()));
            let (stmts, errors, _pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &[], strict);

            let lint_warnings = if cli.lint {
                let config = build_lint_config(cli);
                let schema = load_lint_schema(cli).unwrap_or_else(|| auto_schema_files.clone());
                let schema_ref =
                    if schema.columns.is_empty() && schema.indexes.is_empty() { None } else { Some(&schema) };
                run_lint(&stmts, ogsql_parser::linter::Confidence::Full, &config, schema_ref)
            } else {
                vec![]
            };

            let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
            let has_var_errors = !var_errors.is_empty();

            if real_errors.is_empty() && warnings.is_empty() && !has_var_errors {
                println!("{}: VALID", file_path);
            } else if real_errors.is_empty() && !has_var_errors {
                println!("{}: VALID ({} warning(s)):", file_path, warnings.len());
                for w in &warnings {
                    eprintln!("  warning: {}", w);
                }
            } else {
                any_invalid = true;
                let total_errors = real_errors.len() + var_errors.len();
                println!("{}: INVALID ({} error(s), {} warning(s)):", file_path, total_errors, warnings.len());
                for e in &real_errors {
                    eprintln!("  error: {}", e);
                }
                for ve in &var_errors {
                    eprintln!("  error: {}", format_var_err(ve));
                }
                for w in &warnings {
                    eprintln!("  warning: {}", w);
                }
            }
            if !lint_warnings.is_empty() {
                eprintln!("\n── Lint Warnings for {} ({}) ──", file_path, lint_warnings.len());
                format_warnings_text(&lint_warnings);
            }
            if !real_errors.is_empty() && cli.verbose {
                write_error_log(&sql, Some(file_path), &stmts, &real_errors);
            }
        }
        if any_invalid {
            std::process::exit(1);
        }
    }
}

fn cmd_validate_dir(cli: &Cli, dir_paths: &[String], exts: &[String], csv: bool, stats: bool, strict: bool) {
    use std::path::Path;

    for dir_path in dir_paths {
        if !Path::new(dir_path).is_dir() {
            die!("Error: '{}' is not a directory", dir_path);
        }
    }

    let normalized_exts: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();

    let mut files: Vec<(String, String, std::path::PathBuf)> = Vec::new();
    for dir_path in dir_paths {
        let root = Path::new(dir_path);
        for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if !normalized_exts.contains(&file_ext) {
                continue;
            }

            let rel_dir = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| {
                    let s = p.to_str().unwrap_or(".");
                    if s.is_empty() {
                        "."
                    } else {
                        s
                    }
                })
                .unwrap_or(".")
                .to_string();

            let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

            files.push((file_name, rel_dir, path.to_path_buf()));
        }
    }

    files.sort_by(|a, b| a.2.cmp(&b.2));
    files.dedup_by(|a, b| a.2 == b.2);

    if files.is_empty() {
        eprintln!("No files found with extension(s): {}", exts.join(", "));
        return;
    }

    let mut total_files = 0usize;
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;
    let mut any_invalid = false;
    let mut all_results: Vec<(String, String, String, Vec<ogsql_parser::ParserError>)> = Vec::new();
    let mut files_with_errors: HashSet<String> = HashSet::new();
    let mut files_with_warnings: HashSet<String> = HashSet::new();
    let mut stmt_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();

    if csv {
        output_csv_validate_header();
    }

    // Pre-scan: collect all defined routine names across all files
    let mut all_defined_funcs: Vec<String> = Vec::new();
    for (_, _, abs_path) in &files {
        let sql = read_file_path(abs_path);
        let output = parse_input(&sql, false, cli.mybatis);
        all_defined_funcs.extend(ogsql_parser::collect_defined_routine_names(&output.statements));
    }
    all_defined_funcs.sort();
    all_defined_funcs.dedup();

    // Pre-scan: collect DDL schema (columns + indexes) from all files
    let mut auto_schema = ogsql_parser::FullSchema::default();
    if cli.schema_json.is_none() {
        for (_, _, abs_path) in &files {
            let sql = read_file_path(abs_path);
            let output = parse_input(&sql, false, cli.mybatis);
            let fs = ogsql_parser::collect_ddl_schema(&output.statements);
            auto_schema.columns.extend(fs.columns);
            for (table, idxs) in fs.indexes {
                auto_schema.indexes.entry(table).or_default().extend(idxs);
            }
        }
    }

    let min_level = cli.min_level.as_ref().map(|s| match s.to_lowercase().as_str() {
        "prohibition" => ogsql_parser::linter::WarningLevel::Prohibition,
        "performance" => ogsql_parser::linter::WarningLevel::Performance,
        "caution" => ogsql_parser::linter::WarningLevel::Caution,
        _ => ogsql_parser::linter::WarningLevel::Suggestion,
    });

    for (file_name, rel_dir, abs_path) in &files {
        let sql = read_file_path(abs_path);
        let (stmts, errors, pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &all_defined_funcs, strict);

        let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
        let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
        let has_var_errors = !var_errors.is_empty();

        let format_var_err = |ve: &ogsql_parser::UndefinedVariableError| -> String {
            let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
            let kind_label = match ve.kind {
                ogsql_parser::UndefinedRefKind::Function => "undefined function",
                ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
            };
            format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info)
        };

        if !real_errors.is_empty() || has_var_errors {
            any_invalid = true;
            files_with_errors.insert(file_name.clone());
        }
        if !warnings.is_empty() {
            files_with_warnings.insert(file_name.clone());
        }

        total_errors += real_errors.len() + var_errors.len();
        total_warnings += warnings.len();
        total_files += 1;

        for si in &stmts {
            *stmt_counts.entry(stmt_category(&si.statement)).or_insert(0) += 1;
        }
        for err in &errors {
            let kind = parser_error_kind(err);
            let file_set = if is_warning(err) { &mut warning_kinds } else { &mut error_kinds };
            let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
            entry.0 += 1;
            entry.1.insert(file_name.clone());
        }

        if csv {
            let mut all_errors = errors.clone();
            for pe in &pkg_errors {
                let msg = match &pe.detail {
                    Some(d) => format!("package {}: {} — {}", pe.package_name, pe.subprogram_name, d),
                    None => format!("package {}: {} — {:?}", pe.package_name, pe.subprogram_name, pe.kind),
                };
                all_errors.push(ogsql_parser::ParserError::Warning {
                    message: msg,
                    location: ogsql_parser::SourceLocation::default(),
                    level: ogsql_parser::linter::WarningLevel::Caution,
                });
            }
            let lint_warnings = if cli.lint {
                let config = build_lint_config(cli);
                run_lint(&stmts, ogsql_parser::linter::Confidence::Full, &config, load_lint_schema(cli).as_ref())
            } else {
                vec![]
            };
            output_csv_validate_rows(&stmts, &all_errors, &var_errors, &lint_warnings, file_name, rel_dir, min_level);
        } else if cli.json {
            if !real_errors.is_empty() {
                all_results.push((
                    file_name.clone(),
                    rel_dir.clone(),
                    sql.clone(),
                    errors.iter().filter(|e| !is_warning(e)).cloned().collect(),
                ));
            }
        } else if real_errors.is_empty() && warnings.is_empty() && !has_var_errors {
            println!("[{}/{}] VALID", rel_dir, file_name);
        } else if real_errors.is_empty() && !has_var_errors {
            println!("[{}/{}] VALID ({} warning(s))", rel_dir, file_name, warnings.len());
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        } else {
            let total_errs = real_errors.len() + var_errors.len();
            println!("[{}/{}] INVALID ({} error(s), {} warning(s))", rel_dir, file_name, total_errs, warnings.len());
            for e in &real_errors {
                eprintln!("  error: {}", e);
            }
            for ve in &var_errors {
                eprintln!("  error: {}", format_var_err(ve));
            }
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
            if cli.verbose {
                let all_real: Vec<&ogsql_parser::ParserError> = real_errors.to_vec();
                write_error_log(&sql, Some(&format!("{}/{}", rel_dir, file_name)), &stmts, &all_real);
            }
        }
        if cli.verbose && !real_errors.is_empty() && !has_var_errors {
            write_error_log(&sql, Some(&format!("{}/{}", rel_dir, file_name)), &stmts, &real_errors);
        }
        let _ = &stmts;
    }

    if csv {
        if stats {
            let total_stmts: usize = stmt_counts.values().sum();
            print_parse_stats(
                total_files,
                &files_with_errors,
                &files_with_warnings,
                total_stmts,
                &stmt_counts,
                &error_kinds,
                &warning_kinds,
                "validate --csv",
            );
        }
        if any_invalid {
            std::process::exit(1);
        }
    } else if cli.json {
        let mut results = Vec::new();
        for (file_name, rel_dir, abs_path) in &files {
            let sql = read_file_path(abs_path);
            let (_stmts, errors, pkg_errors, var_errors) = validate_sql(&sql, cli.mybatis, &all_defined_funcs, strict);
            let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();

            let mut file_result = serde_json::json!({
                "file": file_name,
                "directory": rel_dir,
                "valid": real_errors.is_empty() && var_errors.is_empty(),
                "error_count": real_errors.len() + var_errors.len(),
                "warning_count": warnings.len(),
                "errors": errors,
            });
            if !pkg_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
            }
            results.push(file_result);
        }
        let mut out = serde_json::json!({
            "valid": !any_invalid,
            "total_files": total_files,
            "total_errors": total_errors,
            "total_warnings": total_warnings,
            "files": results,
        });
        if strict {
            out.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
        }
        if !all_results.is_empty() {
            out.as_object_mut().unwrap().insert("error_log".to_string(), serde_json::json!(all_results.len()));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        if stats {
            let total_stmts: usize = stmt_counts.values().sum();
            print_parse_stats(
                total_files,
                &files_with_errors,
                &files_with_warnings,
                total_stmts,
                &stmt_counts,
                &error_kinds,
                &warning_kinds,
                "validate -j",
            );
        }
    } else {
        println!();
        if stats {
            let total_stmts: usize = stmt_counts.values().sum();
            print_parse_stats(
                total_files,
                &files_with_errors,
                &files_with_warnings,
                total_stmts,
                &stmt_counts,
                &error_kinds,
                &warning_kinds,
                "validate",
            );
        }
        if any_invalid {
            print!(
                "Result: INVALID — {} error(s), {} warning(s) from {} file(s)",
                total_errors, total_warnings, total_files
            );
            if strict {
                println!(" (--strict mode enabled)");
            } else {
                println!();
            }
            std::process::exit(1);
        } else if total_warnings > 0 {
            println!("Result: VALID — {} warning(s) from {} file(s)", total_warnings, total_files);
        } else {
            println!("Result: VALID — {} file(s)", total_files);
        }
    }
}

fn write_error_log(source: &str, file_path: Option<&str>, stmts: &[StatementInfo], errors: &[&ParserError]) {
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new().append(true).create(true).open("error.log") {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  warning: cannot create error.log: {}", e);
            return;
        }
    };

    let source_lines: Vec<&str> = source.lines().collect();

    let mut groups: Vec<(usize, Option<String>, usize, usize, Vec<usize>)> = Vec::new();

    for (err_idx, err) in errors.iter().enumerate() {
        let (line, _col) = match err {
            ParserError::UnexpectedToken { location, .. } => (location.line, location.column),
            ParserError::UnexpectedEof { location, .. } => (location.line, location.column),
            ParserError::TokenizerError(_) => (0, 0),
            ParserError::ReservedKeywordAsIdentifier { location, .. } => (location.line, location.column),
            _ => (0, 0),
        };
        if line == 0 {
            groups.push((usize::MAX, None, 0, 0, vec![err_idx])); // sentinel: no line info
        } else if let Some(si_idx) = stmts.iter().position(|si| line >= si.start_line && line <= si.end_line) {
            let si = &stmts[si_idx];
            let (sub_name, sub_start, sub_end) = find_error_sub_item(&si.statement, line);
            if let Some(pos) = groups.iter().position(|(idx, sn, ss, se, _)| {
                *idx == si_idx && *sn == sub_name && *ss == sub_start && *se == sub_end
            }) {
                groups[pos].4.push(err_idx);
            } else {
                groups.push((si_idx, sub_name, sub_start, sub_end, vec![err_idx]));
            }
        } else {
            groups.push((usize::MAX, None, 0, 0, vec![err_idx])); // sentinel: unmatched line
        }
    }

    let context_radius: usize = 5;

    for (si_idx, sub_name, sub_start, sub_end, err_indices) in &groups {
        if let Some(fp) = file_path {
            let _ = writeln!(file, "File: {}", fp);
        }
        for &err_idx in err_indices {
            let _ = writeln!(file, "Error: {}", errors[err_idx]);
        }

        if *si_idx != usize::MAX {
            let si = &stmts[*si_idx];

            let all_lines: Vec<&str> = if !si.sql_text.is_empty() {
                si.sql_text.lines().collect()
            } else {
                // sql_text 为空 (Package Body / DDL) 时回退到完整源码
                let start = si.start_line.saturating_sub(1);
                let end = si.end_line.min(source_lines.len());
                source_lines[start..end].to_vec()
            };
            let stmt_start = si.start_line;

            let (label_start, label_end, omitted_after) = if *sub_start > 0 && *sub_end > 0 {
                (*sub_start, *sub_end, sub_end - sub_start + 1)
            } else {
                (si.start_line, si.end_line, all_lines.len())
            };

            let error_lines: Vec<usize> =
                err_indices.iter().map(|&ei| error_line(errors[ei])).filter(|&l| l > 0).collect();

            let label_relative_start = label_start.saturating_sub(stmt_start);
            let label_len = label_end.saturating_sub(label_start) + 1;

            let mut ctx_start = label_relative_start.saturating_sub(context_radius);
            let mut ctx_end = (label_relative_start + label_len).min(all_lines.len());

            for &eline in &error_lines {
                let rel = eline.saturating_sub(stmt_start);
                ctx_start = ctx_start.min(rel.saturating_sub(context_radius));
                ctx_end = ctx_end.max((rel + 1).min(all_lines.len()));
            }

            let ctx_lines = &all_lines[ctx_start..ctx_end];
            let omitted_before = label_relative_start.saturating_sub(ctx_start);
            let omitted_after_actual = all_lines.len().saturating_sub(ctx_end);

            if let Some(name) = sub_name {
                let _ =
                    writeln!(file, "In {} (line {}-{} of {}-line statement):", name, sub_start, sub_end, omitted_after);
            } else {
                let _ = writeln!(file, "Statement (line {}-{}):", si.start_line, si.end_line);
            }

            for (i, l) in ctx_lines.iter().enumerate() {
                let abs_line = ctx_start + i + stmt_start;
                if error_lines.contains(&abs_line) {
                    let _ = writeln!(file, "  {:>4} |> {}", abs_line, l);
                } else {
                    let _ = writeln!(file, "  {:>4} |  {}", abs_line, l);
                }
            }
            if omitted_before > context_radius || omitted_after_actual > context_radius {
                let _ = writeln!(
                    file,
                    "  ... ({} lines omitted) ...",
                    omitted_before.saturating_sub(context_radius) + omitted_after_actual.saturating_sub(context_radius)
                );
            }
        }
        let _ = writeln!(file, "{}", "-".repeat(60));
    }
    // eprintln!("  error details written to error.log");
}

fn write_dir_error_log(
    out_root: &std::path::Path,
    all_errors: &[(String, String, String, Vec<ogsql_parser::ParserError>)],
) {
    use std::io::Write;
    let log_path = out_root.join("error.log");
    let mut file = match std::fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  warning: cannot create {}: {}", log_path.display(), e);
            return;
        }
    };
    let context_radius: usize = 5;
    for (file_name, rel_dir, source, errors) in all_errors {
        let _ = writeln!(file, "File: {}/{}", rel_dir, file_name);
        for err in errors {
            let _ = writeln!(file, "Error: {}", err);
        }
        let source_lines: Vec<&str> = source.lines().collect();
        let err_lines: Vec<usize> = errors.iter().map(error_line).filter(|&l| l > 0).collect();
        if err_lines.is_empty() {
            let _ = writeln!(file, "{}", "-".repeat(60));
            continue;
        }
        let min_line = *err_lines.iter().min().unwrap_or(&1);
        let max_line = *err_lines.iter().max().unwrap_or(&1);
        let start = min_line.saturating_sub(context_radius + 1);
        let end = (max_line + context_radius).min(source_lines.len());
        for (i, line) in source_lines[start..end].iter().enumerate() {
            let abs_line = start + i + 1;
            if err_lines.contains(&abs_line) {
                let _ = writeln!(file, "  {:>4} |> {}", abs_line, line);
            } else {
                let _ = writeln!(file, "  {:>4} |  {}", abs_line, line);
            }
        }
        let _ = writeln!(file, "{}", "-".repeat(60));
    }
    eprintln!(
        "  {} error(s) from {} file(s) written to {}",
        all_errors.iter().map(|(_, _, _, e)| e.len()).sum::<usize>(),
        all_errors.len(),
        log_path.display()
    );
}

fn error_line(err: &ParserError) -> usize {
    match err {
        ParserError::UnexpectedToken { location, .. } => location.line,
        ParserError::UnexpectedEof { location, .. } => location.line,
        ParserError::ReservedKeywordAsIdentifier { location, .. } => location.line,
        ParserError::Warning { location, .. } => location.line,
        ParserError::UnsupportedSyntax { location, .. } => location.line,
        ParserError::TokenizerError(te) => tokenizer_error_line(te),
    }
}

fn tokenizer_error_line(err: &ogsql_parser::TokenizerError) -> usize {
    use ogsql_parser::TokenizerError;
    match err {
        TokenizerError::UnterminatedString(loc)
        | TokenizerError::UnterminatedComment(loc)
        | TokenizerError::UnterminatedDollarString(loc)
        | TokenizerError::UnterminatedQuotedIdentifier(loc)
        | TokenizerError::InvalidCharacter { location: loc, .. } => loc.line,
        TokenizerError::UnexpectedEof { location: loc, .. } => loc.line,
    }
}

fn find_error_sub_item(stmt: &Statement, error_line: usize) -> (Option<String>, usize, usize) {
    use ogsql_parser::ast::PackageItem;
    match stmt {
        Statement::CreatePackageBody(pkg) => {
            for item in &pkg.items {
                match item {
                    PackageItem::Procedure(p)
                        if p.start_line > 0 && error_line >= p.start_line && error_line <= p.end_line =>
                    {
                        let name = p.name.join(".");
                        return (Some(format!("PROCEDURE {}", name)), p.start_line, p.end_line);
                    }
                    PackageItem::Function(f)
                        if f.start_line > 0 && error_line >= f.start_line && error_line <= f.end_line =>
                    {
                        let name = f.name.join(".");
                        return (Some(format!("FUNCTION {}", name)), f.start_line, f.end_line);
                    }
                    _ => {}
                }
            }
            (None, 0, 0)
        }
        Statement::CreatePackage(pkg) => {
            for item in &pkg.items {
                match item {
                    PackageItem::Procedure(p)
                        if p.start_line > 0 && error_line >= p.start_line && error_line <= p.end_line =>
                    {
                        let name = p.name.join(".");
                        return (Some(format!("PROCEDURE {}", name)), p.start_line, p.end_line);
                    }
                    PackageItem::Function(f)
                        if f.start_line > 0 && error_line >= f.start_line && error_line <= f.end_line =>
                    {
                        let name = f.name.join(".");
                        return (Some(format!("FUNCTION {}", name)), f.start_line, f.end_line);
                    }
                    _ => {}
                }
            }
            (None, 0, 0)
        }
        _ => (None, 0, 0),
    }
}

#[cfg(feature = "serve")]
mod serve;

mod serve_stdio;

#[cfg(feature = "tui")]
fn cmd_playground() {
    use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
    use crossterm::execute;
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
    use ratatui::backend::CrosstermBackend;
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Tabs, Wrap};
    use ratatui::Frame;
    use std::io;

    struct App {
        input: String,
        cursor: usize,
        tab_index: usize,
        input_scroll: u16,
        output_scroll: u16,
    }

    impl App {
        fn new() -> Self {
            Self {
                input: String::from("SELECT id, name\nFROM users\nWHERE status = 'active'\nORDER BY id LIMIT 10;"),
                cursor: 42,
                tab_index: 0,
                input_scroll: 0,
                output_scroll: 0,
            }
        }
    }

    fn compute_output(app: &App) -> String {
        let sql = app.input.trim();
        if sql.is_empty() {
            return String::new();
        }

        let tokens = match Tokenizer::new(sql).tokenize() {
            Ok(t) => t,
            Err(e) => return format!("Tokenizer error: {}", e),
        };

        match app.tab_index {
            1 => tokens
                .iter()
                .map(|t| {
                    let (tt, val) = token_display(t);
                    format!("{:<16} L{:>3}:C{:>3}  {}", tt, t.location.line, t.location.column, val)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            2 => {
                let mut parser = Parser::new(tokens);
                let mut stmts = Vec::new();
                while let Some(r) = parser.parse_next() {
                    if let Ok(s) = r {
                        stmts.push(s);
                    }
                }
                let fmt = SqlFormatter::new();
                stmts.iter().map(|s| fmt.format_statement(s)).collect::<Vec<_>>().join(";\n")
            }
            _ => {
                let mut parser = Parser::new(tokens);
                let mut stmts = Vec::new();
                while let Some(r) = parser.parse_next() {
                    if let Ok(s) = r {
                        stmts.push(s);
                    }
                }
                let errors = parser.errors();
                let mut out = format!("{:#?}", stmts);
                if !errors.is_empty() {
                    out.push_str("\n\nErrors:\n");
                    for e in errors {
                        out.push_str(&format!("  {}\n", e));
                    }
                }
                out
            }
        }
    }

    fn draw(f: &mut Frame, app: &App) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(f.area());

        let input_title = Line::from(vec![
            Span::styled(" SQL Input ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" (Esc=quit, Tab=switch view, Shift+Up/Down=scroll output)"),
        ]);
        let input_block =
            Block::default().title(input_title).borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow));
        let input = Paragraph::new(app.input.as_str()).block(input_block).scroll((app.input_scroll, 0));
        f.render_widget(input, chunks[0]);

        let tabs_block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan));
        let tabs = Tabs::new(vec!["AST", "Tokens", "Formatted"])
            .block(tabs_block.clone())
            .select(app.tab_index)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        f.render_widget(&tabs, chunks[1]);

        let output_area = tabs_block.inner(chunks[1]);
        let output_text = compute_output(app);
        let output = Paragraph::new(output_text.as_str()).wrap(Wrap { trim: false }).scroll((app.output_scroll, 0));
        f.render_widget(output, output_area);

        let line_before_cursor = app.input[..app.cursor].matches('\n').count() as u16;
        let last_col = app.input[..app.cursor].split('\n').next_back().map(|l| l.len()).unwrap_or(0) as u16;
        f.set_cursor_position((chunks[0].x + last_col + 1, chunks[0].y + line_before_cursor + 1 - app.input_scroll));
    }

    enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("Failed to enter alt screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend).expect("Failed to create terminal");

    let mut app = App::new();

    loop {
        terminal.draw(|f| draw(f, &app)).expect("Failed to draw");

        match event::read().expect("Failed to read event") {
            Event::Key(KeyEvent { code, modifiers, .. }) => match code {
                KeyCode::Esc => break,
                KeyCode::Tab => app.tab_index = (app.tab_index + 1) % 3,
                KeyCode::Backspace => {
                    if app.cursor > 0 {
                        app.input.remove(app.cursor - 1);
                        app.cursor -= 1;
                    }
                }
                KeyCode::Delete => {
                    if app.cursor < app.input.len() {
                        app.input.remove(app.cursor);
                    }
                }
                KeyCode::Left => {
                    if app.cursor > 0 {
                        app.cursor -= 1;
                    }
                }
                KeyCode::Right => {
                    if app.cursor < app.input.len() {
                        app.cursor += 1;
                    }
                }
                KeyCode::Up => {
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        app.output_scroll = app.output_scroll.saturating_sub(1);
                    }
                }
                KeyCode::Down => {
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        app.output_scroll = app.output_scroll.saturating_add(1);
                    }
                }
                KeyCode::Enter => {
                    app.input.insert(app.cursor, '\n');
                    app.cursor += 1;
                }
                KeyCode::Char(c) => {
                    app.input.insert(app.cursor, c);
                    app.cursor += 1;
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }

    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("Failed to leave alt screen");
    terminal.show_cursor().unwrap();
}

#[cfg(feature = "ibatis")]
fn cmd_parse_xml(cli: &Cli, dir: Option<&str>, csv: bool, java_src: Option<&str>, stats: bool, structured: bool) {
    if dir.is_some() && !cli.file.is_empty() {
        die!("Error: --dir and -f are mutually exclusive");
    }

    #[cfg(feature = "java")]
    let java_roots: Vec<std::path::PathBuf> = match java_src {
        Some(path) => {
            let p = std::path::Path::new(path);
            if !p.is_dir() {
                die!("Error: '{}' is not a directory", path);
            }
            vec![p.to_path_buf()]
        }
        None => {
            let scan_dir = dir.map(std::path::Path::new).unwrap_or_else(|| std::path::Path::new("."));
            let detected = ogsql_parser::ibatis::detect_java_roots(scan_dir);
            if !detected.is_empty() {
                eprintln!("Auto-detected Java source roots:");
                for r in &detected {
                    eprintln!("  {}", r.display());
                }
            }
            detected
        }
    };
    #[cfg(not(feature = "java"))]
    let java_roots: Vec<std::path::PathBuf> = Vec::new();

    if structured {
        if dir.is_some() {
            die!("Error: --structured is not supported with --dir yet");
        }
        cmd_parse_xml_structured(cli, csv);
        return;
    }

    if let Some(dir_path) = dir {
        cmd_parse_xml_dir(cli, dir_path, csv, &java_roots, stats);
    } else {
        cmd_parse_xml_single(cli, csv, &java_roots);
    }
}

#[cfg(feature = "ibatis")]
fn cmd_parse_xml_single(cli: &Cli, csv: bool, java_roots: &[std::path::PathBuf]) {
    if cli.file.len() > 1 {
        die!("Error: parse-xml command accepts at most one --file");
    }
    let file_opt = cli.file.first().map(|s| s.as_str());
    let input = match file_opt {
        Some(path) => std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e)),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            buf
        }
    };

    #[cfg(feature = "java")]
    let result = if java_roots.is_empty() {
        ogsql_parser::ibatis::parse_mapper_bytes_with_path(&input, file_opt)
    } else {
        ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(&input, file_opt, java_roots.to_vec())
    };
    #[cfg(not(feature = "java"))]
    let result = ogsql_parser::ibatis::parse_mapper_bytes_with_path(&input, file_opt);

    let lint_warnings = if cli.lint {
        let config = build_lint_config(cli);
        let mut ws = lint_xml_statements(&result.statements, &config);
        let structured = ogsql_parser::ibatis::parse_mapper_bytes_structured(&input);
        let expand_ws = ogsql_parser::linter::structured::lint_structured_mapper(&structured, &config);
        ws.extend(expand_ws);
        ws
    } else {
        vec![]
    };

    if csv {
        output_csv_xml_header();
        output_csv_xml_rows(&result.statements, file_opt.unwrap_or("<stdin>"), ".");
    } else if cli.json {
        let mut out = serde_json::to_string_pretty(&result).unwrap();
        if !lint_warnings.is_empty() {
            let mut val: serde_json::Value = serde_json::from_str(&out).unwrap();
            val.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            val.as_object_mut().unwrap().insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
            out = serde_json::to_string_pretty(&val).unwrap();
        }
        println!("{}", out);
    } else {
        print_xml_text(&result);
        if !lint_warnings.is_empty() {
            eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
            format_warnings_text(&lint_warnings);
            eprintln!("── Summary ──");
            eprintln!("  Total: {} warnings", lint_warnings.len());
        }
    }
}

#[cfg(feature = "ibatis")]
fn cmd_parse_xml_structured(cli: &Cli, _csv: bool) {
    if cli.file.len() > 1 {
        die!("Error: parse-xml --structured accepts at most one --file");
    }
    let file_opt = cli.file.first().map(|s| s.as_str());
    let input = match file_opt {
        Some(path) => std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e)),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            buf
        }
    };

    let result = ogsql_parser::ibatis::parse_mapper_bytes_structured_with_path(&input, file_opt);

    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("Error: {:?}", err);
        }
    }

    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}

#[cfg(feature = "ibatis")]
fn ibatis_error_kind(err: &ogsql_parser::ibatis::error::IbatisError) -> &'static str {
    use ogsql_parser::ibatis::error::IbatisError;
    match err {
        IbatisError::XmlError { .. } => "XmlError",
        IbatisError::UnknownFragment { .. } => "UnknownFragment",
        IbatisError::CircularInclude { .. } => "CircularInclude",
        IbatisError::MissingAttribute { .. } => "MissingAttribute",
        IbatisError::EmptyMapper => "EmptyMapper",
        IbatisError::SqlParseError(_) => "SqlParseError",
        IbatisError::JdbcCallRequiresCallable { .. } => "JdbcCallRequiresCallable",
    }
}

#[cfg(feature = "ibatis")]
fn is_likely_mapper_xml(bytes: &[u8]) -> bool {
    let head = if bytes.len() > 4096 { &bytes[..4096] } else { bytes };
    // Match <mapper or <sqlMap followed by a non-identifier byte.
    // This avoids false positives on <mappers>, <sqlMapConfig>, etc.
    head.windows(8).any(|w| {
        (w[..7] == *b"<mapper" || w[..7] == *b"<sqlMap") && matches!(w[7], b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')
    }) || head.windows(16).any(|w| w == b"<!DOCTYPE mapper" || w == b"<!DOCTYPE sqlMap")
}

#[cfg(feature = "ibatis")]
fn cmd_parse_xml_dir(cli: &Cli, dir_path: &str, csv: bool, java_roots: &[std::path::PathBuf], stats: bool) {
    use std::path::Path;

    let root = Path::new(dir_path);
    if !root.is_dir() {
        die!("Error: '{}' is not a directory", dir_path);
    }

    let mut all_results: Vec<(String, String, ogsql_parser::ibatis::ParsedMapper)> = Vec::new();
    let mut all_expand_lint: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();

    for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("xml") {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };

        if !is_likely_mapper_xml(&bytes) {
            if cli.verbose {
                eprintln!("Skipping non-mapper XML: {}", path.display());
            }
            continue;
        }

        let rel_dir =
            path.parent().and_then(|p| p.strip_prefix(root).ok()).map(|p| p.to_str().unwrap_or(".")).unwrap_or(".");

        #[cfg(feature = "java")]
        let result = if java_roots.is_empty() {
            ogsql_parser::ibatis::parse_mapper_bytes_with_path(&bytes, Some(&path.to_string_lossy()))
        } else {
            ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(
                &bytes,
                Some(&path.to_string_lossy()),
                java_roots.to_vec(),
            )
        };
        #[cfg(not(feature = "java"))]
        let result = ogsql_parser::ibatis::parse_mapper_bytes_with_path(&bytes, Some(&path.to_string_lossy()));

        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        all_results.push((file_name, rel_dir.to_string(), result));

        if cli.lint {
            let config = build_lint_config(cli);
            let structured = ogsql_parser::ibatis::parse_mapper_bytes_structured(&bytes);
            all_expand_lint.extend(ogsql_parser::linter::structured::lint_structured_mapper(&structured, &config));
        }
    }

    // Stats accumulators
    let mut files_with_xml_errors: HashSet<String> = HashSet::new();
    let mut files_with_sql_errors: HashSet<String> = HashSet::new();
    let mut files_with_sql_warnings: HashSet<String> = HashSet::new();
    let mut mapper_stmt_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_sql_stmts = 0usize;
    let mut xml_error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut sql_error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut sql_warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();

    for (file_name, _rel_dir, result) in &all_results {
        if !result.errors.is_empty() {
            files_with_xml_errors.insert(file_name.clone());
        }
        for err in &result.errors {
            let kind = ibatis_error_kind(err);
            let entry = xml_error_kinds.entry(kind).or_insert((0, HashSet::new()));
            entry.0 += 1;
            entry.1.insert(file_name.clone());
        }
        for stmt in &result.statements {
            *mapper_stmt_counts.entry(format!("{:?}", stmt.kind)).or_insert(0) += 1;
            if let Some((infos, parse_errors)) = &stmt.parse_result {
                total_sql_stmts += infos.len();
                for perr in parse_errors {
                    let kind = parser_error_kind(perr);
                    let file_set = if is_warning(perr) { &mut sql_warning_kinds } else { &mut sql_error_kinds };
                    let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    entry.1.insert(file_name.clone());
                    if is_warning(perr) {
                        files_with_sql_warnings.insert(file_name.clone());
                    } else {
                        files_with_sql_errors.insert(file_name.clone());
                    }
                }
            }
        }
    }

    let total_mapper: usize = all_results.iter().map(|(_, _, r)| r.statements.len()).sum();

    if csv {
        output_csv_xml_header();
        for (file_name, rel_dir, result) in &all_results {
            output_csv_xml_rows(&result.statements, file_name, rel_dir);
        }
    } else if cli.json {
        let combined: Vec<serde_json::Value> = all_results
            .iter()
            .map(|(f, d, r)| {
                serde_json::json!({
                    "file": f,
                    "directory": d,
                    "namespace": r.namespace,
                    "statements": r.statements,
                    "errors": r.errors,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&combined).unwrap());
    } else {
        for (file_name, _rel_dir, result) in &all_results {
            if !result.errors.is_empty() {
                eprintln!("[{}] {} error(s):", file_name, result.errors.len());
                for e in &result.errors {
                    eprintln!("  {}", e);
                }
            }

            for stmt in &result.statements {
                println!("── {} ({:?}) [{} L{}] ──", stmt.id, stmt.kind, file_name, stmt.line);
                println!("{}", stmt.flat_sql.trim());
                if !stmt.parameters.is_empty() {
                    let typed: Vec<String> = stmt
                        .parameters
                        .iter()
                        .map(|p| match &p.jdbc_type {
                            Some(jt) => format!("{}:{:?}", p.name, jt),
                            None => p.name.clone(),
                        })
                        .collect();
                    println!("  [params: {}]", typed.join(", "));
                }
                if stmt.has_dynamic_elements {
                    println!("  [contains dynamic SQL elements]");
                }
                if let Some((infos, errors)) = &stmt.parse_result {
                    let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
                    let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
                    if !real_errors.is_empty() {
                        eprintln!("  {} parse error(s):", real_errors.len());
                        for e in &real_errors {
                            eprintln!("    {}", e);
                        }
                    }
                    if !warnings.is_empty() {
                        eprintln!("  {} warning(s):", warnings.len());
                        for w in &warnings {
                            eprintln!("    {}", w);
                        }
                    }
                    if real_errors.is_empty() {
                        println!(
                            "  ✓ Parsed successfully ({} statement(s)){}",
                            infos.len(),
                            if warnings.is_empty() { "" } else { " (with warnings)" }
                        );
                    }
                }
                println!();
            }
        }
        println!("Total: {} statement(s) from {} file(s)", total_mapper, all_results.len());

        if cli.lint {
            let config = build_lint_config(cli);
            let mut all_lint: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();
            for (_file_name, _rel_dir, result) in &all_results {
                all_lint.extend(lint_xml_statements(&result.statements, &config));
            }
            all_lint.extend(all_expand_lint);
            if !all_lint.is_empty() {
                eprintln!("\n── Lint Warnings ({}) ──", all_lint.len());
                format_warnings_text(&all_lint);
                eprintln!("── Summary ──");
                eprintln!("  Total: {} warnings", all_lint.len());
            }
        }
    }

    if stats {
        let total_xml_errors: usize = xml_error_kinds.values().map(|(c, _)| c).sum();
        let total_sql_err_count: usize = sql_error_kinds.values().map(|(c, _)| c).sum();
        let total_sql_warn_count: usize = sql_warning_kinds.values().map(|(c, _)| c).sum();

        stats_bar("");
        stats_bar("Summary / parse-xml");
        stats_bar("");

        eprintln!("  {:<28} {}", "XML files processed:", all_results.len());
        eprintln!("  {:<28} {}", "Total mapper statements:", total_mapper);
        let max_kind = mapper_stmt_counts.keys().map(|k| k.len()).max().unwrap_or(10);
        for (kind, count) in &mapper_stmt_counts {
            let pct = if total_mapper > 0 { *count as f64 / total_mapper as f64 * 100.0 } else { 0.0 };
            eprintln!("    {:width$} {:>6} ({:>5.1}%)", kind, count, pct, width = max_kind + 4);
        }
        if total_sql_stmts > 0 {
            eprintln!("  {:<28} {}", "SQL statements (parsed):", total_sql_stmts);
        }
        eprintln!();

        if total_xml_errors > 0 {
            stats_bar("XML error breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_xml_errors, files_with_xml_errors.len());
            for (kind, (cnt, files)) in &xml_error_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
        if total_sql_err_count > 0 {
            stats_bar("SQL parse error breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_sql_err_count, files_with_sql_errors.len());
            for (kind, (cnt, files)) in &sql_error_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
        if total_sql_warn_count > 0 {
            stats_bar("SQL warning breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_sql_warn_count, files_with_sql_warnings.len());
            for (kind, (cnt, files)) in &sql_warning_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
    }
}

#[cfg(feature = "ibatis")]
fn print_xml_text(result: &ogsql_parser::ibatis::ParsedMapper) {
    if !result.errors.is_empty() {
        eprintln!("{} error(s):", result.errors.len());
        for e in &result.errors {
            eprintln!("  {}", e);
        }
    }

    for stmt in &result.statements {
        println!("── {} ({:?}) ──", stmt.id, stmt.kind);
        println!("{}", stmt.flat_sql.trim());
        if !stmt.parameters.is_empty() {
            let typed: Vec<String> = stmt
                .parameters
                .iter()
                .map(|p| match &p.jdbc_type {
                    Some(jt) => format!("{}:{:?}", p.name, jt),
                    None => p.name.clone(),
                })
                .collect();
            println!("  [params: {}]", typed.join(", "));
        }
        if stmt.has_dynamic_elements {
            println!("  [contains dynamic SQL elements]");
        }
        if let Some((infos, errors)) = &stmt.parse_result {
            let warnings: Vec<_> = errors.iter().filter(|e| is_warning(e)).collect();
            let real_errors: Vec<_> = errors.iter().filter(|e| !is_warning(e)).collect();
            if !real_errors.is_empty() {
                eprintln!("  {} parse error(s):", real_errors.len());
                for e in &real_errors {
                    eprintln!("    {}", e);
                }
            }
            if !warnings.is_empty() {
                eprintln!("  {} warning(s):", warnings.len());
                for w in &warnings {
                    eprintln!("    {}", w);
                }
            }
            if real_errors.is_empty() {
                println!(
                    "  ✓ Parsed successfully ({} statement(s)){}",
                    infos.len(),
                    if warnings.is_empty() { "" } else { " (with warnings)" }
                );
            }
        }
        println!();
    }

    println!("Total: {} statement(s) in namespace '{}'", result.statements.len(), result.namespace);
}

// ---- cmd_validate_xml / validate-xml CLI handlers ----
// validate-xml 命令处理函数：解析、语义校验、lint

#[cfg(feature = "ibatis")]
/// Validate a single iBatis/MyBatis XML mapper file — parse + semantic checks + lint
/// 校验单个 iBatis/MyBatis XML mapper 文件（解析 + 语义校验 + lint）
fn cmd_validate_xml_single(cli: &Cli, csv: bool, java_roots: &[std::path::PathBuf], strict: bool) {
    if cli.file.len() > 1 {
        die!("Error: validate-xml command accepts at most one --file");
    }
    let file_opt = cli.file.first().map(|s| s.as_str());
    let input = match file_opt {
        Some(path) => std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e)),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            buf
        }
    };

    #[cfg(feature = "java")]
    let result = if java_roots.is_empty() {
        ogsql_parser::ibatis::parse_mapper_bytes_with_path(&input, file_opt)
    } else {
        ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(&input, file_opt, java_roots.to_vec())
    };
    #[cfg(not(feature = "java"))]
    let _ = java_roots;
    #[cfg(not(feature = "java"))]
    let result = ogsql_parser::ibatis::parse_mapper_bytes_with_path(&input, file_opt);

    let file_name = file_opt.unwrap_or("<stdin>");

    // 收集所有 StatementInfo 和 ParserError
    let mut all_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
    let mut all_errors: Vec<ogsql_parser::ParserError> = Vec::new();
    for stmt in &result.statements {
        if let Some((ref infos, ref parse_errors)) = stmt.parse_result {
            all_stmts.extend(infos.iter().cloned());
            all_errors.extend(parse_errors.iter().cloned());
        }
    }

    // Convert XML parse errors to ParserError for unified output
    // 将 XML 解析错误转换为 ParserError 以便统一输出
    for e in &result.errors {
        all_errors.push(ogsql_parser::ParserError::UnexpectedToken {
            location: ogsql_parser::SourceLocation::default(),
            expected: "valid XML mapper".to_string(),
            got: format!("{}", e),
        });
    }

    // Run validation pipeline: PACKAGE consistency, MERGE semantics, PL variable validation
    // 运行校验管线：PACKAGE 一致性、MERGE 语义、PL 变量校验
    let (core_errors, pkg_errors, var_errors) = validate_from_stmts(&all_stmts, &[], strict);

    // Lint with Confidence::Full
    // 使用 Full 置信度运行 lint
    let config = build_lint_config(cli);
    let mut lint_warnings: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();
    if !all_stmts.is_empty() {
        let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
        let ws = linter.lint(&all_stmts, None, ogsql_parser::linter::Confidence::Full);
        lint_warnings.extend(ws);
    }
    // Also run lint_xml_expanded for C018 rule (foreach in INSERT VALUES)
    // 同时运行 lint_xml_expanded 检测 C018 规则（INSERT VALUES 中的 foreach）
    {
        let structured = ogsql_parser::ibatis::parse_mapper_bytes_structured(&input);
        let expand_ws = ogsql_parser::linter::structured::lint_structured_mapper(&structured, &config);
        lint_warnings.extend(expand_ws);
    }

    let format_var_err = |ve: &ogsql_parser::UndefinedVariableError| -> String {
        let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
        let kind_label = match ve.kind {
            ogsql_parser::UndefinedRefKind::Function => "undefined function",
            ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
        };
        format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info)
    };

    if csv {
        // CSV output: one row per extracted statement
        println!("file,directory,line,type,name,parent,parameters,return_type,sql,valid,error_count,warning_count,errors,warnings");
        for stmt in &result.statements {
            // Use parse_result directly (not filtered by error_line) to correctly attribute
            // per-statement errors without N× amplification or silent drop.
            let (stmt_real_errors, stmt_warnings): (Vec<&ogsql_parser::ParserError>, Vec<&ogsql_parser::ParserError>) =
                match &stmt.parse_result {
                    Some((_, parse_errors)) => {
                        let errs = parse_errors.iter().filter(|e| !is_warning(e)).collect();
                        let warns = parse_errors.iter().filter(|e| is_warning(e)).collect();
                        (errs, warns)
                    }
                    None => (vec![], vec![]),
                };
            let err_msgs: Vec<String> = stmt_real_errors.iter().map(|e| format!("{}", e)).collect();
            let warn_msgs: Vec<String> = stmt_warnings.iter().map(|e| format!("{}", e)).collect();
            println!(
                "{},.,{},{},{},{},,,{},{},{},{},{},{}",
                csv_escape(file_name),
                stmt.line,
                format!("{:?}", stmt.kind),
                csv_escape(&stmt.id),
                csv_escape(&result.namespace),
                csv_escape(&stmt.flat_sql.trim().replace('\r', "")),
                if stmt_real_errors.is_empty() { "VALID" } else { "INVALID" },
                stmt_real_errors.len(),
                stmt_warnings.len(),
                csv_escape(&err_msgs.join("; ")),
                csv_escape(&warn_msgs.join("; ")),
            );
        }
        // Global-level errors: XML parse errors + validation errors (MERGE, PACKAGE, etc.)
        // These don't belong to any single statement's parse_result.
        // Emit a line=0 summary row if any exist.
        let global_err_msgs: Vec<String> = result
            .errors
            .iter()
            .map(|e| format!("{}", e))
            .chain(core_errors.iter().filter(|e| !is_warning(e)).map(|e| format!("{}", e)))
            .chain(var_errors.iter().map(&format_var_err))
            .collect();
        let global_warn_msgs: Vec<String> =
            core_errors.iter().filter(|e| is_warning(e)).map(|e| format!("{}", e)).collect();
        if !global_err_msgs.is_empty() || !global_warn_msgs.is_empty() {
            let has_global_real = !global_err_msgs.is_empty();
            println!(
                "{},.,0,,,,,,,{},{},{},{},{}",
                csv_escape(file_name),
                if has_global_real { "INVALID" } else { "VALID" },
                global_err_msgs.len(),
                global_warn_msgs.len(),
                csv_escape(&global_err_msgs.join("; ")),
                csv_escape(&global_warn_msgs.join("; ")),
            );
        }
        // Compute overall status for exit code: include validation errors (core_errors)
        // and per-statement parse errors (already in all_errors).
        all_errors.extend(core_errors);
        let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
        let has_global_or_parse_real = !real_errors.is_empty() || !var_errors.is_empty();
        if has_global_or_parse_real {
            std::process::exit(1);
        }
        return;
    }

    // Text mode below: extend all_errors with validation errors before summary computation
    all_errors.extend(core_errors);

    let warnings: Vec<_> = all_errors.iter().filter(|e| is_warning(e)).collect();
    let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
    let has_errors = !real_errors.is_empty() || !var_errors.is_empty();

    if cli.json {
        let mut out = serde_json::json!({
            "valid": !has_errors,
            "error_count": real_errors.len() + var_errors.len(),
            "warning_count": warnings.len(),
            "errors": all_errors,
            "statements": result.statements,
        });
        if strict {
            out.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
        }
        if !pkg_errors.is_empty() {
            out.as_object_mut()
                .unwrap()
                .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
        }
        if !var_errors.is_empty() {
            out.as_object_mut().unwrap().insert("undefined_variables".to_string(), serde_json::json!(var_errors));
        }
        if !lint_warnings.is_empty() {
            out.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            out.as_object_mut().unwrap().insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        // Text output: print each statement info then VALID/INVALID
        // 文本输出：打印每条语句信息，然后输出 VALID/INVALID
        if !result.errors.is_empty() {
            eprintln!("XML error(s):");
            for e in &result.errors {
                eprintln!("  {}", e);
            }
        }

        for stmt in &result.statements {
            println!("── {} ({:?}) ──", stmt.id, stmt.kind);
            println!("{}", stmt.flat_sql.trim());
            if !stmt.parameters.is_empty() {
                let typed: Vec<String> = stmt
                    .parameters
                    .iter()
                    .map(|p| match &p.jdbc_type {
                        Some(jt) => format!("{}:{:?}", p.name, jt),
                        None => p.name.clone(),
                    })
                    .collect();
                println!("  [params: {}]", typed.join(", "));
            }
            if stmt.has_dynamic_elements {
                println!("  [contains dynamic SQL elements]");
            }
            // Show parse errors for this statement from parse_result directly
            let stmt_real_errors: Vec<&ogsql_parser::ParserError> = match &stmt.parse_result {
                Some((_, parse_errors)) => parse_errors.iter().filter(|e| !is_warning(e)).collect(),
                None => vec![],
            };
            if !stmt_real_errors.is_empty() {
                eprintln!("  {} parse error(s):", stmt_real_errors.len());
                for e in &stmt_real_errors {
                    eprintln!("    {}", e);
                }
            }
            println!();
        }

        // Print VALID/INVALID summary
        // 输出 VALID/INVALID 汇总
        if has_errors {
            let total = real_errors.len() + var_errors.len();
            if strict {
                println!("INVALID ({} error(s), {} warning(s)) [strict mode]:", total, warnings.len());
            } else {
                println!("INVALID ({} error(s), {} warning(s)):", total, warnings.len());
            }
            for e in &real_errors {
                eprintln!("  error: {}", e);
            }
            for ve in &var_errors {
                eprintln!("  error: {}", format_var_err(ve));
            }
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        } else if !warnings.is_empty() {
            println!("VALID ({} warning(s)):", warnings.len());
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        } else {
            println!("VALID");
        }

        if !lint_warnings.is_empty() {
            eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
            format_warnings_text(&lint_warnings);
            eprintln!("── Summary ──");
            eprintln!("  Total: {} warnings", lint_warnings.len());
        }

        if has_errors {
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "ibatis")]
/// Validate iBatis/MyBatis XML mappers in a directory — batch validation with stats
/// 批量校验目录中的 iBatis/MyBatis XML mapper 文件（含统计信息）
fn cmd_validate_xml_dir(
    cli: &Cli,
    dir_path: &str,
    csv: bool,
    java_roots: &[std::path::PathBuf],
    stats: bool,
    strict: bool,
) {
    use std::path::Path;

    let root = Path::new(dir_path);
    if !root.is_dir() {
        die!("Error: '{}' is not a directory", dir_path);
    }

    let mut files_processed: Vec<(
        String,
        String,
        Vec<ogsql_parser::ParserError>,
        Vec<ogsql_parser::UndefinedVariableError>,
        Vec<ogsql_parser::PackageConsistencyError>,
        Vec<ogsql_parser::linter::SqlWarning>,
        bool,
    )> = Vec::new();
    let mut any_invalid = false;

    // Track stats accumulators
    // 统计信息累加器
    let mut total_files = 0usize;
    let mut total_errors = 0usize;
    let mut total_warnings = 0usize;
    let mut files_with_errors: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut files_with_warnings: std::collections::HashSet<String> = std::collections::HashSet::new();

    if csv {
        println!("file,directory,line,type,name,parent,parameters,return_type,sql,valid,error_count,warning_count,errors,warnings");
    }

    for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("xml") {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };

        if !is_likely_mapper_xml(&bytes) {
            if cli.verbose {
                eprintln!("Skipping non-mapper XML: {}", path.display());
            }
            continue;
        }

        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let rel_dir = path
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| {
                let s = p.to_str().unwrap_or(".");
                if s.is_empty() {
                    "."
                } else {
                    s
                }
            })
            .unwrap_or(".")
            .to_string();

        #[cfg(feature = "java")]
        let result = if java_roots.is_empty() {
            ogsql_parser::ibatis::parse_mapper_bytes_with_path(&bytes, Some(&path.to_string_lossy()))
        } else {
            ogsql_parser::ibatis::parse_mapper_bytes_with_java_src(
                &bytes,
                Some(&path.to_string_lossy()),
                java_roots.to_vec(),
            )
        };
        #[cfg(not(feature = "java"))]
        let _ = java_roots;
        #[cfg(not(feature = "java"))]
        let result = ogsql_parser::ibatis::parse_mapper_bytes_with_path(&bytes, Some(&path.to_string_lossy()));

        let mut all_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
        let mut all_errors: Vec<ogsql_parser::ParserError> = Vec::new();

        for stmt in &result.statements {
            if let Some((ref infos, ref parse_errors)) = stmt.parse_result {
                all_stmts.extend(infos.iter().cloned());
                all_errors.extend(parse_errors.iter().cloned());
            }
        }

        for e in &result.errors {
            all_errors.push(ogsql_parser::ParserError::UnexpectedToken {
                location: ogsql_parser::SourceLocation::default(),
                expected: "valid XML mapper".to_string(),
                got: format!("{}", e),
            });
        }

        let (core_errors, pkg_errors, var_errors) = validate_from_stmts(&all_stmts, &[], strict);

        let config = build_lint_config(cli);
        let mut lint_warnings: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();
        if !all_stmts.is_empty() {
            let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config.clone());
            let ws = linter.lint(&all_stmts, None, ogsql_parser::linter::Confidence::Full);
            lint_warnings.extend(ws);
        }
        {
            let structured = ogsql_parser::ibatis::parse_mapper_bytes_structured(&bytes);
            let expand_ws = ogsql_parser::linter::structured::lint_structured_mapper(&structured, &config);
            lint_warnings.extend(expand_ws);
        }

        let format_var_err = |ve: &ogsql_parser::UndefinedVariableError| -> String {
            let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
            let kind_label = match ve.kind {
                ogsql_parser::UndefinedRefKind::Function => "undefined function",
                ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
            };
            format!("{} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info)
        };

        if csv {
            for stmt in &result.statements {
                let (stmt_real_errors, stmt_warnings): (
                    Vec<&ogsql_parser::ParserError>,
                    Vec<&ogsql_parser::ParserError>,
                ) = match &stmt.parse_result {
                    Some((_, parse_errors)) => {
                        let errs = parse_errors.iter().filter(|e| !is_warning(e)).collect();
                        let warns = parse_errors.iter().filter(|e| is_warning(e)).collect();
                        (errs, warns)
                    }
                    None => (vec![], vec![]),
                };
                let err_msgs: Vec<String> = stmt_real_errors.iter().map(|e| format!("{}", e)).collect();
                let warn_msgs: Vec<String> = stmt_warnings.iter().map(|e| format!("{}", e)).collect();
                println!(
                    "{},{},{},{},{},{},,,{},{},{},{},{},{}",
                    csv_escape(&file_name),
                    csv_escape(&rel_dir),
                    stmt.line,
                    format!("{:?}", stmt.kind),
                    csv_escape(&stmt.id),
                    csv_escape(&result.namespace),
                    csv_escape(&stmt.flat_sql.trim().replace('\r', "")),
                    if stmt_real_errors.is_empty() { "VALID" } else { "INVALID" },
                    stmt_real_errors.len(),
                    stmt_warnings.len(),
                    csv_escape(&err_msgs.join("; ")),
                    csv_escape(&warn_msgs.join("; ")),
                );
            }
            // Global-level errors: XML parse errors + validation errors (MERGE, PACKAGE)
            let global_err_msgs: Vec<String> = result
                .errors
                .iter()
                .map(|e| format!("{}", e))
                .chain(core_errors.iter().filter(|e| !is_warning(e)).map(|e| format!("{}", e)))
                .chain(var_errors.iter().map(&format_var_err))
                .collect();
            let global_warn_msgs: Vec<String> =
                core_errors.iter().filter(|e| is_warning(e)).map(|e| format!("{}", e)).collect();
            let has_global_real = !global_err_msgs.is_empty();
            if !global_err_msgs.is_empty() || !global_warn_msgs.is_empty() {
                println!(
                    "{},{},0,,,,,,,{},{},{},{},{}",
                    csv_escape(&file_name),
                    csv_escape(&rel_dir),
                    if has_global_real { "INVALID" } else { "VALID" },
                    global_err_msgs.len(),
                    global_warn_msgs.len(),
                    csv_escape(&global_err_msgs.join("; ")),
                    csv_escape(&global_warn_msgs.join("; ")),
                );
            }
            // Extend and compute stats with validation errors included
            all_errors.extend(core_errors);
            let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = all_errors.iter().filter(|e| is_warning(e)).collect();
            let has_errors = !real_errors.is_empty() || !var_errors.is_empty();
            total_files += 1;
            if has_errors {
                any_invalid = true;
                files_with_errors.insert(file_name.clone());
            }
            if !warnings.is_empty() {
                files_with_warnings.insert(file_name.clone());
            }
            total_errors += real_errors.len() + var_errors.len();
            total_warnings += warnings.len();
        } else {
            // Non-CSV: extend core_errors, compute stats, then output
            all_errors.extend(core_errors);
            let warnings: Vec<_> = all_errors.iter().filter(|e| is_warning(e)).collect();
            let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
            let has_errors = !real_errors.is_empty() || !var_errors.is_empty();

            total_files += 1;
            if has_errors {
                any_invalid = true;
                files_with_errors.insert(file_name.clone());
            }
            if !warnings.is_empty() {
                files_with_warnings.insert(file_name.clone());
            }
            total_errors += real_errors.len() + var_errors.len();
            total_warnings += warnings.len();

            if cli.json {
                files_processed.push((
                    file_name,
                    rel_dir,
                    all_errors,
                    var_errors,
                    pkg_errors,
                    lint_warnings,
                    has_errors,
                ));
            } else {
                // Text output per file
                if has_errors {
                    println!(
                        "[{}/{}] INVALID ({} error(s), {} warning(s))",
                        rel_dir,
                        file_name,
                        real_errors.len() + var_errors.len(),
                        warnings.len()
                    );
                    for e in &real_errors {
                        eprintln!("  error: {}", e);
                    }
                    for ve in &var_errors {
                        eprintln!("  error: {}", format_var_err(ve));
                    }
                    for w in &warnings {
                        eprintln!("  warning: {}", w);
                    }
                } else if !warnings.is_empty() {
                    println!("[{}/{}] VALID ({} warning(s))", rel_dir, file_name, warnings.len());
                    for w in &warnings {
                        eprintln!("  warning: {}", w);
                    }
                } else {
                    println!("[{}/{}] VALID", rel_dir, file_name);
                }
                // Also print statement details if verbose
                if cli.verbose && !result.statements.is_empty() {
                    for stmt in &result.statements {
                        println!("  ── {} ({:?}) ──", stmt.id, stmt.kind);
                        println!("  {}", stmt.flat_sql.trim());
                    }
                }
            }
        }
    }

    if csv {
        if any_invalid {
            std::process::exit(1);
        }
        return;
    }

    if cli.json {
        let mut results: Vec<serde_json::Value> = Vec::new();
        for (file_name, rel_dir, all_errors, var_errors, pkg_errors, lint_warnings, has_err) in &files_processed {
            let real_errors_count = all_errors.iter().filter(|e| !is_warning(e)).count();
            let warnings_count = all_errors.iter().filter(|e| is_warning(e)).count();
            let mut file_result = serde_json::json!({
                "file": file_name,
                "directory": rel_dir,
                "valid": !has_err,
                "error_count": real_errors_count + var_errors.len(),
                "warning_count": warnings_count,
                "errors": all_errors,
            });
            if !pkg_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("package_consistency_errors".to_string(), serde_json::json!(pkg_errors));
            }
            if !var_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("undefined_variables".to_string(), serde_json::json!(var_errors));
            }
            if !lint_warnings.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            }
            if strict {
                file_result.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
            }
            results.push(file_result);
        }
        let mut out = serde_json::json!({
            "valid": !any_invalid,
            "total_files": total_files,
            "total_errors": total_errors,
            "total_warnings": total_warnings,
            "files": results,
        });
        if strict {
            out.as_object_mut().unwrap().insert("strict_mode".to_string(), serde_json::json!(true));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else if stats {
        stats_bar("");
        stats_bar("Summary / validate-xml");
        stats_bar("");
        eprintln!("  {:<28} {}", "XML files validated:", total_files);
        eprintln!("  {:<28} {}", "Files with errors:", files_with_errors.len());
        eprintln!("  {:<28} {}", "Files with warnings:", files_with_warnings.len());
        eprintln!("  {:<28} {}", "Total errors:", total_errors);
        eprintln!("  {:<28} {}", "Total warnings:", total_warnings);
        eprintln!();
    }

    if total_files == 0 {
        eprintln!("No .xml files found in '{}'", dir_path);
    }

    if any_invalid {
        if stats {
            print!(
                "Result: INVALID — {} error(s), {} warning(s) from {} file(s)",
                total_errors, total_warnings, total_files
            );
            if strict {
                println!(" (--strict mode enabled)");
            } else {
                println!();
            }
        }
        std::process::exit(1);
    } else if total_files > 0 && stats {
        if total_warnings > 0 {
            println!("Result: VALID — {} warning(s) from {} file(s)", total_warnings, total_files);
        } else {
            println!("Result: VALID — {} file(s)", total_files);
        }
    }
}

#[cfg(feature = "ibatis")]
/// Entry dispatcher for validate-xml — route to single or directory handler
/// validate-xml 入口分发函数：路由到单文件或目录处理函数
fn cmd_validate_xml(cli: &Cli, dir: Option<&str>, csv: bool, java_src: Option<&str>, stats: bool, strict: bool) {
    // Build java_roots the same way as cmd_parse_xml
    // 与 cmd_parse_xml 相同的 java_roots 构建逻辑
    #[cfg(feature = "java")]
    let java_roots: Vec<std::path::PathBuf> = match java_src {
        Some(path) => {
            let p = std::path::Path::new(path);
            if !p.is_dir() {
                die!("Error: '{}' is not a directory", path);
            }
            vec![p.to_path_buf()]
        }
        None => {
            let scan_dir = dir.map(std::path::Path::new).unwrap_or_else(|| std::path::Path::new("."));
            let detected = ogsql_parser::ibatis::detect_java_roots(scan_dir);
            if !detected.is_empty() {
                eprintln!("Auto-detected Java source roots:");
                for r in &detected {
                    eprintln!("  {}", r.display());
                }
            }
            detected
        }
    };
    #[cfg(not(feature = "java"))]
    let _ = java_src;
    #[cfg(not(feature = "java"))]
    let java_roots: Vec<std::path::PathBuf> = Vec::new();

    if let Some(dir_path) = dir {
        cmd_validate_xml_dir(cli, dir_path, csv, &java_roots, stats, strict);
    } else {
        cmd_validate_xml_single(cli, csv, &java_roots, strict);
    }
}

#[cfg(feature = "java")]
fn cmd_parse_java(
    cli: &Cli,
    extra_sql_methods: &[String],
    extra_sql_var_patterns: &[String],
    dir: Option<&str>,
    csv: bool,
    stats: bool,
) {
    match dir {
        Some(dir_path) => cmd_parse_java_dir(cli, extra_sql_methods, extra_sql_var_patterns, dir_path, csv, stats),
        None => cmd_parse_java_single(cli, extra_sql_methods, extra_sql_var_patterns, csv),
    }
}

#[cfg(feature = "java")]
fn cmd_parse_java_single(cli: &Cli, extra_sql_methods: &[String], extra_sql_var_patterns: &[String], csv: bool) {
    if cli.file.len() > 1 {
        die!("Error: parse-java command accepts at most one --file");
    }
    let file_opt = cli.file.first().map(|s| s.as_str());
    let (source, file_path) = match file_opt {
        Some(path) => {
            let bytes = std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e));
            let (text, _encoding) =
                ogsql_parser::token::decode_sql_file(&bytes).unwrap_or_else(|e| die!("Error decoding {}: {}", path, e));
            (text, path.to_string())
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            (buf, "<stdin>".to_string())
        }
    };

    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: extra_sql_methods.to_vec(),
        extra_sql_var_patterns: extra_sql_var_patterns.to_vec(),
    };
    let result = ogsql_parser::java::extract_sql_from_java(&source, &file_path, &config);

    let lint_warnings = if cli.lint {
        let config = build_lint_config(cli);
        lint_java_extractions(&result.extractions, &config)
    } else {
        vec![]
    };

    if csv {
        output_csv_java_header();
        output_csv_java_rows(&result.extractions, &file_path, ".");
    } else if cli.json {
        let mut out = serde_json::to_string_pretty(&result).unwrap();
        if !lint_warnings.is_empty() {
            let mut val: serde_json::Value = serde_json::from_str(&out).unwrap();
            val.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            val.as_object_mut().unwrap().insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
            out = serde_json::to_string_pretty(&val).unwrap();
        }
        println!("{}", out);
    } else {
        print_java_text(&result, &file_path);
        if !lint_warnings.is_empty() {
            eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
            format_warnings_text(&lint_warnings);
            eprintln!("── Summary ──");
            eprintln!("  Total: {} warnings", lint_warnings.len());
        }
    }
}

#[cfg(feature = "java")]
fn java_error_kind(err: &ogsql_parser::java::error::JavaError) -> &'static str {
    use ogsql_parser::java::error::JavaError;
    match err {
        JavaError::ParseError { .. } => "ParseError",
        JavaError::SqlParseError { .. } => "SqlParseError",
        JavaError::IoError(_) => "IoError",
        JavaError::EncodingError(_) => "EncodingError",
    }
}

#[cfg(feature = "java")]
fn cmd_parse_java_dir(
    cli: &Cli,
    extra_sql_methods: &[String],
    extra_sql_var_patterns: &[String],
    dir_path: &str,
    csv: bool,
    stats: bool,
) {
    use std::path::Path;

    let root = Path::new(dir_path);
    if !root.is_dir() {
        die!("Error: '{}' is not a directory", dir_path);
    }

    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: extra_sql_methods.to_vec(),
        extra_sql_var_patterns: extra_sql_var_patterns.to_vec(),
    };

    let mut all_results: Vec<(String, String, ogsql_parser::java::JavaExtractResult)> = Vec::new();

    for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("java") {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };

        let (source, _encoding) = match ogsql_parser::token::decode_sql_file(&bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };

        let file_path_str = path.to_string_lossy().to_string();
        let result = ogsql_parser::java::extract_sql_from_java(&source, &file_path_str, &config);

        let rel_dir =
            path.parent().and_then(|p| p.strip_prefix(root).ok()).map(|p| p.to_str().unwrap_or(".")).unwrap_or(".");

        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        all_results.push((file_name, rel_dir.to_string(), result));
    }

    // Stats accumulators
    let mut files_with_java_errors: HashSet<String> = HashSet::new();
    let mut files_with_sql_errors: HashSet<String> = HashSet::new();
    let mut files_with_sql_warnings: HashSet<String> = HashSet::new();
    let mut total_sql_stmts = 0usize;
    let mut java_error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut sql_error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut sql_warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut extraction_method_counts: BTreeMap<String, usize> = BTreeMap::new();

    for (file_name, _rel_dir, result) in &all_results {
        if !result.errors.is_empty() {
            files_with_java_errors.insert(file_name.clone());
        }
        for err in &result.errors {
            let kind = java_error_kind(err);
            let entry = java_error_kinds.entry(kind).or_insert((0, HashSet::new()));
            entry.0 += 1;
            entry.1.insert(file_name.clone());
        }
        for ext in &result.extractions {
            *extraction_method_counts.entry(format!("{:?}", ext.origin.method)).or_insert(0) += 1;
            if let Some(parse_result) = &ext.parse_result {
                total_sql_stmts += parse_result.statements.len();
                for perr in &parse_result.errors {
                    let kind = parser_error_kind(perr);
                    let file_set = if is_warning(perr) { &mut sql_warning_kinds } else { &mut sql_error_kinds };
                    let entry = file_set.entry(kind).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    entry.1.insert(file_name.clone());
                    if is_warning(perr) {
                        files_with_sql_warnings.insert(file_name.clone());
                    } else {
                        files_with_sql_errors.insert(file_name.clone());
                    }
                }
            }
        }
    }

    if csv {
        output_csv_java_header();
        for (file_name, rel_dir, result) in &all_results {
            output_csv_java_rows(&result.extractions, file_name, rel_dir);
        }
    } else if cli.json {
        let combined: Vec<serde_json::Value> = all_results
            .iter()
            .map(|(f, d, r)| {
                serde_json::json!({
                    "file": f,
                    "directory": d,
                    "extractions": r.extractions,
                    "errors": r.errors,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&combined).unwrap());
    } else {
        let mut total = 0usize;
        for (file_name, _rel_dir, result) in &all_results {
            if !result.errors.is_empty() {
                eprintln!("[{}] {} error(s):", file_name, result.errors.len());
                for e in &result.errors {
                    eprintln!("  {}", e);
                }
            }

            for ext in &result.extractions {
                let location = match &ext.origin.class_name {
                    Some(cls) => format!("{}::{}", cls, ext.origin.method_name.as_deref().unwrap_or("")),
                    None => file_name.clone(),
                };
                println!(
                    "── {:?} [{:?}] @ {} L{} [{}] ──",
                    ext.origin.method, ext.sql_kind, location, ext.origin.line, file_name
                );
                println!("{}", ext.sql.trim());
                if ext.is_concatenated {
                    println!("  [concatenated]");
                }
                if ext.is_text_block {
                    println!("  [text block]");
                }
                if ext.parameter_style != ogsql_parser::java::ParameterStyle::None {
                    println!("  [params: {:?}]", ext.parameter_style);
                }
                if let Some(parse_result) = &ext.parse_result {
                    let warnings: Vec<_> = parse_result.errors.iter().filter(|e| is_warning(e)).collect();
                    let real_errors: Vec<_> = parse_result.errors.iter().filter(|e| !is_warning(e)).collect();
                    if !real_errors.is_empty() {
                        eprintln!("  {} parse error(s):", real_errors.len());
                        for e in &real_errors {
                            eprintln!("    {}", e);
                        }
                    }
                    if !warnings.is_empty() {
                        eprintln!("  {} warning(s):", warnings.len());
                        for w in &warnings {
                            eprintln!("    {}", w);
                        }
                    }
                    if real_errors.is_empty() {
                        println!(
                            "  ✓ Parsed successfully ({} statement(s)){}",
                            parse_result.statements.len(),
                            if warnings.is_empty() { "" } else { " (with warnings)" }
                        );
                    }
                }
                println!();
            }
            total += result.extractions.len();
        }
        println!("Total: {} extraction(s) from {} file(s)", total, all_results.len());

        if cli.lint {
            let config = build_lint_config(cli);
            let mut all_lint: Vec<ogsql_parser::linter::SqlWarning> = Vec::new();
            for (_file_name, _rel_dir, result) in &all_results {
                all_lint.extend(lint_java_extractions(&result.extractions, &config));
            }
            if !all_lint.is_empty() {
                eprintln!("\n── Lint Warnings ({}) ──", all_lint.len());
                format_warnings_text(&all_lint);
                eprintln!("── Summary ──");
                eprintln!("  Total: {} warnings", all_lint.len());
            }
        }
    }

    if stats {
        let total_extractions: usize = all_results.iter().map(|(_, _, r)| r.extractions.len()).sum();
        let total_java_errors: usize = java_error_kinds.values().map(|(c, _)| c).sum();
        let total_sql_err_count: usize = sql_error_kinds.values().map(|(c, _)| c).sum();
        let total_sql_warn_count: usize = sql_warning_kinds.values().map(|(c, _)| c).sum();

        stats_bar("");
        stats_bar("Summary / parse-java");
        stats_bar("");

        eprintln!("  {:<28} {}", "Java files processed:", all_results.len());
        eprintln!("  {:<28} {}", "Total extractions:", total_extractions);
        if !extraction_method_counts.is_empty() {
            let max_kind = extraction_method_counts.keys().map(|k| k.len()).max().unwrap_or(10);
            for (kind, count) in &extraction_method_counts {
                let pct = if total_extractions > 0 { *count as f64 / total_extractions as f64 * 100.0 } else { 0.0 };
                eprintln!("    {:width$} {:>6} ({:>5.1}%)", kind, count, pct, width = max_kind + 4);
            }
        }
        if total_sql_stmts > 0 {
            eprintln!("  {:<28} {}", "SQL statements (parsed):", total_sql_stmts);
        }
        eprintln!();

        if total_java_errors > 0 {
            stats_bar("Java error breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_java_errors, files_with_java_errors.len());
            for (kind, (cnt, files)) in &java_error_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
        if total_sql_err_count > 0 {
            stats_bar("SQL parse error breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_sql_err_count, files_with_sql_errors.len());
            for (kind, (cnt, files)) in &sql_error_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
        if total_sql_warn_count > 0 {
            stats_bar("SQL warning breakdown");
            eprintln!("  Total: {} (in {} file(s))", total_sql_warn_count, files_with_sql_warnings.len());
            for (kind, (cnt, files)) in &sql_warning_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
    }
}

#[cfg(feature = "java")]
fn print_java_text(result: &ogsql_parser::java::JavaExtractResult, file_path: &str) {
    if !result.errors.is_empty() {
        eprintln!("{} error(s):", result.errors.len());
        for e in &result.errors {
            eprintln!("  {}", e);
        }
    }

    if result.extractions.is_empty() {
        println!("No SQL statements found in {}", file_path);
    }

    for ext in &result.extractions {
        let location = match &ext.origin.class_name {
            Some(cls) => format!("{}::{}", cls, ext.origin.method_name.as_deref().unwrap_or("")),
            None => file_path.to_string(),
        };
        println!("── {:?} [{:?}] @ {} L{} ──", ext.origin.method, ext.sql_kind, location, ext.origin.line);
        println!("{}", ext.sql.trim());
        if ext.is_concatenated {
            println!("  [concatenated]");
        }
        if ext.is_text_block {
            println!("  [text block]");
        }
        if ext.parameter_style != ogsql_parser::java::ParameterStyle::None {
            println!("  [params: {:?}]", ext.parameter_style);
        }
        if let Some(parse_result) = &ext.parse_result {
            let warnings: Vec<_> = parse_result.errors.iter().filter(|e| is_warning(e)).collect();
            let real_errors: Vec<_> = parse_result.errors.iter().filter(|e| !is_warning(e)).collect();
            if !real_errors.is_empty() {
                eprintln!("  {} parse error(s):", real_errors.len());
                for e in &real_errors {
                    eprintln!("    {}", e);
                }
            }
            if !warnings.is_empty() {
                eprintln!("  {} warning(s):", warnings.len());
                for w in &warnings {
                    eprintln!("    {}", w);
                }
            }
            if real_errors.is_empty() {
                println!(
                    "  ✓ Parsed successfully ({} statement(s)){}",
                    parse_result.statements.len(),
                    if warnings.is_empty() { "" } else { " (with warnings)" }
                );
            }
        }
        println!();
    }

    println!("Total: {} extraction(s) from {}", result.extractions.len(), file_path);
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn extract_variables(sql: &str) -> String {
    let prefixes = ["__XML_PARAM_", "__XML_RAW_", "__JAVA_VAR_", "__SQL_PARAM_", "__SQL_RAW_"];
    let mut vars = Vec::new();
    let mut i = 0;
    let bytes = sql.as_bytes();
    let len = bytes.len();

    while i < len {
        if bytes[i] != b'_' {
            i += 1;
            continue;
        }
        let mut found = false;
        for prefix in &prefixes {
            let prefix_bytes = prefix.as_bytes();
            if i + prefix_bytes.len() + 2 <= len && &bytes[i..i + prefix_bytes.len()] == prefix_bytes {
                let content_start = i + prefix_bytes.len();
                let mut end = content_start;
                while end + 1 < len && !(bytes[end] == b'_' && bytes[end + 1] == b'_') {
                    end += 1;
                }
                if end + 1 < len {
                    vars.push(sql[i..end + 2].to_string());
                    i = end + 2;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            i += 1;
        }
    }

    vars.join(";")
}

// ═══════════════════════════════════════════════════════════════
//  validate-java CLI handlers
// ═══════════════════════════════════════════════════════════════

#[cfg(feature = "java")]
/// validate-java CSV 输出头
fn output_csv_validate_java_header() {
    print!("\u{FEFF}");
    println!("file,directory,line,type,name,parent,parameters,return_type,sql,valid,error_count,warning_count,errors,warnings");
}

#[cfg(feature = "java")]
/// validate-java CSV 输出行：每行对应一个提取的 SQL 片段
fn output_csv_validate_java_rows(extractions: &[ogsql_parser::java::ExtractedSql], file_name: &str, rel_dir: &str) {
    for ext in extractions {
        let method = match (&ext.origin.class_name, &ext.origin.method_name) {
            (Some(cls), Some(m)) => format!("{}::{}", cls, m),
            (None, Some(m)) => m.clone(),
            (Some(cls), None) => cls.clone(),
            (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
        };

        let (errors, warnings) = match &ext.parse_result {
            Some(parse_result) => {
                let refs: Vec<&ogsql_parser::ParserError> = parse_result.errors.iter().collect();
                (merge_error_messages(&refs, false), merge_error_messages(&refs, true))
            }
            None => (String::new(), String::new()),
        };

        let ext_error_count =
            ext.parse_result.as_ref().map(|pr| pr.errors.iter().filter(|e| !is_warning(e)).count()).unwrap_or(0);
        let ext_warning_count =
            ext.parse_result.as_ref().map(|pr| pr.errors.iter().filter(|e| is_warning(e)).count()).unwrap_or(0);

        let sql = ext.sql.trim().replace('\r', "");
        println!(
            "{},{},{},{},{},{},,,{},{},{},{},{},{}",
            csv_escape(file_name),
            csv_escape(rel_dir),
            ext.origin.line,
            format!("{:?}", ext.sql_kind),
            csv_escape(&method),
            csv_escape(ext.origin.class_name.as_deref().unwrap_or("")),
            csv_escape(&sql),
            if ext_error_count == 0 { "VALID" } else { "INVALID" },
            ext_error_count,
            ext_warning_count,
            csv_escape(&errors),
            csv_escape(&warnings),
        );
    }
}

#[cfg(feature = "java")]
/// 单文件 validate-java：读取 Java 源文件，提取 SQL，执行语义校验 + lint
fn cmd_validate_java_single(
    cli: &Cli,
    extra_sql_methods: &[String],
    extra_sql_var_patterns: &[String],
    csv: bool,
    strict: bool,
) {
    if cli.file.len() > 1 {
        die!("Error: validate-java command accepts at most one --file");
    }
    let file_opt = cli.file.first().map(|s| s.as_str());
    let (source, file_path) = match file_opt {
        Some(path) => {
            let bytes = std::fs::read(path).unwrap_or_else(|e| die!("Error reading {}: {}", path, e));
            let (text, _encoding) =
                ogsql_parser::token::decode_sql_file(&bytes).unwrap_or_else(|e| die!("Error decoding {}: {}", path, e));
            (text, path.to_string())
        }
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| die!("Error reading stdin: {}", e));
            (buf, "<stdin>".to_string())
        }
    };

    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: extra_sql_methods.to_vec(),
        extra_sql_var_patterns: extra_sql_var_patterns.to_vec(),
    };
    let result = ogsql_parser::java::extract_sql_from_java(&source, &file_path, &config);

    // 收集所有 StatementInfo 和错误
    let mut all_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
    let mut all_errors: Vec<ogsql_parser::ParserError> = Vec::new();
    let mut has_parse_error = false;

    // Java 提取错误转为 ParserError::Warning
    for je in &result.errors {
        all_errors.push(ogsql_parser::ParserError::Warning {
            message: format!("Java extraction error: {}", je),
            location: ogsql_parser::SourceLocation::default(),
            level: ogsql_parser::linter::WarningLevel::Caution,
        });
    }

    for ext in &result.extractions {
        if let Some(ref parse_result) = ext.parse_result {
            all_stmts.extend(parse_result.statements.clone());
            for pe in &parse_result.errors {
                if !is_warning(pe) {
                    has_parse_error = true;
                }
                all_errors.push(pe.clone());
            }
        }
    }

    // PACKAGE + MERGE + PL 校验（共享 validate_from_stmts 管道）
    let (core_errors, _pkg_errors, var_errors) = validate_from_stmts(&all_stmts, &[], strict);
    all_errors.extend(core_errors);

    let has_var_errors = !var_errors.is_empty();
    let has_any_error = has_parse_error || has_var_errors;

    // Lint with Confidence::Full（使用 SqlLinter::with_default_rules 直接创建）
    // 与 lint_java_extractions 不同，后者使用 Confidence::Partial
    let lint_warnings = if cli.lint {
        let config = build_lint_config(cli);
        let linter = ogsql_parser::linter::SqlLinter::with_default_rules(config);
        linter.lint(&all_stmts, None, ogsql_parser::linter::Confidence::Full)
    } else {
        vec![]
    };

    if csv {
        output_csv_validate_java_header();
        output_csv_validate_java_rows(&result.extractions, &file_path, ".");
        if has_any_error {
            std::process::exit(1);
        }
        return;
    }

    if cli.json {
        // JSON 输出：{file, valid, error_count, warning_count, errors, extractions: [{line, method, sql, kind}]}
        let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
        let warnings: Vec<_> = all_errors.iter().filter(|e| is_warning(e)).collect();
        let extractions_json: Vec<serde_json::Value> = result
            .extractions
            .iter()
            .map(|ext| {
                let method = match (&ext.origin.class_name, &ext.origin.method_name) {
                    (Some(cls), Some(mn)) => format!("{}::{}", cls, mn),
                    (None, Some(mn)) => mn.clone(),
                    (Some(cls), None) => cls.clone(),
                    (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
                };
                serde_json::json!({
                    "line": ext.origin.line,
                    "method": method,
                    "sql": ext.sql.trim(),
                    "kind": format!("{:?}", ext.sql_kind),
                })
            })
            .collect();
        let mut out = serde_json::json!({
            "file": file_path,
            "valid": !has_any_error,
            "error_count": real_errors.len() + var_errors.len(),
            "warning_count": warnings.len() + lint_warnings.len(),
            "errors": all_errors,
            "extractions": extractions_json,
        });
        if !var_errors.is_empty() {
            out.as_object_mut().unwrap().insert("undefined_variables".to_string(), serde_json::json!(var_errors));
        }
        if !lint_warnings.is_empty() {
            out.as_object_mut().unwrap().insert("lint_warnings".to_string(), serde_json::json!(lint_warnings));
            out.as_object_mut().unwrap().insert("lint_summary".to_string(), format_warnings_summary(&lint_warnings));
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        if has_any_error {
            std::process::exit(1);
        }
    } else {
        // 文本输出
        if !result.errors.is_empty() {
            eprintln!("Java extraction errors for {}:", file_path);
            for je in &result.errors {
                eprintln!("  {}", je);
            }
        }

        let real_errors: Vec<_> = all_errors.iter().filter(|e| !is_warning(e)).collect();
        let warnings: Vec<_> = all_errors.iter().filter(|e| is_warning(e)).collect();

        if !has_any_error && warnings.is_empty() {
            println!("{}: VALID", file_path);
        } else if !has_any_error {
            println!("{}: VALID ({} warning(s)):", file_path, warnings.len());
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        } else {
            let total_errors = real_errors.len() + var_errors.len();
            if strict {
                println!(
                    "{}: INVALID ({} error(s), {} warning(s)) [strict mode]:",
                    file_path,
                    total_errors,
                    warnings.len()
                );
            } else {
                println!("{}: INVALID ({} error(s), {} warning(s)):", file_path, total_errors, warnings.len());
            }
            for e in &real_errors {
                eprintln!("  error: {}", e);
            }
            for ve in &var_errors {
                let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
                let kind_label = match ve.kind {
                    ogsql_parser::UndefinedRefKind::Function => "undefined function",
                    ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
                };
                eprintln!("  error: {} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info);
            }
            for w in &warnings {
                eprintln!("  warning: {}", w);
            }
        }

        // 逐提取信息
        for ext in &result.extractions {
            let location = match (&ext.origin.class_name, &ext.origin.method_name) {
                (Some(cls), Some(mn)) => format!("{}::{}", cls, mn),
                (None, Some(mn)) => mn.clone(),
                (Some(cls), None) => cls.clone(),
                (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
            };
            println!("── {:?} [{:?}] @ {} L{} ──", ext.origin.method, ext.sql_kind, location, ext.origin.line);
            println!("{}", ext.sql.trim());
            if let Some(ref parse_result) = ext.parse_result {
                let ext_real: Vec<_> = parse_result.errors.iter().filter(|e| !is_warning(e)).collect();
                let ext_warns: Vec<_> = parse_result.errors.iter().filter(|e| is_warning(e)).collect();
                if !ext_real.is_empty() {
                    eprintln!("  {} error(s):", ext_real.len());
                    for e in &ext_real {
                        eprintln!("    {}", e);
                    }
                }
                if !ext_warns.is_empty() {
                    eprintln!("  {} warning(s):", ext_warns.len());
                    for w in &ext_warns {
                        eprintln!("    {}", w);
                    }
                }
                if ext_real.is_empty() {
                    println!(
                        "  ✓ SQL valid ({} statement(s)){}",
                        parse_result.statements.len(),
                        if ext_warns.is_empty() { "" } else { " (with warnings)" }
                    );
                } else {
                    println!("  ✗ SQL INVALID");
                }
            } else {
                println!("  (no parse result)");
            }
            println!();
        }

        if !lint_warnings.is_empty() {
            eprintln!("\n── Lint Warnings ({}) ──", lint_warnings.len());
            format_warnings_text(&lint_warnings);
            eprintln!("── Summary ──");
            eprintln!("  Total: {} warnings", lint_warnings.len());
        }

        if has_any_error {
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "java")]
/// 目录 validate-java：递归扫描 .java 文件，跨文件提取 + 校验
fn cmd_validate_java_dir(
    cli: &Cli,
    extra_sql_methods: &[String],
    extra_sql_var_patterns: &[String],
    dir_path: &str,
    csv: bool,
    stats: bool,
    strict: bool,
) {
    use std::path::Path;

    let root = Path::new(dir_path);
    if !root.is_dir() {
        die!("Error: '{}' is not a directory", dir_path);
    }

    // 收集所有 .java 文件
    let mut files: Vec<(String, String)> = Vec::new(); // (file_path, source)
    for entry in walkdir::WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("java") {
            continue;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };
        let (source, _encoding) = match ogsql_parser::token::decode_sql_file(&bytes) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", path.display(), e);
                continue;
            }
        };
        let file_path_str = path.to_string_lossy().to_string();
        files.push((file_path_str, source));
    }

    if files.is_empty() {
        die!("No .java files found in '{}'", dir_path);
    }

    // 跨文件提取（使用 CrossFileState 支持 PreparedStatement 回填）
    let config = ogsql_parser::java::JavaExtractConfig {
        extra_sql_methods: extra_sql_methods.to_vec(),
        extra_sql_var_patterns: extra_sql_var_patterns.to_vec(),
    };
    let file_refs: Vec<(&str, &str)> = files.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
    let mut state = ogsql_parser::java::CrossFileState::default();
    let all_results = ogsql_parser::java::extract_sql_from_java_files_with_state(&file_refs, &config, &mut state);

    // 统计信息
    let mut files_with_errors: HashSet<String> = HashSet::new();
    let mut files_with_warnings: HashSet<String> = HashSet::new();
    let mut total_extractions = 0usize;
    let mut total_stmts = 0usize;
    let mut error_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut warning_kinds: BTreeMap<&'static str, (usize, HashSet<String>)> = BTreeMap::new();
    let mut var_error_files: HashSet<String> = HashSet::new();

    // CSV 头（仅在 csv 模式下）
    if csv {
        output_csv_validate_java_header();
    }

    // JSON 收集
    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for result in &all_results {
        let file_name =
            Path::new(&result.file_path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let rel_dir = Path::new(&result.file_path)
            .parent()
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| p.to_str().unwrap_or("."))
            .unwrap_or(".");

        let mut file_stmts: Vec<ogsql_parser::StatementInfo> = Vec::new();
        let mut file_errors: Vec<ogsql_parser::ParserError> = Vec::new();
        let mut has_file_error = false;

        // Java 提取错误
        for je in &result.errors {
            file_errors.push(ogsql_parser::ParserError::Warning {
                message: format!("Java extraction error: {}", je),
                location: ogsql_parser::SourceLocation::default(),
                level: ogsql_parser::linter::WarningLevel::Caution,
            });
        }

        for ext in &result.extractions {
            total_extractions += 1;
            if let Some(ref parse_result) = ext.parse_result {
                total_stmts += parse_result.statements.len();
                file_stmts.extend(parse_result.statements.clone());
                for pe in &parse_result.errors {
                    if !is_warning(pe) {
                        has_file_error = true;
                        let kind = parser_error_kind(pe);
                        error_kinds.entry(kind).or_insert_with(|| (0, HashSet::new())).0 += 1;
                        error_kinds.get_mut(kind).unwrap().1.insert(file_name.clone());
                    } else {
                        let kind = parser_error_kind(pe);
                        warning_kinds.entry(kind).or_insert_with(|| (0, HashSet::new())).0 += 1;
                        warning_kinds.get_mut(kind).unwrap().1.insert(file_name.clone());
                    }
                    file_errors.push(pe.clone());
                }
            }
        }

        // PACKAGE + MERGE + PL 校验
        let (core_errors, _pkg_errors, var_errors) = validate_from_stmts(&file_stmts, &[], strict);
        file_errors.extend(core_errors);

        if !var_errors.is_empty() {
            var_error_files.insert(file_name.clone());
        }
        if has_file_error || !var_errors.is_empty() {
            files_with_errors.insert(file_name.clone());
        }
        if file_errors.iter().any(is_warning) {
            files_with_warnings.insert(file_name.clone());
        }

        if csv {
            output_csv_validate_java_rows(&result.extractions, &file_name, rel_dir);
        } else if cli.json {
            // JSON 按文件收集
            let real_errors: Vec<_> = file_errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = file_errors.iter().filter(|e| is_warning(e)).collect();
            let extractions_json: Vec<serde_json::Value> = result
                .extractions
                .iter()
                .map(|ext| {
                    let method = match (&ext.origin.class_name, &ext.origin.method_name) {
                        (Some(cls), Some(mn)) => format!("{}::{}", cls, mn),
                        (None, Some(mn)) => mn.clone(),
                        (Some(cls), None) => cls.clone(),
                        (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
                    };
                    serde_json::json!({
                        "line": ext.origin.line,
                        "method": method,
                        "sql": ext.sql.trim(),
                        "kind": format!("{:?}", ext.sql_kind),
                    })
                })
                .collect();
            let mut file_result = serde_json::json!({
                "file": result.file_path,
                "valid": !has_file_error && var_errors.is_empty(),
                "error_count": real_errors.len() + var_errors.len(),
                "warning_count": warnings.len(),
                "errors": file_errors,
                "extractions": extractions_json,
            });
            if !var_errors.is_empty() {
                file_result
                    .as_object_mut()
                    .unwrap()
                    .insert("undefined_variables".to_string(), serde_json::json!(var_errors));
            }
            json_results.push(file_result);
        } else {
            // 文本输出
            let real_errors: Vec<_> = file_errors.iter().filter(|e| !is_warning(e)).collect();
            let warnings: Vec<_> = file_errors.iter().filter(|e| is_warning(e)).collect();
            let has_var = !var_errors.is_empty();

            if !result.errors.is_empty() {
                eprintln!("Java extraction errors for {}:", result.file_path);
                for je in &result.errors {
                    eprintln!("  {}", je);
                }
            }

            if !has_file_error && !has_var && warnings.is_empty() {
                println!("[{}/{}] VALID", rel_dir, file_name);
            } else if !has_file_error && !has_var {
                println!("[{}/{}] VALID ({} warning(s))", rel_dir, file_name, warnings.len());
                for w in &warnings {
                    eprintln!("  warning: {}", w);
                }
            } else {
                println!(
                    "[{}/{}] INVALID ({} error(s), {} warning(s))",
                    rel_dir,
                    file_name,
                    real_errors.len() + var_errors.len(),
                    warnings.len()
                );
                for e in &real_errors {
                    eprintln!("  error: {}", e);
                }
                for ve in &var_errors {
                    let line_info = ve.location.as_ref().map(|sp| format!(":{}", sp.start.line)).unwrap_or_default();
                    let kind_label = match ve.kind {
                        ogsql_parser::UndefinedRefKind::Function => "undefined function",
                        ogsql_parser::UndefinedRefKind::Variable => "undefined variable",
                    };
                    eprintln!("  error: {} '{}' in {}{}", kind_label, ve.variable_name, ve.context, line_info);
                }
                for w in &warnings {
                    eprintln!("  warning: {}", w);
                }
            }

            // 逐提取详细信息
            for ext in &result.extractions {
                let location = match (&ext.origin.class_name, &ext.origin.method_name) {
                    (Some(cls), Some(mn)) => format!("{}::{}", cls, mn),
                    (None, Some(mn)) => mn.clone(),
                    (Some(cls), None) => cls.clone(),
                    (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
                };
                println!("── {:?} [{:?}] @ {} L{} ──", ext.origin.method, ext.sql_kind, location, ext.origin.line);
                println!("{}", ext.sql.trim());
                if let Some(ref parse_result) = ext.parse_result {
                    let ext_real: Vec<_> = parse_result.errors.iter().filter(|e| !is_warning(e)).collect();
                    let ext_warns: Vec<_> = parse_result.errors.iter().filter(|e| is_warning(e)).collect();
                    if !ext_real.is_empty() {
                        eprintln!("  {} error(s):", ext_real.len());
                        for e in &ext_real {
                            eprintln!("    {}", e);
                        }
                    }
                    if !ext_warns.is_empty() {
                        eprintln!("  {} warning(s):", ext_warns.len());
                        for w in &ext_warns {
                            eprintln!("    {}", w);
                        }
                    }
                    if ext_real.is_empty() {
                        println!(
                            "  ✓ SQL valid ({} statement(s)){}",
                            parse_result.statements.len(),
                            if ext_warns.is_empty() { "" } else { " (with warnings)" }
                        );
                    } else {
                        println!("  ✗ SQL INVALID");
                    }
                } else {
                    println!("  (no parse result)");
                }
                println!();
            }
        }
    }

    // JSON 整体输出（dir 模式）
    if cli.json && !csv {
        let out = serde_json::json!({ "files": json_results });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    }

    // 统计信息
    if stats {
        let total_error_count: usize = error_kinds.values().map(|(c, _)| c).sum();
        let total_warning_count: usize = warning_kinds.values().map(|(c, _)| c).sum();

        stats_bar("");
        stats_bar("Summary / validate-java");
        stats_bar("");

        eprintln!("  {:<28} {}", "Java files processed:", all_results.len());
        eprintln!("  {:<28} {}", "Total extractions:", total_extractions);
        if total_stmts > 0 {
            eprintln!("  {:<28} {}", "SQL statements (parsed):", total_stmts);
        }
        eprintln!();

        if !files_with_errors.is_empty() {
            stats_bar("Files with validation errors");
            eprintln!("  Total: {} file(s)", files_with_errors.len());
            if !var_error_files.is_empty() {
                eprintln!("  PL variable errors: {} file(s)", var_error_files.len());
            }
            eprintln!();
        }

        if total_error_count > 0 {
            stats_bar("Error breakdown");
            eprintln!("  Total: {} error(s) (in {} file(s))", total_error_count, files_with_errors.len());
            for (kind, (cnt, files)) in &error_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }

        if total_warning_count > 0 {
            stats_bar("Warning breakdown");
            eprintln!("  Total: {} warning(s) (in {} file(s))", total_warning_count, files_with_warnings.len());
            for (kind, (cnt, files)) in &warning_kinds {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, cnt, files.len());
            }
            eprintln!();
        }
    }

    if !files_with_errors.is_empty() {
        std::process::exit(1);
    }
}

#[cfg(feature = "java")]
/// validate-java 分发器：根据是否指定 dir 路由到单文件或目录模式
fn cmd_validate_java(
    cli: &Cli,
    extra_sql_methods: &[String],
    extra_sql_var_patterns: &[String],
    dir: Option<&str>,
    csv: bool,
    stats: bool,
    strict: bool,
) {
    match dir {
        Some(dir_path) => {
            cmd_validate_java_dir(cli, extra_sql_methods, extra_sql_var_patterns, dir_path, csv, stats, strict)
        }
        None => cmd_validate_java_single(cli, extra_sql_methods, extra_sql_var_patterns, csv, strict),
    }
}

#[cfg(feature = "ibatis")]
fn output_csv_xml_header() {
    print!("\u{FEFF}");
    println!("file,directory,line,method,sql,variables,parameter_types,error,warning");
}

#[cfg(feature = "ibatis")]
fn output_csv_xml_rows(statements: &[ogsql_parser::ibatis::ParsedStatement], file_name: &str, rel_dir: &str) {
    for stmt in statements {
        let (errors, warnings) = match &stmt.parse_result {
            Some((_, parse_errors)) => {
                let refs: Vec<&ogsql_parser::ParserError> = parse_errors.iter().collect();
                (merge_error_messages(&refs, false), merge_error_messages(&refs, true))
            }
            None => (String::new(), String::new()),
        };

        let sql = stmt.flat_sql.trim().replace('\r', "");
        let variables = extract_variables(&stmt.flat_sql);
        let parameter_types: String = stmt
            .parameters
            .iter()
            .map(|p| match p.jdbc_type {
                Some(ref jt) => format!("{}:{:?}", p.name, jt),
                None => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(";");
        println!(
            "{},{},{},{},{},{},{},{},{}",
            csv_escape(file_name),
            csv_escape(rel_dir),
            stmt.line,
            csv_escape(&stmt.id),
            csv_escape(&sql),
            csv_escape(&variables),
            csv_escape(&parameter_types),
            csv_escape(&errors),
            csv_escape(&warnings),
        );
    }
}

#[cfg(feature = "java")]
fn output_csv_java_header() {
    print!("\u{FEFF}");
    println!("file,directory,line,method,sql,variables,error,warning");
}

#[cfg(feature = "java")]
fn output_csv_java_rows(extractions: &[ogsql_parser::java::ExtractedSql], file_name: &str, rel_dir: &str) {
    for ext in extractions {
        let method = match (&ext.origin.class_name, &ext.origin.method_name) {
            (Some(cls), Some(m)) => format!("{}::{}", cls, m),
            (None, Some(m)) => m.clone(),
            (Some(cls), None) => cls.clone(),
            (None, None) => ext.origin.variable_name.clone().unwrap_or_default(),
        };

        let (errors, warnings) = match &ext.parse_result {
            Some(parse_result) => {
                let refs: Vec<&ogsql_parser::ParserError> = parse_result.errors.iter().collect();
                (merge_error_messages(&refs, false), merge_error_messages(&refs, true))
            }
            None => (String::new(), String::new()),
        };

        let sql = ext.sql.trim().replace('\r', "");
        let variables = extract_variables(&ext.sql);
        println!(
            "{},{},{},{},{},{},{},{}",
            csv_escape(file_name),
            csv_escape(rel_dir),
            ext.origin.line,
            csv_escape(&method),
            csv_escape(&sql),
            csv_escape(&variables),
            csv_escape(&errors),
            csv_escape(&warnings),
        );
    }
}

// ═══════════════════════════════════════════════════════════════
//  Stats helpers for --dir commands
// ═══════════════════════════════════════════════════════════════

use std::collections::{BTreeMap, HashSet};

fn stmt_category(stmt: &Statement) -> &'static str {
    use Statement::*;
    match stmt {
        Select(_) => "SELECT",
        Insert(_) | InsertAll(_) | InsertFirst(_) => "INSERT",
        Update(_) => "UPDATE",
        Delete(_) => "DELETE",
        Merge(_) => "MERGE",
        CreateTable(_) | CreateTableAs(_) => "CREATE TABLE",
        AlterTable(_) | AlterTablespace(_) => "ALTER TABLE",
        Drop(_) => "DROP",
        Truncate(_) => "TRUNCATE",
        CreateIndex(_) | CreateGlobalIndex(_) => "CREATE INDEX",
        AlterIndex(_) => "ALTER INDEX",
        CreateSchema(_) => "CREATE SCHEMA",
        AlterSchema(_) => "ALTER SCHEMA",
        CreateDatabase(_) | CreateDatabaseLink(_) => "CREATE DATABASE",
        AlterDatabase(_) => "ALTER DATABASE",
        CreateTablespace(_) => "CREATE TABLESPACE",
        CreateFunction(_) => "CREATE FUNCTION",
        AlterFunction(_) => "ALTER FUNCTION",
        CreateProcedure(_) => "CREATE PROCEDURE",
        AlterProcedure(_) => "ALTER PROCEDURE",
        CreatePackage(_) | CreatePackageBody(_) => "CREATE PACKAGE",
        AlterPackage(_) => "ALTER PACKAGE",
        CreateView(_) | CreateMaterializedView(_) => "CREATE VIEW",
        AlterView(_) | AlterMaterializedView(_) => "ALTER VIEW",
        CreateSequence(_) => "CREATE SEQUENCE",
        AlterSequence(_) => "ALTER SEQUENCE",
        CreateTrigger(_) => "CREATE TRIGGER",
        AlterTrigger(_) => "ALTER TRIGGER",
        CreateDomain(_) | AlterDomain(_) => "DOMAIN",
        Do(_) | AnonyBlock(_) => "PL/BLOCK",
        Call(_) => "CALL",
        Transaction(_) => "TRANSACTION",
        Copy(_) => "COPY",
        Explain(_) => "EXPLAIN",
        Prepare(_) | Execute(_) | Deallocate(_) => "PREPARE/EXECUTE",
        VariableSet(_) | VariableShow(_) | VariableReset(_) => "SET/SHOW",
        Grant(_) | Revoke(_) => "GRANT/REVOKE",
        GrantRole(_) | RevokeRole(_) => "GRANT/REVOKE ROLE",
        Comment(_) => "COMMENT",
        Lock(_) => "LOCK",
        Analyze(_) => "ANALYZE",
        Vacuum(_) => "VACUUM",
        Values(_) => "VALUES",
        ExecuteDirect(_) => "EXECUTE DIRECT",
        CreateSynonym(_) | AlterSynonym(_) => "SYNONYM",
        CreateExtension(_) | AlterExtension(_) => "EXTENSION",
        CreateRole(_) | AlterRole(_) => "ROLE",
        CreateUser(_) | AlterUser(_) => "USER",
        CreateGroup(_) | AlterGroup(_) => "GROUP",
        CreateType(_) => "CREATE TYPE",
        CreateLanguage(_) => "CREATE LANGUAGE",
        CreateForeignTable(_) => "FOREIGN TABLE",
        CreateForeignServer(_) | CreateFdw(_) => "FOREIGN SERVER",
        CreatePublication(_) | AlterPublication(_) => "PUBLICATION",
        CreateSubscription(_) | AlterSubscription(_) => "SUBSCRIPTION",
        CreateNode(_) | AlterNode(_) | CreateNodeGroup(_) | AlterNodeGroup(_) => "NODE",
        CreateResourcePool(_) | AlterResourcePool(_) => "RESOURCE POOL",
        CreateWorkloadGroup(_) | AlterWorkloadGroup(_) => "WORKLOAD GROUP",
        CreateAuditPolicy(_) | AlterAuditPolicy(_) => "AUDIT POLICY",
        CreateMaskingPolicy(_) | AlterMaskingPolicy(_) => "MASKING POLICY",
        CreateRlsPolicy(_) | AlterRlsPolicy(_) => "RLS POLICY",
        CreateDataSource(_) | AlterDataSource(_) => "DATA SOURCE",
        CreateEvent(_) | AlterEvent(_) => "EVENT",
        CreateDirectory(_) => "DIRECTORY",
        AlterDirectory(_) => "ALTER DIRECTORY",
        CreateAggregate(_) => "CREATE AGGREGATE",
        CreateOperator(_) => "CREATE OPERATOR",
        RefreshMaterializedView(_) => "REFRESH MVIEW",
        Reindex(_) => "REINDEX",
        Cluster(_) => "CLUSTER",
        Listen(_) | Notify(_) | Unlisten(_) => "LISTEN/NOTIFY",
        DeclareCursor(_) | ClosePortal(_) | Fetch(_) | Move(_) => "CURSOR",
        Purge(_) => "PURGE",
        TimeCapsule(_) => "TIME CAPSULE",
        Snapshot(_) => "SNAPSHOT",
        Shrink(_) => "SHRINK",
        Compile(_) => "COMPILE",
        SecLabel(_) => "SECURITY LABEL",
        Rule(_) | DropRule(_) => "RULE",
        CreateCast(_) | CreateConversion(_) => "CAST/CONVERSION",
        CreateOpClass(_) | CreateOpFamily(_) | AlterOpFamily(_) => "OPERATOR CLASS",
        CreateTextSearchConfig(_)
        | CreateTextSearchDict(_)
        | AlterTextSearchConfig(_)
        | AlterTextSearchDict(_)
        | AlterTextSearchConfigFull(_)
        | AlterTextSearchDictFull(_) => "TEXT SEARCH",
        CreateUserMapping(_) | AlterUserMapping(_) | DropUserMapping(_) => "USER MAPPING",
        AlterDefaultPrivileges(_) => "ALTER DEFAULT PRIVILEGES",
        AlterCompositeType(_) => "ALTER TYPE",
        AlterCoordinator(_) => "ALTER COORDINATOR",
        AlterAppWorkloadGroupMapping(_) => "ALTER APP WORKLOAD GROUP",
        AlterDatabaseLink(_) => "ALTER DATABASE LINK",
        AlterLargeObject(_) => "ALTER LARGE OBJECT",
        AlterSession(_) => "ALTER SESSION",
        AlterSystemKillSession(_) => "ALTER SYSTEM KILL",
        AlterGlobalConfig(_) => "ALTER GLOBAL CONFIG",
        DropWeakPasswordDictionary | CreateWeakPasswordDictionary | CreateWeakPasswordDictionaryWithValues(_) => {
            "WEAK PASSWORD DICT"
        }
        CreatePolicyLabel(_) | AlterPolicyLabel(_) | DropPolicyLabel(_) => "POLICY LABEL",
        CreateContQuery(_) => "CONTINUOUS QUERY",
        CreateStream(_) => "STREAM",
        CreateKey(_) => "KEY",
        SetSessionAuthorization(_) => "SET SESSION AUTHORIZATION",
        CreateAppWorkloadGroupMapping(_) | DropAppWorkloadGroupMapping(_) => "APP WORKLOAD GROUP",
        ExpdpDatabase(_) | ExpdpTable(_) | ImpdpDatabase(_) | ImpdpTable(_) => "EXPDP/IMPDP",
        ReassignOwned(_) => "REASSIGN OWNED",
        LockBuckets(_) | MarkBuckets(_) => "LOCK BUCKETS",
        Shutdown(_) | Barrier(_) | Abort | Checkpoint | Empty => "UTILITY",
        Replace(_) => "REPLACE",
        GetDiag(_) => "GET DIAGNOSTICS",
        ShowEvent(_) => "SHOW EVENT",
        RemovePackage(_) => "REMOVE PACKAGE",
        PredictBy(_) => "PREDICT BY",
        CleanConn(_) => "CLEAN CONNECTION",
        Verify(_) => "VERIFY",
        _ => "OTHER",
    }
}

fn parser_error_kind(err: &ParserError) -> &'static str {
    match err {
        ParserError::UnexpectedToken { .. } => "UnexpectedToken",
        ParserError::UnexpectedEof { .. } => "UnexpectedEof",
        ParserError::Warning { .. } => "Warning",
        ParserError::ReservedKeywordAsIdentifier { .. } => "ReservedKeyword",
        ParserError::TokenizerError(_) => "TokenizerError",
        ParserError::UnsupportedSyntax { .. } => "UnsupportedSyntax",
    }
}

fn stats_bar(label: &str) {
    if label.is_empty() {
        eprintln!("  {}", "─".repeat(55));
    } else {
        let side = (55usize.saturating_sub(label.len() + 2)) / 2;
        eprintln!("  {}{}{}", "─".repeat(side), format!(" {} ", label), "─".repeat(side));
    }
}

fn print_parse_stats(
    files_processed: usize,
    files_with_errors: &HashSet<String>,
    files_with_warnings: &HashSet<String>,
    total_stmts: usize,
    stmt_counts: &BTreeMap<&'static str, usize>,
    error_counts: &BTreeMap<&'static str, (usize, HashSet<String>)>,
    warning_counts: &BTreeMap<&'static str, (usize, HashSet<String>)>,
    extra_title: &str,
) {
    let total_errors: usize = error_counts.values().map(|(c, _)| c).sum();
    let total_warnings: usize = warning_counts.values().map(|(c, _)| c).sum();

    stats_bar("");
    let title = if extra_title.is_empty() { "Summary".to_string() } else { format!("Summary / {}", extra_title) };
    stats_bar(&title);
    stats_bar("");

    eprintln!("  {:<24} {}", "Files processed:", files_processed);
    if !files_with_errors.is_empty() {
        eprintln!("  {:<24} {}", "Files with errors:", files_with_errors.len());
    }
    if !files_with_warnings.is_empty() {
        eprintln!("  {:<24} {}", "Files with warnings:", files_with_warnings.len());
    }
    eprintln!("  {:<24} {}", "Total statements:", total_stmts);
    eprintln!();

    if !stmt_counts.is_empty() {
        stats_bar("Statement breakdown");
        let max_name = stmt_counts.keys().map(|k| k.len()).max().unwrap_or(10);
        for (cat, count) in stmt_counts {
            let pct = if total_stmts > 0 { *count as f64 / total_stmts as f64 * 100.0 } else { 0.0 };
            eprintln!("  {:width$} {:>6} ({:>5.1}%)", cat, count, pct, width = max_name);
        }
        eprintln!();
    }

    if total_errors > 0 || total_warnings > 0 {
        stats_bar("Error / Warning breakdown");
        eprint!("  Total: ");
        if total_errors > 0 {
            eprint!("{} error(s) (in {} file(s))", total_errors, files_with_errors.len());
        }
        if total_errors > 0 && total_warnings > 0 {
            eprint!(", ");
        }
        if total_warnings > 0 {
            eprint!("{} warning(s) (in {} file(s))", total_warnings, files_with_warnings.len());
        }
        eprintln!();

        if !error_counts.is_empty() {
            eprintln!();
            eprintln!("  Errors:");
            for (kind, (count, files)) in error_counts {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, count, files.len());
            }
        }
        if !warning_counts.is_empty() {
            eprintln!();
            eprintln!("  Warnings:");
            for (kind, (count, files)) in warning_counts {
                eprintln!("    {:<20} {:>4} ({} file(s))", kind, count, files.len());
            }
        }
        eprintln!();
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Format {
            indent,
            ref keyword_case,
            ref comma,
            line_width,
            uppercase,
            no_select_newline,
            no_logical_newline,
            no_semicolon_newline,
        } => cmd_format(
            &cli,
            indent,
            keyword_case.clone(),
            comma.clone(),
            line_width,
            uppercase,
            no_select_newline,
            no_logical_newline,
            no_semicolon_newline,
        ),
        Commands::Parse { ref dir, ref ext, csv, ref output_dir, stats, ref procedure, extract_sql } => {
            if !dir.is_empty() && !cli.file.is_empty() {
                die!("Error: --dir and -f are mutually exclusive");
            }
            if procedure.is_some() {
                if cli.file.is_empty() {
                    die!("Error: --procedure requires --file (stdin not supported)");
                }
                if cli.file.len() > 1 {
                    die!("Error: --procedure cannot be used with multiple --file arguments");
                }
            }
            if !dir.is_empty() {
                cmd_parse_dir(&cli, dir, ext, csv, output_dir.as_deref(), stats, extract_sql);
            } else {
                cmd_parse(&cli, csv, procedure.as_deref(), extract_sql);
            }
        }
        Commands::JsonToSql => cmd_json2sql(&cli),
        Commands::Tokenize => cmd_tokenize(&cli),
        Commands::Validate { ref dir, ref ext, csv, stats, strict } => {
            if !dir.is_empty() && !cli.file.is_empty() {
                die!("Error: --dir and -f are mutually exclusive");
            }
            if !dir.is_empty() {
                cmd_validate_dir(&cli, dir, ext, csv, stats, strict);
            } else {
                cmd_validate(&cli, csv, strict);
            }
        }
        Commands::ServeStdio => serve_stdio::run(),
        #[cfg(feature = "serve")]
        Commands::Serve { port, host } => {
            let addr = format!("{}:{}", host, port);

            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "ogsql=info,tower_http=info".into()),
                )
                .json()
                .init();

            tracing::info!(addr = %addr, "starting ogsql server");
            eprintln!("ogsql server listening on http://{}", addr);
            #[cfg(feature = "utoipa-swagger-ui")]
            eprintln!("  Swagger UI:  http://{}/api-docs/swagger-ui/", addr);
            eprintln!("  OpenAPI spec: http://{}/api-docs/openapi.json", addr);

            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                let listener = tokio::net::TcpListener::bind(&addr)
                    .await
                    .unwrap_or_else(|e| die!("Failed to bind {}: {}", addr, e));
                axum::serve(listener, serve::router()).await.unwrap_or_else(|e| die!("Server error: {}", e));
            });
        }
        #[cfg(feature = "tui")]
        Commands::Playground => cmd_playground(),
        #[cfg(feature = "ibatis")]
        #[cfg(not(feature = "java"))]
        Commands::ParseXml { ref dir, csv, stats, structured } => {
            cmd_parse_xml(&cli, dir.as_deref(), csv, None, stats, structured)
        }
        #[cfg(feature = "ibatis")]
        #[cfg(feature = "java")]
        Commands::ParseXml { ref dir, csv, ref java_src, stats, structured } => {
            cmd_parse_xml(&cli, dir.as_deref(), csv, java_src.as_deref(), stats, structured)
        }
        #[cfg(feature = "mcp")]
        Commands::Mcp => {
            use ogsql_parser::mcp::OgsqlServer;
            use rmcp::ServiceExt;
            eprintln!("ogsql: starting MCP server on stdio");
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                let service = OgsqlServer
                    .serve(rmcp::transport::stdio())
                    .await
                    .unwrap_or_else(|e| die!("MCP server init failed: {:?}", e));
                service.waiting().await.unwrap_or_else(|e| die!("MCP server error: {:?}", e));
            });
        }
        #[cfg(feature = "java")]
        Commands::ParseJava { ref extra_sql_methods, ref extra_sql_var_patterns, ref dir, csv, stats } => {
            cmd_parse_java(&cli, extra_sql_methods, extra_sql_var_patterns, dir.as_deref(), csv, stats)
        }
        #[cfg(feature = "ibatis")]
        Commands::ValidateXml {
            ref dir,
            csv,
            #[cfg(feature = "java")]
            ref java_src,
            stats,
            strict,
        } => {
            #[cfg(feature = "java")]
            let js = java_src.as_deref();
            #[cfg(not(feature = "java"))]
            let js: Option<&str> = None;
            cmd_validate_xml(&cli, dir.as_deref(), csv, js, stats, strict);
        }
        #[cfg(feature = "java")]
        Commands::ValidateJava { ref extra_sql_methods, ref extra_sql_var_patterns, ref dir, csv, stats, strict } => {
            cmd_validate_java(&cli, extra_sql_methods, extra_sql_var_patterns, dir.as_deref(), csv, stats, strict);
        }
    }
}

fn pl_data_type_to_string(dt: &ogsql_parser::ast::plpgsql::PlDataType) -> Option<String> {
    match dt {
        ogsql_parser::ast::plpgsql::PlDataType::TypeName(s) => Some(normalize_data_type(s)),
        ogsql_parser::ast::plpgsql::PlDataType::PercentType { column, .. } => {
            Some(extract_last_ident(column).to_uppercase())
        }
        ogsql_parser::ast::plpgsql::PlDataType::PercentRowType(s) => {
            let ident = extract_last_ident(s);
            Some(format!("{}_ROWTYPE", ident).to_uppercase())
        }
        ogsql_parser::ast::plpgsql::PlDataType::Record => Some("RECORD".into()),
        ogsql_parser::ast::plpgsql::PlDataType::Cursor => None,
        ogsql_parser::ast::plpgsql::PlDataType::RefCursor => None,
    }
}

fn collect_block_vars(
    block: &ogsql_parser::ast::plpgsql::PlBlock,
    params: &[ogsql_parser::ast::RoutineParam],
    schema: Option<&ogsql_parser::FullSchema>,
) -> std::collections::HashMap<String, Option<String>> {
    use ogsql_parser::ast::plpgsql::PlDeclaration;
    let mut vars = std::collections::HashMap::new();
    for p in params {
        vars.insert(p.name.to_ascii_lowercase(), Some(normalize_data_type_with_schema(&p.data_type, schema)));
    }
    for decl in &block.declarations {
        match decl {
            PlDeclaration::Variable(v) => {
                let t = pl_data_type_to_string(&v.data_type);
                vars.insert(v.name.to_ascii_lowercase(), t);
            }
            PlDeclaration::Record(r) => {
                vars.insert(r.name.to_ascii_lowercase(), Some("RECORD".into()));
            }
            PlDeclaration::Cursor(c) => {
                vars.insert(c.name.to_ascii_lowercase(), None);
            }
            PlDeclaration::Type(t) => {
                let name = match t {
                    ogsql_parser::ast::plpgsql::PlTypeDecl::Record { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::TableOf { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::VarrayOf { name, .. } => name,
                    ogsql_parser::ast::plpgsql::PlTypeDecl::RefCursor { name } => name,
                };
                vars.insert(name.to_ascii_lowercase(), None);
            }
            PlDeclaration::NestedProcedure(p) => {
                vars.insert(p.name.join(".").to_ascii_lowercase(), None);
            }
            PlDeclaration::NestedFunction(f) => {
                vars.insert(f.name.join(".").to_ascii_lowercase(), None);
            }
            PlDeclaration::Pragma { name, .. } => {
                vars.insert(name.to_ascii_lowercase(), None);
            }
        }
    }
    vars
}

fn normalize_data_type(data_type: &str) -> String {
    normalize_data_type_inner(data_type)
}

fn normalize_data_type_with_schema(data_type: &str, schema: Option<&ogsql_parser::FullSchema>) -> String {
    let upper = data_type.trim().to_uppercase();

    // %TYPE / %ROWTYPE references: try to resolve from schema first
    if let Some(pos) = upper.find("%TYPE") {
        let reference = &data_type[..pos].trim(); // e.g., "DB_LOG.PROC_NAME"
        if let Some(s) = schema {
            if let Some(resolved) = lookup_column_type(s, reference) {
                return normalize_data_type_inner(&resolved);
            }
        }
        return extract_last_ident(reference).to_uppercase();
    }
    if let Some(pos) = upper.find("%ROWTYPE") {
        let reference = &data_type[..pos].trim();
        if let Some(s) = schema {
            if let Some(resolved) = lookup_column_type(s, reference) {
                let base = normalize_data_type_inner(&resolved);
                return format!("{}_ROWTYPE", base);
            }
        }
        let ident = extract_last_ident(reference);
        return format!("{}_ROWTYPE", ident).to_uppercase();
    }

    normalize_data_type_inner(data_type)
}

/// Look up a column type in the schema. Reference format: "DB_LOG.PROC_NAME" or "PROC_NAME".
fn lookup_column_type(schema: &ogsql_parser::FullSchema, reference: &str) -> Option<String> {
    let parts: Vec<&str> = reference.rsplitn(2, '.').collect();
    let (col_name, table_part) =
        if parts.len() == 2 { (parts[0].trim(), Some(parts[1].trim())) } else { (parts[0].trim(), None) };
    let col_lower = col_name.to_ascii_lowercase();

    for (table, columns) in &schema.columns {
        if let Some(tp) = table_part {
            if !table.eq_ignore_ascii_case(tp) {
                continue;
            }
        }
        if let Some(data_type) = columns.get(&col_lower) {
            return Some(data_type.clone());
        }
    }
    None
}

fn load_schema_for_extract(cli: &Cli) -> Option<ogsql_parser::FullSchema> {
    cli.schema_json.as_deref().and_then(|p| {
        ogsql_parser::load_full_schema(p).map_err(|e| eprintln!("Warning: failed to load schema '{}': {}", p, e)).ok()
    })
}

fn normalize_data_type_inner(data_type: &str) -> String {
    let upper = data_type.trim().to_uppercase();

    // %TYPE / %ROWTYPE references: cannot resolve base type without schema.
    // Extract the referenced column/type name as the best available hint.
    if let Some(pos) = upper.find("%TYPE") {
        let base = &data_type[..pos];
        return extract_last_ident(base).to_uppercase();
    }
    if let Some(pos) = upper.find("%ROWTYPE") {
        let base = &data_type[..pos];
        let ident = extract_last_ident(base);
        return format!("{}_ROWTYPE", ident).to_uppercase();
    }

    // Normalize to JDBC-compatible type names (strip size/precision).
    // Oracle-specific names mapped to standard JDBC java.sql.Types equivalents.
    if upper.starts_with("VARCHAR2") {
        return "VARCHAR".to_string();
    }
    if upper.starts_with("VARCHAR") {
        return "VARCHAR".to_string();
    }
    if upper.starts_with("NVARCHAR2") {
        return "NVARCHAR".to_string();
    }
    if upper.starts_with("NVARCHAR") {
        return "NVARCHAR".to_string();
    }
    if upper.starts_with("CHARACTER VARYING") {
        return "VARCHAR".to_string();
    }
    if upper.starts_with("CHARACTER") {
        return "CHAR".to_string();
    }
    if upper.starts_with("CHAR") {
        return "CHAR".to_string();
    }
    if upper.starts_with("NCHAR") {
        return "NCHAR".to_string();
    }
    if upper.starts_with("NUMBER") {
        return "NUMERIC".to_string();
    }
    if upper.starts_with("NUMERIC") {
        return "NUMERIC".to_string();
    }
    if upper.starts_with("DECIMAL") {
        return "DECIMAL".to_string();
    }
    if upper.starts_with("PLS_INTEGER") {
        return "INTEGER".to_string();
    }
    if upper.starts_with("BINARY_INTEGER") {
        return "INTEGER".to_string();
    }
    if upper.starts_with("BIGINT") {
        return "BIGINT".to_string();
    }
    if upper.starts_with("INTEGER") {
        return "INTEGER".to_string();
    }
    if upper.starts_with("SMALLINT") {
        return "SMALLINT".to_string();
    }
    if upper.starts_with("TINYINT") {
        return "TINYINT".to_string();
    }
    if upper.starts_with("INT") && !upper.starts_with("INTERVAL") && !upper.starts_with("INTEGER") {
        return "INTEGER".to_string();
    }
    if upper.starts_with("DOUBLE PRECISION") {
        return "DOUBLE".to_string();
    }
    if upper.starts_with("DOUBLE") {
        return "DOUBLE".to_string();
    }
    if upper.starts_with("FLOAT") {
        return "FLOAT".to_string();
    }
    if upper.starts_with("REAL") {
        return "REAL".to_string();
    }
    if upper.starts_with("BOOLEAN") {
        return "BOOLEAN".to_string();
    }
    if upper.starts_with("TIMESTAMP") {
        return "TIMESTAMP".to_string();
    }
    if upper.starts_with("DATE") {
        return "DATE".to_string();
    }
    if upper.starts_with("TIME") {
        return "TIME".to_string();
    }
    if upper.starts_with("CLOB") {
        return "CLOB".to_string();
    }
    if upper.starts_with("NCLOB") {
        return "NCLOB".to_string();
    }
    if upper.starts_with("BLOB") {
        return "BLOB".to_string();
    }
    if upper.starts_with("BYTEA") {
        return "BYTEA".to_string();
    }
    if upper.starts_with("RAW") {
        return "RAW".to_string();
    }
    if upper.starts_with("LONG RAW") {
        return "LONG_RAW".to_string();
    }
    if upper.starts_with("LONG") {
        return "LONG".to_string();
    }
    if upper.starts_with("ROWID") {
        return "ROWID".to_string();
    }
    if upper.starts_with("UROWID") {
        return "UROWID".to_string();
    }
    if upper.starts_with("XMLTYPE") {
        return "XMLTYPE".to_string();
    }
    if upper.starts_with("SYS_REFCURSOR") {
        return "CURSOR".to_string();
    }
    if upper.starts_with("REFCURSOR") {
        return "CURSOR".to_string();
    }
    if upper.starts_with("BFILE") {
        return "BFILE".to_string();
    }
    if upper.starts_with("INTERVAL") {
        return "INTERVAL".to_string();
    }
    if upper.starts_with("JSON") {
        return "JSON".to_string();
    }
    if upper.starts_with("TEXT") {
        return "TEXT".to_string();
    }
    if upper.starts_with("SERIAL") {
        return "SERIAL".to_string();
    }
    if upper.starts_with("BIGSERIAL") {
        return "BIGSERIAL".to_string();
    }

    // Unknown type: strip schema prefix, return the base name
    extract_last_ident(data_type).to_uppercase()
}

/// Extract the last dot-separated identifier component (e.g., "DB_LOG.PROC_NAME" → "PROC_NAME").
fn extract_last_ident(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s).trim()
}

fn sanitize_type_for_placeholder(t: &str) -> String {
    let mut s: String = t.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    // Collapse consecutive underscores
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    // Strip trailing underscores to avoid creating "__" when combined with
    // the "_" separator in format!("{}{}_{}__", prefix, sanitized_type, ident)
    while s.ends_with('_') {
        s.pop();
    }
    s
}

/// Returns true if the text looks like a SQL statement (DML/DDL), not a PL/pgSQL assignment.
fn is_sql_statement(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    // Skip PL/pgSQL assignments (contain := without being embedded SQL)
    if lower.contains(":=") {
        return false;
    }
    let sql_prefixes = [
        "select ",
        "insert ",
        "update ",
        "delete ",
        "merge ",
        "with ",
        "create ",
        "alter ",
        "drop ",
        "truncate ",
        "grant ",
        "revoke ",
        "call ",
        "explain ",
        "explain ",
        "lock ",
        "unlock ",
        "commit",
        "rollback",
        "savepoint",
        "set ",
        "reset ",
        "declare ",
        "fetch ",
        "move ",
        "close ",
        "copy ",
        "\\copy ",
    ];
    sql_prefixes.iter().any(|p| lower.starts_with(p))
}

fn replace_pl_vars_in_sql_with_prefix(
    sql: &str,
    vars: &std::collections::HashMap<String, Option<String>>,
    prefix: &str,
) -> String {
    let mut result = String::with_capacity(sql.len());
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let c = bytes[i];

        if c == b'\'' {
            result.push(c as char);
            i += 1;
            while i < len {
                if bytes[i] == b'\'' {
                    result.push(bytes[i] as char);
                    i += 1;
                    if i < len && bytes[i] == b'\'' {
                        result.push(bytes[i] as char);
                        i += 1;
                        continue;
                    }
                    break;
                }
                result.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        if c == b'"' {
            result.push(c as char);
            i += 1;
            while i < len {
                if bytes[i] == b'"' {
                    result.push(bytes[i] as char);
                    i += 1;
                    if i < len && bytes[i] == b'"' {
                        result.push(bytes[i] as char);
                        i += 1;
                        continue;
                    }
                    break;
                }
                result.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        let is_ident_start = c == b'_' || c.is_ascii_alphabetic();
        if is_ident_start {
            let start = i;
            i += 1;
            while i < len && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                i += 1;
            }
            let ident = &sql[start..i];
            if let Some(maybe_type) = vars.get(&ident.to_ascii_lowercase()) {
                let type_str = match maybe_type {
                    Some(t) => format!("{}_{}", sanitize_type_for_placeholder(t), ident),
                    None => ident.to_string(),
                };
                // Absorb subsequent .field access (e.g., v_rec.order_id → single placeholder)
                let _saved_i = i;
                let mut j = i;
                while j < len && bytes[j] == b' ' {
                    j += 1;
                }
                if j < len && bytes[j] == b'.' {
                    j += 1;
                    while j < len && bytes[j] == b' ' {
                        j += 1;
                    }
                    if j < len && (bytes[j] == b'_' || bytes[j].is_ascii_alphabetic()) {
                        let field_start = j;
                        j += 1;
                        while j < len && (bytes[j] == b'_' || bytes[j].is_ascii_alphanumeric()) {
                            j += 1;
                        }
                        let field = &sql[field_start..j];
                        result.push_str(&format!("{}{}_{}__", prefix, type_str, field));
                        i = j;
                        continue;
                    }
                }
                result.push_str(&format!("{}{}__", prefix, type_str));
            } else {
                result.push_str(ident);
            }
            continue;
        }

        result.push(c as char);
        i += 1;
    }

    result
}

/// Replace PL/pgSQL variable references with `__SQL_PARAM_` (bind-variable semantics).
fn replace_pl_vars_in_sql(sql: &str, vars: &std::collections::HashMap<String, Option<String>>) -> String {
    replace_pl_vars_in_sql_with_prefix(sql, vars, "__SQL_PARAM_")
}

/// Replace PL/pgSQL variable references with `__SQL_RAW_` (string-interpolation semantics).
fn replace_pl_vars_in_sql_raw(sql: &str, vars: &std::collections::HashMap<String, Option<String>>) -> String {
    replace_pl_vars_in_sql_with_prefix(sql, vars, "__SQL_RAW_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vars(vars: &[(&str, Option<&str>)]) -> std::collections::HashMap<String, Option<String>> {
        vars.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.map(|s| s.to_string()))).collect()
    }

    #[test]
    fn test_single_var_in_where() {
        let vars = make_vars(&[("p_account_id", Some("INTEGER"))]);
        assert_eq!(
            replace_pl_vars_in_sql("UPDATE accounts SET frozen_flag = 'Y' WHERE account_id = p_account_id", &vars,),
            "UPDATE accounts SET frozen_flag = 'Y' WHERE account_id = __SQL_PARAM_INTEGER_p_account_id__",
        );
    }

    #[test]
    fn test_multiple_vars() {
        let vars = make_vars(&[("p_name", Some("VARCHAR")), ("p_age", Some("INTEGER"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM users WHERE name = p_name AND age = p_age", &vars,),
            "SELECT * FROM users WHERE name = __SQL_PARAM_VARCHAR_p_name__ AND age = __SQL_PARAM_INTEGER_p_age__",
        );
    }

    #[test]
    fn test_var_without_type() {
        let vars = make_vars(&[("v_result", None)]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE id = v_result", &vars),
            "SELECT * FROM t WHERE id = __SQL_PARAM_v_result__",
        );
    }

    #[test]
    fn test_mixed_typed_and_untyped() {
        let vars = make_vars(&[("p_id", Some("INT")), ("v_count", None)]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE id = p_id AND cnt > v_count", &vars,),
            "SELECT * FROM t WHERE id = __SQL_PARAM_INT_p_id__ AND cnt > __SQL_PARAM_v_count__",
        );
    }

    #[test]
    fn test_case_insensitive_var_match() {
        let vars = make_vars(&[("P_ACCOUNT_ID", Some("INTEGER"))]);
        assert_eq!(
            replace_pl_vars_in_sql("WHERE account_id = p_account_id", &vars,),
            "WHERE account_id = __SQL_PARAM_INTEGER_p_account_id__",
        );
    }

    #[test]
    fn test_uppercase_var_in_sql() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(replace_pl_vars_in_sql("WHERE id = P_ID", &vars), "WHERE id = __SQL_PARAM_INT_P_ID__",);
    }

    #[test]
    fn test_var_in_single_quote_string_not_replaced() {
        let vars = make_vars(&[("p_name", Some("VARCHAR"))]);
        assert_eq!(
            replace_pl_vars_in_sql("INSERT INTO logs (msg) VALUES ('p_name was here')", &vars,),
            "INSERT INTO logs (msg) VALUES ('p_name was here')",
        );
    }

    #[test]
    fn test_var_adjacent_to_string_literal() {
        let vars = make_vars(&[("p_status", Some("VARCHAR"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE status = p_status AND msg = 'active'", &vars,),
            "SELECT * FROM t WHERE status = __SQL_PARAM_VARCHAR_p_status__ AND msg = 'active'",
        );
    }

    #[test]
    fn test_escaped_quote_in_string() {
        let vars = make_vars(&[("p_val", Some("TEXT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE name = 'it''s p_val' AND val = p_val", &vars,),
            "SELECT * FROM t WHERE name = 'it''s p_val' AND val = __SQL_PARAM_TEXT_p_val__",
        );
    }

    #[test]
    fn test_var_in_double_quote_not_replaced() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT \"p_id\" FROM t WHERE id = p_id", &vars,),
            "SELECT \"p_id\" FROM t WHERE id = __SQL_PARAM_INT_p_id__",
        );
    }

    #[test]
    fn test_var_as_substring_of_column_not_replaced() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE p_id_extra = 1 AND id = p_id", &vars,),
            "SELECT * FROM t WHERE p_id_extra = 1 AND id = __SQL_PARAM_INT_p_id__",
        );
    }

    #[test]
    fn test_var_prefix_of_another_not_replaced() {
        let vars = make_vars(&[("p", Some("INT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE id = p_name", &vars),
            "SELECT * FROM t WHERE id = p_name",
        );
    }

    #[test]
    fn test_column_name_same_pattern_preserved() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT account_id FROM accounts WHERE account_id = p_id", &vars,),
            "SELECT account_id FROM accounts WHERE account_id = __SQL_PARAM_INT_p_id__",
        );
    }

    #[test]
    fn test_no_vars_no_change() {
        let vars = make_vars(&[]);
        assert_eq!(replace_pl_vars_in_sql("SELECT * FROM t WHERE id = 1", &vars), "SELECT * FROM t WHERE id = 1",);
    }

    #[test]
    fn test_empty_sql() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(replace_pl_vars_in_sql("", &vars), "");
    }

    #[test]
    fn test_unrelated_vars_not_touched() {
        let vars = make_vars(&[("v_x", Some("INT"))]);
        assert_eq!(replace_pl_vars_in_sql("SELECT * FROM t WHERE id = v_y", &vars), "SELECT * FROM t WHERE id = v_y",);
    }

    #[test]
    fn test_underscore_var_name() {
        let vars = make_vars(&[("v_total_count", Some("BIGINT"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t WHERE cnt = v_total_count", &vars,),
            "SELECT * FROM t WHERE cnt = __SQL_PARAM_BIGINT_v_total_count__",
        );
    }

    #[test]
    fn test_var_in_set_clause() {
        let vars = make_vars(&[("p_flag", Some("CHAR")), ("p_id", Some("INTEGER"))]);
        assert_eq!(
            replace_pl_vars_in_sql("UPDATE t SET flag = p_flag WHERE id = p_id", &vars,),
            "UPDATE t SET flag = __SQL_PARAM_CHAR_p_flag__ WHERE id = __SQL_PARAM_INTEGER_p_id__",
        );
    }

    #[test]
    fn test_var_as_function_arg() {
        let vars = make_vars(&[("p_limit", Some("INTEGER"))]);
        assert_eq!(
            replace_pl_vars_in_sql("SELECT * FROM t LIMIT p_limit", &vars,),
            "SELECT * FROM t LIMIT __SQL_PARAM_INTEGER_p_limit__",
        );
    }

    #[test]
    fn test_same_var_multiple_times() {
        let vars = make_vars(&[("p_id", Some("INT"))]);
        assert_eq!(
            replace_pl_vars_in_sql(
                "SELECT * FROM t1 WHERE id = p_id UNION SELECT * FROM t2 WHERE ref_id = p_id",
                &vars,
            ),
            "SELECT * FROM t1 WHERE id = __SQL_PARAM_INT_p_id__ UNION SELECT * FROM t2 WHERE ref_id = __SQL_PARAM_INT_p_id__",
        );
    }

    #[test]
    fn test_extract_variables_sql_param() {
        let sql = "WHERE id = __SQL_PARAM_INT_p_id__ AND name = __SQL_PARAM_VARCHAR_p_name__";
        let vars = extract_variables(sql);
        assert!(vars.contains("__SQL_PARAM_INT_p_id__"), "got: {}", vars);
        assert!(vars.contains("__SQL_PARAM_VARCHAR_p_name__"), "got: {}", vars);
    }

    #[test]
    fn test_extract_variables_mixed_prefixes() {
        let sql = "WHERE id = __SQL_PARAM_INT_p_id__ AND x = __XML_PARAM_id__ AND y = __JAVA_VAR_String_name__";
        let vars = extract_variables(sql);
        assert!(vars.contains("__SQL_PARAM_INT_p_id__"), "got: {}", vars);
        assert!(vars.contains("__XML_PARAM_id__"), "got: {}", vars);
        assert!(vars.contains("__JAVA_VAR_String_name__"), "got: {}", vars);
    }

    #[test]
    fn test_replace_pl_vars_raw() {
        let vars = make_vars(&[("v_sql", Some("VARCHAR2"))]);
        assert_eq!(
            replace_pl_vars_in_sql_raw("SELECT * FROM t WHERE id = v_sql", &vars),
            "SELECT * FROM t WHERE id = __SQL_RAW_VARCHAR2_v_sql__",
        );
    }

    #[test]
    fn test_replace_pl_vars_raw_no_type() {
        let vars = make_vars(&[("v_sql", None)]);
        assert_eq!(replace_pl_vars_in_sql_raw("v_sql", &vars), "__SQL_RAW_v_sql__",);
    }

    #[test]
    fn test_replace_pl_vars_raw_concat() {
        let vars = make_vars(&[("v_table", Some("VARCHAR2"))]);
        assert_eq!(
            replace_pl_vars_in_sql_raw("'SELECT * FROM ' || v_table", &vars),
            "'SELECT * FROM ' || __SQL_RAW_VARCHAR2_v_table__",
        );
    }

    #[test]
    fn test_extract_variables_sql_raw() {
        let sql = "EXECUTE IMMEDIATE __SQL_RAW_VARCHAR2_v_sql__";
        let vars = extract_variables(sql);
        assert!(vars.contains("__SQL_RAW_VARCHAR2_v_sql__"), "got: {}", vars);
    }

    #[test]
    fn test_find_using_keyword_pos_chinese_aliases_no_panic() {
        // Regression: byte-index slicing on multi-byte UTF-8 (Chinese) caused panic.
        // "账号类型" starts at byte 110, char '账' occupies bytes 110..113.
        // A byte-by-byte scan would land on byte 111 (mid-character) and panic on &str[..].
        let sql = "select * from ( select decode ( t . if_inter_bank , '1' , '银行间资金结算账户' , '银行存款类' ) 账号类型 , t . fund_name 组合 , t . accno 账户 from t ) using p_min_amount, p_start_date";
        let pos = find_using_keyword_pos(sql);
        assert!(pos.is_some(), "should find USING keyword");
        let p = pos.unwrap();
        assert!(sql[p..].to_ascii_lowercase().starts_with("using "), "slice at pos must start with 'using '");
    }

    #[test]
    fn test_find_using_keyword_pos_pure_chinese_no_using() {
        let sql = "select 账号类型 , 组合 , 账户名称 from dual";
        assert_eq!(find_using_keyword_pos(sql), None);
    }

    #[test]
    fn test_find_using_keyword_pos_chinese_inside_string_skipped() {
        // "using " inside a string literal must be ignored
        let sql = "select 'using 中文测试' from dual using p_id";
        let pos = find_using_keyword_pos(sql);
        assert!(pos.is_some());
        let p = pos.unwrap();
        assert!(sql[p..].to_ascii_lowercase().starts_with("using "));
    }

    #[test]
    fn test_find_using_keyword_pos_mixed_multibyte() {
        // Mix of Chinese, Japanese, Korean with USING at end
        let sql = "select 账号, データ, 데이터 from t using v_id";
        let pos = find_using_keyword_pos(sql);
        assert!(pos.is_some());
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_positive_mapper_tag() {
        assert!(is_likely_mapper_xml(b"<mapper namespace=\"t\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_positive_sqlmap_tag() {
        assert!(is_likely_mapper_xml(b"<sqlMap namespace=\"t\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_positive_doctype_mapper() {
        assert!(is_likely_mapper_xml(b"<!DOCTYPE mapper PUBLIC \"-//ibatis.apache.org//DTD Mapper 3.0//EN\" \"http://ibatis.apache.org/dtd/ibatis-3-mapper.dtd\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_positive_doctype_sqlmap() {
        assert!(is_likely_mapper_xml(b"<!DOCTYPE sqlMap PUBLIC \"-//ibatis.apache.org//DTD SQL Map Config 2.0//EN\" \"http://ibatis.apache.org/dtd/sql-map-config-2.dtd\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_mappers_plural_alone_does_not_match() {
        // <mappers> without a <mapper child tag should not match
        assert!(!is_likely_mapper_xml(b"<mappers></mappers>"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_sqlmapconfig_alone_does_not_match() {
        // <sqlMapConfig> without a <sqlMap child tag should not match
        assert!(!is_likely_mapper_xml(b"<sqlMapConfig></sqlMapConfig>"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_negative_pom_xml() {
        assert!(!is_likely_mapper_xml(b"<project xmlns=\"http://maven.apache.org/POM/4.0.0\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_negative_web_xml() {
        assert!(!is_likely_mapper_xml(b"<web-app xmlns=\"http://xmlns.jcp.org/xml/ns/javaee\">"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_negative_empty() {
        assert!(!is_likely_mapper_xml(b""));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_negative_short_file() {
        assert!(!is_likely_mapper_xml(b"abc"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_mappers_plural_with_inner_mapper_matches() {
        // mybatis-config.xml references actual <mapper resource=...> tags
        assert!(is_likely_mapper_xml(b"<mappers><mapper resource=\"a.xml\"/></mappers>"));
    }

    #[cfg(feature = "ibatis")]
    #[test]
    fn test_is_likely_mapper_xml_self_closing_mapper_matches() {
        // <mapper/> has the tag name, even if it's an empty/self-closing element
        assert!(is_likely_mapper_xml(b"<mapper/>"));
    }
}
