# OGSQL Parser / OGSQL 解析器

> A hand-written recursive descent SQL parser for openGauss/GaussDB written in Rust  
> 使用 Rust 编写的手写递归下降 SQL 解析器，支持 openGauss/GaussDB

---

## Project Overview / 项目概述

This project implements a complete SQL parser for openGauss/GaussDB (an enterprise-class database system based on PostgreSQL). Built from scratch using pure recursive descent parsing techniques without relying on parser generators.

本项目为 openGauss/GaussDB（一款基于 PostgreSQL 的企业级数据库系统）实现了一个完整的 SQL 解析器。采用纯递归下降解析技术从零构建，不依赖解析器生成器。

---

## Completed Features / 已完成特性

### Phase 1: Foundation / 第一阶段：基础框架

| Component | Status | Details |
|-----------|--------|---------|
| Tokenizer / 分词器 | ✅ Complete | 724 keywords, comments, operators, literals |
| Multi-encoding support / 多字符集支持 | ✅ Complete | UTF-8, EUC-JP, EUC-KR, GB18030, BIG5, UTF-16 |
| AST / 抽象语法树 | ✅ Complete | 219+ AST types defined |
| Parser dispatcher / 解析器分发 | ✅ Complete | Top-level statement routing |
| Unit tests / 单元测试 | ✅ Complete | 1980+ tests |
| Regression tests / 回归测试 | ✅ Complete | 1409/1409 — All openGauss regression tests passing |
| JSON serde / JSON 序列化 | ✅ Complete | Full serde::Serialize + Deserialize on all AST types |

### Phase 2: Core DML / 第二阶段：核心 DML

| Component | Status | Details |
|-----------|--------|---------|
| SELECT | ✅ Complete | Full SELECT syntax including CTEs, subqueries, window functions, PIVOT/UNPIVOT |
| INSERT | ✅ Complete | INSERT VALUES, INSERT SELECT, ON CONFLICT |
| UPDATE | ✅ Complete | UPDATE with SET, FROM, WHERE, RETURNING |
| DELETE | ✅ Complete | DELETE with WHERE, RETURNING |
| MERGE | ✅ Complete | MERGE INTO with WHEN MATCHED/NOT MATCHED + semantic validation |
| Expressions / 表达式 | ✅ Complete | Pratt parser for full expression support |
| Formatter / 格式化器 | ✅ Complete | Two-stage formatter: structured AST pretty-print (subqueries, JOINs, CASE, scalar subqueries each on separate lines) + configurable token formatter (indent, keyword case, comma style, line width, DML/DDL/PL/pgSQL) |

### Phase 3: PL/pgSQL Support / 第三阶段：PL/pgSQL 支持

| Component | Status | Details |
|-----------|--------|---------|
| DO statement / DO语句 | ✅ Complete | `DO $$ ... $$` with dollar-quoted PL/pgSQL bodies |
| Anonymous blocks / 匿名块 | ✅ Complete | `DECLARE ... BEGIN ... END` and `BEGIN ... END` blocks |
| Declarations / 声明 | ✅ Complete | Variables, constants, cursors, RECORD, TYPE, %TYPE, %ROWTYPE |
| Assignment / 赋值 | ✅ Complete | `variable := expression` |
| IF/ELSIF/ELSE | ✅ Complete | Full conditional branching |
| CASE | ✅ Complete | Searched and plain CASE expressions |
| LOOP / WHILE / FOR / FOREACH | ✅ Complete | All loop constructs with labels |
| EXIT / CONTINUE | ✅ Complete | With optional WHEN conditions and labels |
| RETURN | ✅ Complete | With optional expression |
| RAISE | ✅ Complete | All levels: DEBUG, LOG, INFO, NOTICE, WARNING, EXCEPTION |
| EXECUTE / PERFORM | ✅ Complete | Dynamic SQL execution |
| Cursor operations / 游标操作 | ✅ Complete | OPEN, FETCH, CLOSE, MOVE |
| GET DIAGNOSTICS | ✅ Complete | Including STACKED DIAGNOSTICS |
| Exception handling / 异常处理 | ✅ Complete | WHEN ... THEN handlers |
| GOTO | ✅ Complete | GOTO label |
| Transaction control / 事务控制 | ✅ Complete | COMMIT, ROLLBACK, SAVEPOINT within blocks |
| FORALL / PIPE ROW | ✅ Complete | Bulk operations and pipe row |
| PL/pgSQL Formatter / 格式化 | ✅ Complete | Full formatting for all PL/pgSQL constructs |

### Phase 4: DDL / 第四阶段：DDL

| Component | Status | Details |
|-----------|--------|---------|
| CREATE TABLE | ✅ Complete | Columns, constraints, table options, partitioning, distribution |
| CREATE TABLE AS | ✅ Complete | CREATE TABLE ... AS SELECT |
| CREATE INDEX | ✅ Complete | Index creation with options |
| CREATE GLOBAL INDEX | ✅ Complete | Global index for distributed tables |
| CREATE VIEW | ✅ Complete | View definitions |
| CREATE MATERIALIZED VIEW | ✅ Complete | Materialized view with refresh |
| CREATE FUNCTION | ✅ Complete | Full CREATE FUNCTION with parameters, return type, options, PL/pgSQL body |
| CREATE PROCEDURE | ✅ Complete | Full CREATE PROCEDURE with parameters, options, PL/pgSQL body |
| CREATE PACKAGE / PACKAGE BODY | ✅ Complete | Oracle-compatible packages with procedures, functions, cursors |
| CREATE TRIGGER | ✅ Complete | BEFORE/AFTER/INSTEAD OF, events, WHEN condition, EXECUTE |
| CREATE SCHEMA | ✅ Complete | Schema creation |
| CREATE DATABASE | ✅ Complete | Database creation with options |
| CREATE DATABASE LINK | ✅ Complete | Database link for remote access |
| CREATE TABLESPACE | ✅ Complete | Tablespace management |
| CREATE SEQUENCE | ✅ Complete | Sequence with all options |
| CREATE TYPE | ✅ Complete | Composite, enum, range types |
| CREATE DOMAIN | ✅ Complete | Domain with constraints |
| CREATE CAST | ✅ Complete | Type cast definitions |
| CREATE EXTENSION | ✅ Complete | Extension management |
| CREATE ROLE / USER / GROUP | ✅ Complete | Role-based access control |
| CREATE FOREIGN TABLE / SERVER / FDW | ✅ Complete | Foreign data wrapper support |
| CREATE PUBLICATION / SUBSCRIPTION | ✅ Complete | Logical replication |
| CREATE SYNONYM | ✅ Complete | Oracle-compatible synonyms |
| CREATE AGGREGATE / OPERATOR | ✅ Complete | Custom aggregates and operators |
| CREATE MODEL | ✅ Complete | AI model management (PREDICT BY) |
| GaussDB-specific CREATE | ✅ Complete | NODE, NODE GROUP, RESOURCE POOL, WORKLOAD GROUP, AUDIT/MASKING/RLS POLICY, DATA SOURCE, EVENT, OP CLASS/FAMILY, STREAM, KEY, DIRECTORY, LANGUAGE, WEAK PASSWORD DICTIONARY, TEXT SEARCH CONFIG/DICT, CONT QUERY, APP WORKLOAD GROUP MAPPING |
| ALTER TABLE | ✅ Complete | Column operations, constraints, options, partitioning |
| ALTER INDEX / SEQUENCE / VIEW | ✅ Complete | Index, sequence, and view modifications |
| ALTER FUNCTION / PROCEDURE | ✅ Complete | Function and procedure modifications |
| ALTER SCHEMA / DATABASE / TABLESPACE | ✅ Complete | Schema, database, and tablespace modifications |
| ALTER ROLE / USER / GROUP | ✅ Complete | Role management |
| ALTER TRIGGER / EXTENSION | ✅ Complete | Trigger and extension modifications |
| ALTER FOREIGN TABLE / SERVER / FDW | ✅ Complete | Foreign data wrapper modifications |
| ALTER PUBLICATION / SUBSCRIPTION | ✅ Complete | Replication modifications |
| ALTER TYPE / DOMAIN | ✅ Complete | Type system modifications |
| GaussDB-specific ALTER | ✅ Complete | NODE, NODE GROUP, RESOURCE POOL, WORKLOAD GROUP, AUDIT/MASKING/RLS POLICY, DATA SOURCE, EVENT, OP FAMILY, OPERATOR, MATERIALIZED VIEW, GLOBAL CONFIG, SESSION, DATABASE LINK, DIRECTORY, LANGUAGE, PACKAGE, COORDINATOR, APP WORKLOAD GROUP MAPPING, SYNONYM, TEXT SEARCH CONFIG/DICT |
| DROP statements | ✅ Complete | DROP TABLE, INDEX, VIEW, FUNCTION, PROCEDURE, TRIGGER, SCHEMA, DATABASE, and 30+ other object types |
| TRUNCATE | ✅ Complete | TRUNCATE TABLE |
| COMMENT | ✅ Complete | COMMENT ON TABLE/COLUMN/... |
| GRANT / REVOKE | ✅ Complete | Privilege management with role support |

### Phase 5: Advanced Features / 第五阶段：高级特性

| Component | Status | Details |
|-----------|--------|---------|
| Dynamic SQL analysis / 动态SQL分析 | ✅ Complete | Variable tracing, EXECUTE IMMEDIATE resolution, parameterized output |
| MERGE semantic validation / MERGE语义校验 | ✅ Complete | Detects non-deterministic and invalid MERGE patterns |
| Schema loading / Schema 加载 | ✅ Complete | Load and resolve database schema from JSON |
| Return cursor analysis / 返回游标分析 | ✅ Complete | Analyze PL/pgSQL return cursors and result columns |
| PL variable validation / PL变量校验 | ✅ Complete | Detect undefined variables and functions in PL blocks |
| Query fingerprints / SQL指纹 | ✅ Complete | Deterministic fingerprints for query identification |
| Transaction analysis / 事务分析 | ✅ Complete | Analyze COMMIT/ROLLBACK patterns in PL blocks |
| Package consistency / 包一致性校验 | ✅ Complete | Validate PACKAGE vs PACKAGE BODY consistency |
| SQL Linter / SQL 反模式检测 | ✅ Complete | 53 anti-pattern rules across 4 severity levels (Prohibition / Performance / Caution / Suggestion), TOML config, index-aware rules, integrated into CLI / MCP / HTTP API |
| AST Visitor pattern / AST访问者模式 | ✅ Complete | Walk statements, PL blocks, expressions |

### Phase 6: Integrations / 第六阶段：集成

| Component | Status | Details |
|-----------|--------|---------|
| iBatis/MyBatis XML parsing / iBatis XML 解析 | ✅ Complete | Dynamic SQL AST, variant expansion, parameter type inference from Java |
| Java source SQL extraction / Java源文件SQL提取 | ✅ Complete | String concatenation, switch-expression, cross-method PreparedStatement backfill |
| Java connector / Java 连接器 | ✅ Complete | `java-connector/` Maven module: DuckDB-style jar packaging + stdio NDJSON channel (`ogsql serve-stdio`), JVM-safe process isolation with auto-restart. See [docs/java-developer-guide.md](docs/java-developer-guide.md) |
| MCP server / MCP服务器 | ✅ Complete | Model Context Protocol server for Claude Desktop, Cursor, etc. |
| HTTP API server / HTTP API 服务 | ✅ Complete | RESTful API for parse, format, validate, tokenize, json2sql |
| Interactive TUI / 交互式终端 | ✅ Complete | Terminal UI playground for live SQL parsing |
| Strict validation mode / 严格校验模式 | ✅ Complete | Detect undefined function calls in PL blocks |
| validate-xml / validate-java commands / XML与Java校验命令 | ✅ Complete | Dedicated validation commands for iBatis XML mappers and Java sources with CSV/stats output |
| serve API enhancements / HTTP API 增强 | ✅ Complete | `/api/parse-xml`, `/api/parse-java`, per-statement validation breakdown, tracing, multipart upload, Swagger UI (`serve`), and a lightweight `serve-minimal` feature |
| Oracle `(+)` outer-join marker / Oracle 外连接标记 | ✅ Complete | Preserved in AST for full round-trip fidelity |
| `Ident` quote-style preservation / 标识符引号保留 | ✅ Complete | `ObjectName` → `Vec<Ident>` retains quoted-identifier style across parse/format round-trip |
| JDBC `?` placeholder / JDBC 占位符 | ✅ Complete | Tokenizer recognizes `?` as `JdbcParam` for prepared-statement SQL |
| Windows 7 support / Windows 7 支持 | ✅ Complete | Tier 3 target `x86_64-win7-windows-msvc` with nightly + `-Zbuild-std` |

---

## Project Structure / 项目结构

```
ogsql-parser/
├── Cargo.toml              # Project configuration
├── src/
│   ├── lib.rs              # Library exports
│   ├── bin/
│   │   ├── ogsql.rs        # CLI binary
│   │   └── ogsql-mcp.rs    # MCP server binary
│   ├── formatter/          # SQL & PL/pgSQL formatter
│   ├── token_formatter.rs  # Token-level formatter with FormatConfig
│   ├── token/
│   │   ├── mod.rs          # Token types and spans
│   │   ├── tokenizer.rs    # Lexical analyzer
│   │   ├── keyword.rs      # 724 keyword definitions
│   │   └── encoding.rs     # Multi-encoding support
│   ├── ast/
│   │   ├── mod.rs          # 219+ AST type definitions
│   │   ├── plpgsql.rs      # PL/pgSQL AST types (40+ types)
│   │   └── visitor.rs      # AST visitor pattern
│   ├── parser/
│   │   ├── mod.rs          # Parser dispatch & main logic
│   │   ├── select.rs       # SELECT statement parser
│   │   ├── dml.rs          # INSERT, UPDATE, DELETE, MERGE
│   │   ├── ddl/            # DDL statement parsers (table, create, alter, drop)
│   │   ├── expr.rs         # Expression parser (Pratt)
│   │   ├── plpgsql.rs      # PL/pgSQL parser (25+ statement types)
│   │   ├── hint_validator.rs # Hint validation
│   │   ├── function_registry.rs # Built-in function registry
│   │   └── utility/        # Utility parsers (types, constraints, statements, etc.)
│   ├── analyzer/
│   │   ├── mod.rs          # Semantic analysis (dynamic SQL, fingerprints, MERGE, etc.)
│   │   ├── schema.rs       # Schema loading and resolution
│   │   └── return_cursor.rs # Return cursor analysis
│   ├── linter/             # SQL anti-pattern linter (53 rules, 4 severity levels)
│   ├── ibatis/             # iBatis/MyBatis XML parsing
│   ├── java/               # Java source SQL extraction
│   └── mcp/                # MCP server implementation
├── examples/
│   ├── regression.rs       # Run regression tests
│   ├── test_issues.rs      # Issue-specific test cases
│   └── test_loc.rs         # Line/column accuracy tests
├── tests/
│   ├── roundtrip.rs        # SQL ↔ JSON round-trip tests
│   ├── plpgsql.rs          # PL/pgSQL integration tests
│   ├── multi_statement.rs  # Multi-statement parsing tests
│   └── error_handling.rs   # Error handling tests
├── docs/
│   ├── user-guide.md       # User guide (CLI, HTTP API, MCP)
│   ├── crate-guide.md      # Crate developer guide (Rust API, AST JSON)
│   ├── ast-json-reference.md # AST JSON structure reference
│   └── plans/              # Implementation plans
├── lib/
│   └── openGauss-server/   # Reference source (git submodule)
└── docs/references/GaussDB-2.23.07.210/    # Documentation reference
```

---

## Getting Started / 快速开始

### Prerequisites / 环境要求

- Rust 1.71+ with Cargo

### Build / 编译

```bash
# Default build (CLI only)
cargo build --release --features cli

# With HTTP API server
cargo build --release --features serve

# With TUI playground
cargo build --release --features tui

# With MCP server
cargo build --release --features mcp

# All features
cargo build --release --features full
```

### Run Tests / 运行测试

```bash
# Unit tests (1980+ tests)
cargo test

# Regression tests against openGauss test suite
cargo run --example regression
```

### CLI Usage / 命令行使用

```
$ ogsql --help
openGauss/GaussDB SQL Parser

Usage: ogsql [OPTIONS] <COMMAND>

Commands:
  format      Format SQL statements with configurable options
  parse       Parse SQL into AST and print the abstract syntax tree / 解析 SQL 为 AST
  json2sql    Convert JSON (from `parse -j`) back to SQL / 将 JSON 还原为 SQL
  tokenize    Tokenize SQL into a list of tokens / 将 SQL 分词为 token 列表
  validate    Validate SQL syntax and report errors / 校验 SQL 语法
  validate-xml   Validate iBatis/MyBatis XML mappers [requires: ibatis feature]
  validate-java  Validate SQL extracted from Java source files [requires: java feature]
  parse-xml   Parse iBatis/MyBatis XML mapper → extracted SQL [requires: ibatis feature]
  parse-java  Extract and parse SQL from Java source files [requires: java feature]
  serve       Start an HTTP API server for parsing SQL [requires: serve feature]
  serve-stdio Start a stdio NDJSON line-protocol server for embedded clients (Java/Python/Node); no extra features required / 启动 stdio NDJSON 协议服务（供嵌入客户端长连接使用）
  playground  Launch an interactive terminal UI playground [requires: tui feature]
  mcp         Start the MCP (Model Context Protocol) server [requires: mcp feature]

Options:
  -f, --file <FILE>       Read SQL from file instead of stdin (can specify multiple times)
  -j, --json              Output in JSON format
  -v, --verbose           Verbose output
      --comments          Include comments in output
      --mybatis           Enable MyBatis #{param} and ${expr} placeholder support
      --schema-json <FILE> Schema JSON file for semantic analysis
      --lint              Run SQL anti-pattern linter (53 rules; configurable via .ogsql-lint.toml)
      --extract-sql       Extract dynamic SQL from PL/pgSQL (resolve EXECUTE IMMEDIATE, %TYPE)
  -h, --help              Print help
  -V, --version           Print version
```

#### parse — 解析 SQL

```bash
# Parse from stdin
echo "SELECT * FROM users" | ogsql parse

# Parse to JSON
echo "SELECT * FROM users" | ogsql parse -j

# Parse from file
ogsql -f query.sql parse -j

# Parse from directory (batch)
ogsql parse -d ./sql-files -j

# Parse with output directory (preserves directory structure)
ogsql parse -d ./sql-files -j -o ./output

# Parse only a specific procedure (filter errors by scope)
ogsql -f package.sql parse --procedure my_procedure

# Parse with statistics
ogsql parse -d ./sql-files --stats

# CSV output (flat, one row per statement)
ogsql parse -d ./sql-files --csv
```

#### format — 格式化 SQL

```
$ ogsql format --help
Format SQL statements with configurable indentation, keyword casing, comma style, and line width.
Supports SELECT, INSERT, DELETE, UPDATE, MERGE, WITH (CTE), CREATE TABLE, and PL/pgSQL.

Options:
  -i, --indent <INDENT>              Indentation width in spaces [default: 2]
  -k, --keyword-case <KEYWORD_CASE>  Keyword casing: preserve, upper, lower [default: preserve]
      --comma <COMMA>                Comma style: trailing, leading [default: trailing]
  -w, --line-width <LINE_WIDTH>      Maximum line width (0 = unlimited) [default: 120]
  -u, --uppercase                    Shorthand for --keyword-case upper
      --no-select-newline            Don't put each SELECT column on its own line
      --no-logical-newline           Don't put AND/OR on new lines
      --no-semicolon-newline         Don't put semicolons on their own line
      --mybatis                      Preserve MyBatis #{param} and ${expr} placeholders
```

#### Examples / 示例

```bash
# Tokenize SQL
echo "SELECT * FROM users" | ogsql tokenize

# Parse SQL to AST (debug format)
echo "SELECT * FROM users" | ogsql parse

# Parse SQL to AST (JSON format)
echo "SELECT * FROM users" | ogsql parse -j

# SQL → JSON → SQL round-trip
echo "SELECT id FROM users WHERE id = 1" | ogsql parse -j | ogsql json2sql

# Convert JSON file back to SQL
ogsql -f ast.json json2sql

# Format SQL (default: 2-space indent, keyword casing preserved)
echo "select id,name from users where id=1" | ogsql format

# Format with uppercase keywords and 4-space indent
echo "select * from users" | ogsql format -u -i 4

# Format with leading comma style
echo "SELECT id, name, age FROM users" | ogsql format --comma leading

# Format MyBatis SQL (preserves #{param} placeholders)
echo "SELECT * FROM users WHERE id = #{userId}" | ogsql format --mybatis

# Format INSERT with MyBatis parameters
echo "insert into t (a,b) values (#{x},#{y})" | ogsql format --mybatis

# Format DML statements
echo "update users set name='Bob', age=30 where id=1" | ogsql format
echo "delete from users where id = 1 and status = 'inactive'" | ogsql format
echo "merge into target t using source s on t.id=s.id when matched then update set t.name=s.name" | ogsql format
echo "with cte as (select id from users) select * from cte" | ogsql format

# Validate SQL syntax
echo "SELECT FROM" | ogsql validate

# Validate with strict mode (detects undefined functions in PL blocks)
echo "CREATE OR REPLACE FUNCTION test() RETURNS VOID AS \$\$ BEGIN PERFORM unknown_func(); END; \$\$ LANGUAGE plpgsql" | ogsql validate --strict

# Validate with the SQL anti-pattern linter (53 rules; Prohibition/Performance/Caution/Suggestion)
echo "SELECT * FROM users WHERE LEFT(name, 3) = 'abc'" | ogsql validate --lint

# Lint a directory with index-aware rules (auto-extracts DDL schema for R006/R007)
ogsql validate -d ./sql-files --lint --csv --stats

# Validate SQL files in a directory
ogsql validate -d ./sql-files --csv --stats

# Read from file
ogsql -f query.sql parse -j

# Start HTTP API server
ogsql serve --host 127.0.0.1 --port 8080

# Parse iBatis/MyBatis XML mapper (basic)
echo '<mapper namespace="test"><select id="find">SELECT * FROM t WHERE id = #{id}</select></mapper>' | ogsql parse-xml

# Parse XML mapper with CSV output
ogsql parse-xml --csv -f mapper/UserMapper.xml

# Parse XML mapper with structured dynamic SQL AST
ogsql parse-xml -f mapper/UserMapper.xml --structured

# Parse XML mapper with parameter type inference from Java source
ogsql parse-xml -d /path/to/mapper-xml --java-src /path/to/src/main/java --csv

# Parse XML in directory with stats
ogsql parse-xml -d ./mapper-xml --java-src ./src/main/java --stats --csv

# Extract SQL from Java source files
ogsql parse-java -d ./src/main/java --csv --stats

# Extract SQL with custom method patterns
ogsql parse-java -d ./src/main/java --extra-sql-methods "executeUpdate,executeQuery" --csv
```

#### HTTP API Endpoints / HTTP API 接口

When built with `--features serve`, the following endpoints are available:

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/health` | Health check |
| POST | `/api/parse` | Parse SQL → AST JSON (body: `{"sql": "..."}`) |
| POST | `/api/json2sql` | JSON → SQL (body: `{"json": "..."}`) |
| POST | `/api/format` | Format SQL with configurable options (body: `{"sql": "...", "indent": 2, "keyword_case": "upper", ...}`) |
| POST | `/api/tokenize` | Tokenize SQL (body: `{"sql": "..."}`) |
| POST | `/api/validate` | Validate SQL (body: `{"sql": "..."}`) |

#### MCP Server / MCP 服务器

When built with `--features mcp`, an MCP (Model Context Protocol) server binary is available:

```bash
# Build MCP server
cargo build --release --features mcp

# Run (stdio transport — for Claude Desktop, Cursor, etc.)
ogsql-mcp
```

##### MCP Tools / MCP 工具

| Tool | Description |
|------|-------------|
| `parse` | Parse SQL → AST JSON (with fingerprints, comments, errors) |
| `tokenize` | SQL → Token list with types, values, positions |
| `format` | Format SQL with configurable indent, keyword case, comma style, line width |
| `validate` | Validate SQL syntax, report errors/warnings |
| `json2sql` | Convert AST JSON back to SQL |
| `parse_xml` | Parse iBatis/MyBatis XML mapper → extracted SQL (with optional parameter type inference via `java_src`/`java_sources`) |
| `parse_java` | Extract SQL from Java source files |

##### Claude Desktop Configuration

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "ogsql": {
      "command": "/path/to/ogsql-mcp"
    }
  }
}
```

### Use as Library / 作为库使用

#### Basic Parsing / 基本解析

```rust
use ogsql_parser::{Tokenizer, parser::Parser};

fn main() {
    // Parse a simple SQL statement
    let sql = "SELECT id, name FROM users WHERE status = 'active'";
    let tokens = Tokenizer::new(sql).tokenize().unwrap();
    let statements = Parser::new(tokens).parse().unwrap();
    println!("Parsed {} statement(s)", statements.len());

    // Parse a PL/pgSQL DO block
    let plsql = "DO $$ BEGIN RAISE NOTICE 'hello'; END $$";
    let tokens = Tokenizer::new(plsql).tokenize().unwrap();
    let statements = Parser::new(tokens).parse().unwrap();
    println!("Parsed {} statement(s)", statements.len());
}
```

#### JSON Round-Trip (SQL ↔ JSON) / JSON 往返转换

All AST types implement `serde::Serialize` and `serde::Deserialize`, enabling lossless semantic round-tripping: parse SQL to AST, serialize to JSON, deserialize back to AST, and format back to semantically equivalent SQL.

所有 AST 类型实现了 `serde::Serialize` 和 `serde::Deserialize`，支持无损语义往返：将 SQL 解析为 AST，序列化为 JSON，反序列化回 AST，再格式化为语义等价的 SQL。

```
SQL ──parse──▶ AST ──serialize──▶ JSON ──deserialize──▶ AST ──format──▶ SQL'
                                    │                                       │
                                    └──────── semantically equivalent ────────┘
```

```rust
use ogsql_parser::{Tokenizer, Parser, SqlFormatter, Statement};

fn roundtrip(sql: &str) -> String {
    // 1. SQL → AST
    let tokens = Tokenizer::new(sql).tokenize().unwrap();
    let stmts = Parser::new(tokens).parse().unwrap();

    // 2. AST → JSON
    let json = serde_json::to_string(&stmts).unwrap();

    // 3. JSON → AST
    let restored: Vec<Statement> = serde_json::from_str(&json).unwrap();

    // 4. AST → SQL
    let formatter = SqlFormatter::new();
    let output: Vec<String> = restored.iter()
        .map(|s| formatter.format_statement(s))
        .collect();

    output.join(";\n")
}

fn main() {
    // DML round-trip
    let sql = "SELECT id, name FROM users WHERE status = 'active'";
    assert_eq!(roundtrip(sql), "SELECT id, name FROM users WHERE status = 'active'");

    // Special literal types are preserved (E'', B'', X'', N'', $$ $$)
    let sql2 = "SELECT E'\\ttext', B'1010', X'FF', N'unicode'";
    assert_eq!(roundtrip(sql2), "SELECT E'\\ttext', B'1010', X'FF', N'unicode'");

    // DDL round-trip
    let sql3 = "CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL)";
    assert_eq!(roundtrip(sql3), "CREATE TABLE t (id INTEGER PRIMARY KEY, name VARCHAR(100) NOT NULL)");

    // Window functions with frame clauses
    let sql4 = "SELECT ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)";
    assert_eq!(roundtrip(sql4), "SELECT ROW_NUMBER() OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)");
}
```

**What is preserved / 保留内容：**
- All SQL semantics (operators, expressions, subqueries, joins)
- Special literal types: escape strings (`E'...'`), bit strings (`B'...'`), hex strings (`X'...'`), national strings (`N'...'`), dollar-quoted strings (`$$...$$` / `$tag$...$tag$`)
- Type casts with parameterized types (`NUMERIC(10,2)`, `VARCHAR(100)`, `TIMESTAMP(3) WITH TIME ZONE`)
- Window frame specifications (`ROWS BETWEEN ... AND ...`)
- PL/pgSQL control flow (IF/WHILE/CASE/FOR/FOREACH conditions as structured expressions)
- DDL column types, constraints, domain definitions, cast types, RLS policy expressions

**What is NOT preserved / 不保留内容：**
- Comments and whitespace (non-semantic)
- Original keyword casing (formatter normalizes to uppercase)
- Original formatting/layout

#### Token-Level Formatting / Token 级格式化

```rust
use ogsql_parser::{Tokenizer, Parser, token_formatter::{FormatConfig, KeywordCase, CommaStyle}};

fn main() {
    let sql = "select id,name from users where id=1";
    let tokens = Tokenizer::new(sql).tokenize().unwrap();
    let statements = Parser::new(tokens).parse().unwrap();

    let config = FormatConfig {
        indent_width: 4,
        keyword_case: KeywordCase::Upper,
        comma_style: CommaStyle::Leading,
        line_width: 100,
        ..Default::default()
    };

    for stmt in &statements {
        let formatted = config.format_statement(stmt);
        println!("{}", formatted);
    }
}
```

#### Semantic Analysis / 语义分析

```rust
use ogsql_parser::{
    Tokenizer, Parser,
    validate_pl_variables, validate_merge_semantics,
    compute_query_fingerprints, analyze_transactions,
};

fn main() {
    let sql = "MERGE INTO target t USING source s ON t.id = s.id
               WHEN MATCHED THEN UPDATE SET t.name = s.name
               WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)";

    let tokens = Tokenizer::new(sql).tokenize().unwrap();
    let statements = Parser::new(tokens).parse().unwrap();

    // MERGE semantic validation — detects non-deterministic and invalid patterns
    for stmt in &statements {
        if let Some(errors) = validate_merge_semantics(stmt) {
            for err in &errors {
                println!("MERGE error: {:?}", err);
            }
        }
    }

    // Compute query fingerprints for SQL identification
    let fingerprints = compute_query_fingerprints(&statements);
    for fp in &fingerprints {
        println!("Fingerprint: {}", fp);
    }
}
```

#### AST Visitor Pattern / AST 访问者模式

```rust
use ogsql_parser::{
    Tokenizer, Parser, Expr,
    visitor::{Visitor, VisitorResult, walk_statement},
};

// Custom visitor: collect all function calls
struct FunctionCollector {
    functions: Vec<String>,
}

impl Visitor for FunctionCollector {
    fn visit_expr(&mut self, expr: &Expr) -> VisitorResult {
        match expr {
            Expr::FunctionCall { name, .. } => {
                self.functions.push(name.join("."));
            }
            _ => {}
        }
        VisitorResult::Continue
    }
}

fn main() {
    let sql = "SELECT count(*), max(score) FROM users GROUP BY dept";
    let tokens = Tokenizer::new(sql).tokenize().unwrap();
    let statements = Parser::new(tokens).parse().unwrap();

    let mut collector = FunctionCollector { functions: vec![] };
    for stmt in &statements {
        walk_statement(&mut collector, stmt);
    }

    println!("Functions found: {:?}", collector.functions);
    // Output: Functions found: ["count", "max"]
}
```

#### iBatis/MyBatis XML Parsing / iBatis XML 解析

When built with `--features ibatis`, MyBatis XML mapper parsing is available:

```rust
use ogsql_parser::ibatis;

fn main() {
    let xml = br#"<mapper namespace="com.example.UserMapper">
        <select id="findById" parameterType="com.example.User">
            SELECT * FROM users WHERE id = #{id} AND name = #{name}
        </select>
    </mapper>"#;

    let result = ibatis::parse_mapper_bytes(xml);
    for stmt in &result.statements {
        println!("{}: {}", stmt.id, stmt.flat_sql);
        for param in &stmt.parameters {
            println!("  param {} -> {:?}", param.name, param.jdbc_type);
        }
    }
}
```

With `--features ibatis,java`, parameter types are inferred from Java source:

```rust
let result = ibatis::parse_mapper_bytes_with_java_src(
    xml, None, vec![std::path::PathBuf::from("/project/src/main/java")]
);
```

---

## Architecture Overview / 架构概览

```
SQL Input
    |
    v
+---------------+     +------------------+     +---------------+     +--------+     +-----------+
|  Tokenizer    | --> |  Token Stream    | --> |    Parser     | --> |  AST   | --> | Formatter |
|  (Lexer)      |     |  (Vec<Token>)    |     |  (Recursive   |     |        |     |           |
|               |     |                  |     |   Descent)    |     |  serde |     |           |
+---------------+     +------------------+     +---------------+     +--------+     +-----------+
                                                                          │  ▲
                                                                          │  │
                                                                    JSON serialize/deserialize
                                                                          │  │
                                                                          ▼  │
                                                                    +-----------+
                                                                    |   JSON    |
                                                                    +-----------+
```

### Key Design Decisions / 关键设计决策

1. **No parser generators / 无解析器生成器**  
   Hand-written recursive descent for full control over error messages and recovery

2. **Pratt parsing for expressions / 使用 Pratt 解析表达式**  
   Handles operator precedence naturally without left-recursion issues

3. **Zero-copy where possible / 尽可能零拷贝**  
   Tokens reference source string slices

4. **Multi-encoding support / 多字符集支持**  
   Automatic detection and conversion of UTF-8, EUC-JP, EUC-KR, GB18030, BIG5, UTF-16

5. **Unified parser core / 统一解析器核心**  
   All interfaces (CLI, HTTP API, TUI, MCP) share the same parser core

6. **Full serde support / 完整 serde 支持**  
   All AST types derive `Serialize` + `Deserialize`, enabling lossless JSON round-trip (SQL → JSON → SQL)

7. **Built-in function registry / 内置函数注册表**  
   981 registered built-in functions with category and domain metadata across 44 domains, with per-function distribution markers distinguishing open-source openGauss from commercial GaussDB

---

## Benchmarks / 性能对比

Full benchmark reports (data collected 2026-06, Linux x86_64, Rust 1.96) are in [`docs/reports/`](docs/reports/). Headline numbers below come from **9,612 GaussDB official regression statements × 5 iterations** (48,060 parses) across 4 parsers spanning Rust / C+Python / Java runtimes.

完整对比报告见 [`docs/reports/`](docs/reports/)。以下关键数据来自 **GaussDB 官方回归测试集 9,612 句 × 5 迭代**（共 48,060 次解析），横跨 Rust / C+Python / Java 四个运行时。

### Performance Summary / 性能总览

| Parser | Runtime | Throughput (q/s) | Median latency | P99 latency | Cold start | Memory (RSS) |
|---|---|---|---|---|---|---|
| **ogsql-parser** | Rust | **356,866** | **1.97 µs** | **11.30 µs** | **2.5 ms** | **6.2 MB** |
| sqlparser-rs 0.52 | Rust | 145,953 | 4.93 µs | 32.47 µs | 2.5 ms | 5.8 MB |
| pglast 7.14 (libpg_query) | C + Python | 28,165 | 29.89 µs | 148.00 µs | ~51 ms | 28.0 MB |
| JSqlParser 5.3 | Java (JVM) | 843 | 343.38 µs | 1,665.49 µs | ~308 ms | 116.2 MB |

- **2.4× faster** than sqlparser-rs (same language, comparable). Cross-category speedup is stable at **1.9×–3.1×**, confirming a structural advantage (compact AST, GaussDB-specific fast paths, zero-copy token stream).
- **12.7× faster** than pglast; **423× faster** than JSqlParser.

### Parse Success Rate / 解析成功率

| Corpus | ogsql-parser | sqlparser-rs | pglast | JSqlParser |
|---|---|---|---|---|
| GaussDB official (9,612 stmts) | **100%** | 100% | 79.2% | 78.3% |
| MyBatis-Plus realistic (30 stmts) | **100%** | 100% | 56.7% | 100% |

- pglast / JSqlParser fail on openGauss extensions: `DISTRIBUTE BY`, range `PARTITION`, Hint comments `/*+ IndexScan(t) */`, Oracle-style `IDENTIFIED BY`, `VARRAY`, partition-targeted `INSERT`.
- In the MyBatis-Plus scenario pglast degrades sharply because PostgreSQL treats `user` / `order` as reserved keywords and rejects MySQL-isms (`LIMIT m, n`, `ON DUPLICATE KEY UPDATE`).

### AST Equivalence / AST 等价性

On 13 canonical SQL patterns (SELECT/JOIN/CTE/subquery/window/UNION/INSERT/UPDATE/DELETE), all four parsers produced **semantically equivalent views on 11–13/13 cases** — i.e. static-analysis tools built on any of them agree on standard SQL.

### Stress Tests / 极限压力测试

| Input | ogsql | sqlparser-rs | pglast | JSqlParser |
|---|---|---|---|---|
| 10K-row INSERT (168 KB) | **12.5 ms** | 22.2 ms | 71.9 ms | 360.4 ms |
| 200-column SELECT | **123 µs** | 336 µs | 1,372 µs | 4,057 µs |
| 50-level nested subquery | **76 µs** | 80 µs | 953 µs | 5,398 µs |
| Error input (unterminated) | **0.3 µs** | 0.5 µs | 3.5 µs | 405+ µs |

ogsql-parser scales linearly at ~30–75 ns/byte and exits early on empty/degenerate input (0.1 µs), which matters for batch SQL validation tools.

### Why ogsql-parser Wins / 优势来源

1. Single target dialect → no dialect-trait indirection
2. Explicit two-phase `Tokenizer` + `Parser` → reusable token stream
3. More compact AST (shared structs in common cases)
4. GaussDB-specific syntax (PL/pgSQL, hints, partitioning) on a dedicated fast path

> Selection guidance: choose **ogsql-parser** for openGauss/GaussDB + Rust + high QPS; **sqlparser-rs** for cross-dialect Rust; **pglast** for pure PostgreSQL + Python; **JSqlParser** for the Java/MyBatis ecosystem. Full decision matrix and reproduction steps in [`docs/reports/`](docs/reports/).

---

## Development Roadmap / 开发路线图

| Phase | Focus | Status |
|-------|-------|--------|
| Phase 1 | Foundation: Tokenizer, AST, dispatcher, multi-encoding | ✅ Complete |
| Phase 2 | Core DML: SELECT, INSERT, UPDATE, DELETE, MERGE | ✅ Complete |
| Phase 3 | PL/pgSQL: DO blocks, anonymous blocks, control flow | ✅ Complete |
| Phase 4 | DDL: CREATE, ALTER, DROP statements | ✅ Complete |
| Phase 5 | Advanced: Semantic analysis, dynamic SQL, MERGE validation | ✅ Complete |
| Phase 6 | Integrations: iBatis XML, Java SQL, MCP, strict validation | ✅ Complete |
| Phase 7 | Optimization: Error recovery, performance | 📋 Planned |

---

## Technology Stack / 技术栈

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust 2021 | Safety, performance, ecosystem |
| Error handling | `thiserror` 2.0 | Ergonomic error types |
| Encoding | `encoding_rs` 0.8 | Multi-character set support (EUC-JP, EUC-KR, GB18030, etc.) |
| CLI | `clap` 4 | Derive-based argument parsing |
| HTTP API | `axum` 0.8 | Async web framework |
| MCP | `rmcp` 1.5 | Model Context Protocol server |
| TUI | `ratatui` 0.29 | Terminal UI framework |
| XML parsing | `quick-xml` 0.39 | Fast XML parser for iBatis mappers |
| Java parsing | `tree-sitter-java` 0.23 | Incremental Java parsing |
| Keyword lookup | `phf` 0.11 | Compile-time perfect hash for O(1) keyword matching |
| Testing | Built-in + walkdir 2.0 | Regression test file discovery |
| Win7 build | nightly + `-Zbuild-std` | Tier 3 Windows 7 target support |

---

## References / 参考资料

This project references the following openGauss source files:

- `lib/openGauss-server/src/backend/parser/gram.y` (35,325 lines) — Main SQL grammar specification
- `lib/openGauss-server/src/common/pl/plpgsql/src/gram.y` (15,770 lines) — PL/pgSQL grammar specification
- `lib/openGauss-server/src/backend/parser/scan.l` — Lexer specification
- `lib/openGauss-server/src/include/parser/kwlist.h` — Keyword definitions (724 keywords)
- `lib/openGauss-server/src/test/regress/sql/` — 1,397 regression test files

Documentation reference:
- `docs/references/GaussDB-2.23.07.210/term/` — GaussDB terminology and specifications

---

## Documentation / 文档

| Document | Audience | Description |
|----------|----------|-------------|
| [User Guide](docs/user-guide.md) | 终端用户 | CLI、HTTP API、MCP 使用指南 |
| [Crate Developer Guide](docs/crate-guide.md) | Rust 开发者 | Crate API 使用、AST JSON 参考、错误类型参考 |
| [AST JSON Reference](docs/ast-json-reference.md) | AST 消费者 | `ogsql parse -j` 输出的完整 JSON 结构 |
| [Contributing](CONTRIBUTING.md) | 项目贡献者 | 架构概览、从源码构建、开发规范 |
| [GaussDB SQL Features](docs/gaussdb-sql-features.md) | 全部 | GaussDB/openGauss SQL 语法支持详情 |
| [Benchmark Reports](docs/reports/) | 全部 | ogsql-parser vs sqlparser-rs / pglast / JSqlParser 性能、成功率、AST 等价性对比 |

---

## License / 许可证

MIT OR Apache-2.0

---

## Contributing / 贡献

This is an active development project. Phases 1–6 are complete, and Phase 7 (Optimization) is planned.

这是一个活跃的开发项目。第一至第六阶段已完成，第七阶段（优化）已规划。

---

**Status / 状态**: Phase 1–6 Complete | 1980+ unit tests | 1409/1409 regression tests passing | 53 lint rules  
**Version / 版本**: 0.10.1
**Last Updated / 最后更新**: 2026-06-27
