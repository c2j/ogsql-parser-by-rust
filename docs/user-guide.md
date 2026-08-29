# OGSQL Parser 用户指南

> 版本 0.8.14 | 适用于 openGauss / GaussDB SQL 解析器

本文档面向 **终端用户** — 使用命令行工具、HTTP API 或 MCP 服务器的人群。

- 如果你是 **crate 开发者**（将 ogsql-parser 作为 Rust 依赖或使用其 AST JSON 输出），请参阅 [Crate 开发者指南](./crate-guide.md)
- 如果你是 **项目贡献者**（参与 ogsql-parser 开发），请参阅 [贡献指南](../CONTRIBUTING.md)

---

## 目录

- [1. 简介](#1-简介)
- [2. 安装](#2-安装)
- [3. 命令行工具 (CLI)](#3-命令行工具-cli)
  - [3.1 总览](#31-总览)
  - [3.2 parse — 解析 SQL](#32-parse--解析-sql)
  - [3.3 tokenize — 分词](#33-tokenize--分词)
  - [3.4 format — 格式化](#34-format--格式化)
  - [3.5 validate — 语法校验](#35-validate--语法校验)
  - [3.6 json2sql — JSON 还原 SQL](#36-json2sql--json-还原-sql)
  - [3.7 parse-xml — 解析 iBatis/MyBatis XML](#37-parse-xml--解析-ibatismybatis-xml)
  - [3.8 parse-java — 提取 Java 中的 SQL](#38-parse-java--提取-java-中的-sql)
  - [3.9 serve — HTTP API 服务器](#39-serve--http-api-服务器)
  - [3.10 playground — 交互式终端](#310-playground--交互式终端)
- [4. MCP 服务器](#4-mcp-服务器)
  - [4.1 构建与启动](#41-构建与启动)
  - [4.2 可用工具](#42-可用工具)
  - [4.3 配置 Claude Desktop](#43-配置-claude-desktop)
- [7. 支持的 SQL 语法](#7-支持的-sql-语法)
  - [7.1 DML 语句](#71-dml-语句)
  - [7.2 DDL 语句](#72-ddl-语句)
  - [7.3 PL/pgSQL 语句](#73-plpgsql-语句)
  - [7.4 其他语句](#74-其他语句)
- [8. 多字符集支持](#8-多字符集支持)
- [10. 常见问题 (FAQ)](#10-常见问题-faq)
- [11. SQL 反模式检测 (Lint)](#11-sql-反模式检测-lint)
  - [11.1 快速开始](#111-快速开始)
  - [11.2 命令行选项](#112-命令行选项)
  - [11.3 配置文件](#113-配置文件)
  - [11.4 规则清单](#114-规则清单)
- [13. 语义校验规则](#13-语义校验规则)
  - [13.1 PL 变量校验（严格模式）](#131-pl-变量校验严格模式)
  - [13.2 MERGE 语义校验](#132-merge-语义校验)
  - [13.3 包一致性校验](#133-包一致性校验)

---

## 1. 简介

OGSQL Parser 是一个使用 Rust 编写的 SQL 解析器，专为 openGauss / GaussDB（基于 PostgreSQL 的企业级数据库）设计。它采用手写递归下降解析技术，提供以下核心能力：

- **SQL 解析**：将 SQL 文本解析为抽象语法树 (AST)，支持 180+ 种语句类型
- **SQL 格式化**：可配置缩进、关键字大小写、逗号风格、行宽等
- **SQL 校验**：检测语法错误并报告详细的错误位置
- **分词 (Tokenize)**：将 SQL 拆解为 Token 序列，支持 717 个关键字
- **JSON 往返转换**：SQL → AST → JSON → AST → SQL 的无损语义转换
- **PL/pgSQL 支持**：完整的 PL/pgSQL 过程化语言解析，包括控制流、游标、异常处理等
- **iBatis/MyBatis XML 解析**：从 XML Mapper 文件中提取 SQL，支持参数类型推断
- **Java 源文件 SQL 提取**：从 Java 代码中提取并解析嵌入的 SQL 语句
- **多字符集支持**：自动检测和转换 UTF-8、EUC-JP、EUC-KR、GB18030、BIG5、UTF-16
- **MCP 服务器**：作为 AI 工具服务器，与 Claude Desktop、Cursor 等 AI 编辑器集成
- **SQL 反模式检测 (Lint)**：内置 61 条规则，覆盖禁止项、性能、注意事项和建议四个等级

该项目提供了多种使用方式：命令行工具、HTTP API 服务、交互式终端以及 MCP 服务器。

> 如需在 Rust 项目中以 crate 依赖方式使用，或程序化处理 AST JSON 输出，请参阅 [Crate 开发者指南](./crate-guide.md)。

---

## 2. 安装

### 2.1 环境要求

- **Rust 1.71+** 及 Cargo（用于从源码编译）

### 2.2 获取二进制

```bash
# 从源码编译 CLI 工具
cargo build --release --features cli

# 编译完成后，二进制文件位于 target/release/ogsql
```

如需编译带 HTTP API、TUI 或 MCP 功能的版本：

```bash
# CLI + HTTP API 服务器
cargo build --release --features serve

# 全部功能
cargo build --release --features full

# MCP 服务器
cargo build --release --features mcp
```

> 详细的 Feature 说明、Windows 7 构建方式等，请参阅 [贡献指南](../CONTRIBUTING.md)。

---

## 3. 命令行工具 (CLI)

需要使用 `--features cli` 或包含 `cli` 的 feature（如 `serve`、`full`）编译。

### 3.1 总览

```
$ ogsql --help

   ██████╗  ██████╗ ███████╗ ██████╗ ██╗      ██████╗  █████╗ ██████╗ ███████╗███████╗██████╗
  ██╔═══██╗██╔════╝ ██╔════╝██╔═══██╗██║      ██╔══██╗██╔══██╗██╔══██╗██╔════╝██╔════╝██╔══██╗
  ██║   ██║██║  ███╗███████╗██║   ██║██║      ██████╔╝███████║██████╔╝███████╗███████╗██████╔╝
  ██║   ██║██║   ██║╚════██║██║   ██║██║      ██╔═══╝ ██╔══██║██╔══██╗╚════██║██╔═══╝ ██╔══██╗
  ╚██████╔╝╚██████╔╝███████║╚██████╔╝███████╗ ██║     ██║  ██║██║  ██║███████║███████╗██║  ██║
   ╚═════╝  ╚═════╝ ╚══════╝ ╚═════╝ ╚══════╝ ╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚══════╝╚═╝  ╚═╝

ogsql 0.6.18
openGauss/GaussDB SQL Parser

Usage: ogsql [OPTIONS] <COMMAND>

Commands:
  format      格式化 SQL 语句
  parse       解析 SQL 为 AST（抽象语法树）
  json2sql    将 JSON 还原为 SQL
  tokenize    将 SQL 分词为 token 列表
  validate    校验 SQL 语法
  parse-xml   解析 iBatis/MyBatis XML Mapper [需要 ibatis feature]
  parse-java  从 Java 源文件中提取 SQL [需要 java feature]
  serve       启动 HTTP API 服务器 [需要 serve feature]
  playground  启动交互式终端 [需要 tui feature]

Options:
  -f, --file <FILE>     从文件读取 SQL（可多次指定）
  -j, --json            以 JSON 格式输出
  -v, --verbose         详细输出模式
      --comments        输出中包含注释信息
      --mybatis         启用 MyBatis #{param} 和 ${expr} 占位符支持
      --schema-json     指定 schema JSON 文件（用于语义分析）
  -h, --help            显示帮助信息
  -V, --version         显示版本号
```

**全局选项说明：**

| 选项 | 说明 |
|------|------|
| `-f, --file <FILE>` | 从文件读取 SQL 而非标准输入，可多次指定处理多个文件 |
| `-j, --json` | 输出 JSON 格式（适用于 parse、tokenize 等命令） |
| `-v, --verbose` | 启用详细输出，显示更多解析过程信息 |
| `--comments` | 在输出中包含注释信息 |
| `--mybatis` | 启用 MyBatis 占位符支持，格式化时保留 `#{param}` 和 `${expr}` |
| `--schema-json <FILE>` | 指定数据库 schema JSON 文件，用于启用语义分析功能 |

---

### 3.2 parse — 解析 SQL

将 SQL 文本解析为 AST（抽象语法树），输出结构化的语法树信息。

```bash
# 基本用法：从标准输入解析
echo "SELECT id, name FROM users WHERE status = 'active'" | ogsql parse

# 以 JSON 格式输出（推荐用于程序化处理）
echo "SELECT * FROM users" | ogsql parse -j

# 从文件解析
ogsql -f query.sql parse -j

# 解析包含 MyBatis 占位符的 SQL
echo "SELECT * FROM users WHERE id = #{userId}" | ogsql --mybatis parse -j

# 解析 PL/pgSQL
echo "DO \$\$ BEGIN RAISE NOTICE 'hello'; END \$\$" | ogsql parse -j

# 解析多语句文件，输出带详细信息
ogsql -f procedures.sql parse -j -v --comments
```

**JSON 输出结构：**

```json
{
  "statements": [
    {
      "Select": {
        "targets": [...],
        "from": [...],
        "where_clause": {...}
      },
      "sql_text": "SELECT id, name FROM users WHERE status = 'active'",
      "start_line": 1,
      "start_col": 1,
      "end_line": 1,
      "end_col": 52
    }
  ],
  "errors": []
}
```

每个语句包含：
- 语句变体（如 `Select`、`Insert` 等）及其结构化字段
- `sql_text`：原始 SQL 文本
- `start_line`/`start_col`/`end_line`/`end_col`：源码位置信息

> JSON 输出结构的完整参考，请参阅 [AST JSON Reference](./ast-json-reference.md)。

---

### 3.3 tokenize — 分词

将 SQL 文本拆分为 Token 序列，显示每个 Token 的类型、值和位置。

```bash
# 基本分词
echo "SELECT * FROM users WHERE id = 1" | ogsql tokenize

# JSON 格式输出
echo "SELECT * FROM users" | ogsql tokenize -j
```

**输出示例（JSON）：**

```json
[
  {
    "token": "SELECT",
    "kind": "Keyword",
    "span": { "start": 0, "end": 6, "line": 1, "col": 1 }
  },
  {
    "token": "*",
    "kind": "Star",
    "span": { "start": 7, "end": 8, "line": 1, "col": 8 }
  }
]
```

---

### 3.4 format — 格式化

将 SQL 文本格式化为标准化的、可读性强的格式。支持 DML、DDL 和 PL/pgSQL。

```bash
# 默认格式化（2 空格缩进，保留原始大小写）
echo "select id,name from users where id=1" | ogsql format

# 关键字大写 + 4 空格缩进
echo "select * from users" | ogsql format -u -i 4

# 使用 leading 逗号风格
echo "SELECT id, name, age FROM users" | ogsql format --comma leading

# 设置行宽限制
echo "SELECT * FROM users" | ogsql format -w 80

# 保留 MyBatis 占位符
echo "SELECT * FROM users WHERE id = #{userId}" | ogsql format --mybatis

# 紧凑模式（不换行）
echo "SELECT id, name FROM users" | ogsql format --no-select-newline --no-logical-newline

# 格式化 DML 语句
echo "update users set name='Bob',age=30 where id=1" | ogsql format
echo "delete from users where id = 1 and status = 'inactive'" | ogsql format
echo "merge into target t using source s on t.id=s.id when matched then update set t.name=s.name" | ogsql format

# 格式化 CTE（WITH 语句）
echo "with cte as (select id from users) select * from cte" | ogsql format

# 从文件读取并格式化
ogsql -f query.sql format -u -i 4
```

**格式化选项详解：**

| 选项 | 默认值 | 说明 |
|------|--------|------|
| `-i, --indent <N>` | `2` | 缩进空格数 |
| `-k, --keyword-case <CASE>` | `preserve` | 关键字大小写：`preserve`（保留原样）、`upper`（大写）、`lower`（小写） |
| `--comma <STYLE>` | `trailing` | 逗号风格：`trailing`（行尾逗号）、`leading`（行首逗号） |
| `-w, --line-width <N>` | `120` | 最大行宽，`0` 表示不限制 |
| `-u, --uppercase` | — | `--keyword-case upper` 的快捷方式 |
| `--no-select-newline` | — | 不将每个 SELECT 列放在单独的行 |
| `--no-logical-newline` | — | 不将 AND/OR 放在新行 |
| `--no-semicolon-newline` | — | 不将分号放在单独的行 |
| `--mybatis` | — | 保留 MyBatis `#{param}` 和 `${expr}` 占位符 |

---

### 3.5 validate — 语法校验

检查 SQL 语法是否正确，报告错误和警告信息。支持单文件、目录批量扫描、严格模式和多种输出格式。

```bash
# 校验单条 SQL
echo "SELECT * FROM users" | ogsql validate
# 输出: OK

# 校验错误 SQL
echo "SELECT FROM" | ogsql validate
# 输出: 错误信息，包含位置和原因

# 从文件校验
ogsql -f procedures.sql validate

# 批量扫描目录
ogsql validate -d ./sql-files

# 启用严格模式（检测 PL 块中未定义函数调用）
echo "CREATE OR REPLACE FUNCTION test() RETURNS VOID AS \$\$ BEGIN PERFORM unknown_func(); END; \$\$ LANGUAGE plpgsql" | ogsql validate --strict

# 批量扫描 + CSV 输出 + 统计
ogsql validate -d ./sql-files --csv --stats

# 批量扫描 + JSON 输出 + 统计
ogsql validate -d ./sql-files -j --stats

# 校验并生成错误日志（需 -v）
ogsql -f procedures.sql validate -v

# 配合 --lint 启用 SQL 反模式检测
ogsql validate -d ./sql-files --lint --stats
```

**校验选项：**

| 选项 | 说明 |
|------|------|
| `-d, --dir <DIR>` | 递归扫描目录中的 SQL 文件（可多次指定） |
| `-e, --ext <EXT>` | 扫描的文件扩展名，逗号分隔（默认: `sql`） |
| `--csv` | 以 CSV 格式输出校验结果（每行一条语句） |
| `--stats` | 输出统计摘要（文件数、错误数、语句类型分布等） |
| `--strict` | 启用严格模式：检测 PL 块中未定义的函数调用（详见 [13.1](#131-pl-变量校验严格模式)） |

**校验流程：**

`validate` 命令依次执行以下检查：

1. **分词** — 检查字符串、注释等是否正确闭合
2. **语法解析** — 检查 SQL 语法是否正确，报告解析错误和警告
3. **MERGE 语义校验** — 检查 MERGE 语句的 GaussDB 兼容性（详见 [13.2](#132-merge-语义校验)）
4. **包一致性校验** — 检查 PACKAGE 与 PACKAGE BODY 是否匹配（详见 [13.3](#133-包一致性校验)）
5. **PL 变量校验** — 检查 PL 块中是否使用了未声明的变量（`--strict` 时还检查未定义函数，详见 [13.1](#131-pl-变量校验严格模式)）

**退出码：**

- `0` — 所有文件校验通过（可能有警告）
- `1` — 存在错误（`--csv` 或批量模式时）

---

### 3.6 json2sql — JSON 还原 SQL

将 `parse -j` 生成的 JSON 转换回 SQL 文本。支持从标准输入或文件读取 JSON。

```bash
# 管道往返：SQL → JSON → SQL
echo "SELECT id FROM users WHERE id = 1" | ogsql parse -j | ogsql json2sql

# 从 JSON 文件还原
ogsql -f ast.json json2sql
```

**注意：** 还原后的 SQL 在语义上与原始 SQL 等价，但格式可能不同（注释和原始大小写不会保留）。

---

### 3.7 parse-xml — 解析 iBatis/MyBatis XML

从 iBatis/MyBatis XML Mapper 文件中提取 SQL 语句及其参数信息。需要 `--features ibatis`。

```bash
# 解析单个 XML（从标准输入）
echo '<mapper namespace="test"><select id="find">SELECT * FROM t WHERE id = #{id}</select></mapper>' | ogsql parse-xml

# 解析单个文件，CSV 输出
ogsql parse-xml --csv -f mapper/UserMapper.xml

# 解析单个文件，JSON 输出
ogsql parse-xml -f mapper/UserMapper.xml -j

# 解析目录下所有 XML
ogsql parse-xml --dir ./mapper-xml -j

# 结合 Java 源码进行参数类型推断
ogsql parse-xml -d /path/to/mapper-xml --java-src /path/to/src/main/java --csv

# 指定多个 Java 源码目录
ogsql parse-xml --dir ./mapper-xml --java-src ./src/main/java --java-src ./lib/src -j
```

**输出说明：**

每个提取的 SQL 语句包含：
- `id`：Mapper 语句 ID（对应 XML 中的 `id` 属性）
- `flat_sql`：展开动态标签后的平坦 SQL
- `parameters`：参数列表，包含名称和推断的 JDBC 类型

---

### 3.8 parse-java — 提取 Java 中的 SQL

从 Java 源文件中提取嵌入的 SQL 语句。需要 `--features java`。

```bash
# 从单个 Java 文件提取
ogsql parse-java -f src/main/java/com/example/UserDao.java

# 从目录批量提取
ogsql parse-java --dir src/main/java -j
```

---

### 3.9 serve — HTTP API 服务器

启动 HTTP API 服务器，通过网络接口访问解析功能。需要 `--features serve`。

```bash
# 启动服务器（默认 0.0.0.0:3000）
ogsql serve

# 自定义主机和端口
ogsql serve --host 127.0.0.1 --port 8080
```

**可用 API 端点：**

| 方法 | 端点 | 说明 | 请求体 |
|------|------|------|--------|
| GET | `/api/health` | 健康检查（含版本号） | — |
| POST | `/api/parse` | 解析 SQL → AST JSON | `{"sql": "...", "lint": true, "lint_config": {...}}` |
| POST | `/api/json2sql` | JSON → SQL | `{"json": "..."}` |
| POST | `/api/format` | 格式化 SQL | `{"sql": "...", "indent": 2, "keyword_case": "upper", ...}` |
| POST | `/api/tokenize` | SQL 分词 | `{"sql": "..."}` |
| POST | `/api/validate` | 校验 SQL（语法+语义+lint） | `{"sql": "...", "strict": true, "lint": true}` |
| POST | `/api/parse-xml` | 解析 iBatis/MyBatis XML | `{"xml": "..."}` |
| POST | `/api/validate-xml` | 校验 iBatis/MyBatis XML | `{"xml": "...", "strict": true, "lint": true}` |
| POST | `/api/parse-java` | 提取 Java 中的 SQL | `{"source": "..."}` |
| POST | `/api/validate-java` | 校验 Java 源文件中的 SQL | `{"source": "...", "strict": true, "lint": true}` |
| GET | `/api-docs/openapi.json` | OpenAPI 规范 | — |
| GET | `/api-docs/swagger-ui` | Swagger UI 交互文档 | — |

> 注：`parse-xml` / `validate-xml` 需要 `--features ibatis`。`parse-java` / `validate-java` 需要 `--features java`。Swagger UI 资源已嵌入二进制，**无需外网访问**。

**请求示例：**

```bash
# 解析 SQL
curl -X POST http://localhost:3000/api/parse \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT id, name FROM users WHERE id = 1"}'

# 格式化 SQL
curl -X POST http://localhost:3000/api/format \
  -H "Content-Type: application/json" \
  -d '{"sql": "select id,name from users", "indent": 4, "keyword_case": "upper"}'

# 校验 SQL
curl -X POST http://localhost:3000/api/validate \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT FROM"}'
```

**format 请求体参数：**

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `sql` | string | — | 必填，要格式化的 SQL |
| `indent` | number | `2` | 缩进空格数 |
| `keyword_case` | string | `"preserve"` | 关键字大小写：`preserve`、`upper`、`lower` |
| `comma_style` | string | `"trailing"` | 逗号风格：`trailing`、`leading` |
| `line_width` | number | `120` | 最大行宽 |
| `uppercase` | boolean | `false` | 等同于 `keyword_case: "upper"` |
| `mybatis` | boolean | `false` | 是否保留 MyBatis 占位符 |
| `no_select_newline` | boolean | `false` | 不将每个 SELECT 列放在单独的行 |
| `no_logical_newline` | boolean | `false` | 不将 AND/OR 放在新行 |
| `no_semicolon_newline` | boolean | `false` | 不将分号放在单独的行 |

**Lint 配置（`lint_config`）参数：**

当设置 `"lint": true` 时，可传入 `lint_config` 自定义规则行为：

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `min_level` | string | `"suggestion"` | 最低报告级别：`prohibition`、`performance`、`caution`、`suggestion` |
| `min_confidence` | string | `"partial"` | 最低可信度：`full`、`partial` |
| `suppress` | string[] | `[]` | 禁用的规则 ID 列表（如 `["P001", "C018"]`） |
| `in_list_threshold` | number | `500` | P003 IN 列表大小阈值 |
| `subquery_depth_limit` | number | `3` | P014 子查询嵌套深度限制 |
| `non_equi_join_limit` | number | `2` | P007 非等值连接数限制 |

**错误响应格式：**

所有端点错误均返回正确的 HTTP 状态码：

| HTTP 状态码 | 说明 | 响应示例 |
|------------|------|---------|
| 400 | 无效 JSON 或缺少必填字段 | `{"error":"bad_request","message":"Invalid JSON: ..."}` |
| 422 | SQL 分词/解析失败 | `{"error":"unprocessable_entity","message":"Tokenization error: ..."}` |
| 404 | 资源未找到（如 procedure 不存在） | `{"error":"not_found","message":"Procedure 'x' not found"}` |
| 500 | 服务器内部错误 | `{"error":"internal_error","message":"serialization failed"}` |

---

### 3.10 playground — 交互式终端

启动交互式终端 UI（TUI），实时输入 SQL 并查看解析结果。需要 `--features tui`。

```bash
ogsql playground
```

在 playground 中可以：
- 实时输入 SQL 语句
- 查看解析后的 AST 结构
- 实时查看格式化结果
- 查看分词结果

---

## 4. MCP 服务器

OGSQL Parser 可作为 MCP (Model Context Protocol) 服务器运行，为 AI 编辑器和助手提供 SQL 解析工具。需要 `--features mcp`。

### 4.1 构建与启动

```bash
# 编译 MCP 服务器
cargo build --release --features mcp

# 运行（使用 stdio 传输，适用于 Claude Desktop、Cursor 等）
./target/release/ogsql-mcp
```

### 4.2 可用工具

MCP 服务器提供以下工具供 AI 助手调用：

| 工具名 | 说明 | 参数 |
|--------|------|------|
| `parse` | 解析 SQL → AST JSON（含指纹、注释、错误） | `sql` |
| `tokenize` | SQL → Token 列表（含类型、值、位置） | `sql` |
| `format` | 格式化 SQL | `sql`, 可选: `indent`, `keyword_case`, `comma`, `line_width` |
| `validate` | 校验 SQL 语法，报告错误/警告 | `sql` |
| `json2sql` | 将 AST JSON 转换回 SQL | `json` |
| `parse_xml` | 解析 iBatis/MyBatis XML Mapper | `xml` 或 `dir`, 可选: `java_src`/`java_sources` |
| `parse_java` | 从 Java 源文件提取 SQL | `path` 或 `dir` |

### 4.3 配置 Claude Desktop

在 Claude Desktop 的配置文件 `claude_desktop_config.json` 中添加：

```json
{
  "mcpServers": {
    "ogsql": {
      "command": "/path/to/ogsql-mcp"
    }
  }
}
```

配置完成后，Claude 即可使用 SQL 解析工具来分析和处理 SQL 语句。

---

## 7. 支持的 SQL 语法

### 7.1 DML 语句

| 语句 | 说明 |
|------|------|
| `SELECT` | 完整语法，包括 CTE（WITH）、子查询、窗口函数、PIVOT/UNPIVOT |
| `INSERT` | INSERT VALUES、INSERT SELECT、ON CONFLICT |
| `UPDATE` | UPDATE SET、FROM、WHERE、RETURNING |
| `DELETE` | DELETE WHERE、RETURNING |
| `MERGE` | MERGE INTO、WHEN MATCHED / NOT MATCHED |
| `WITH (CTE)` | 通用表表达式，递归 CTE |
| `VALUES` | VALUES 列表 |

### 7.2 DDL 语句

| 语句 | 状态 |
|------|------|
| `CREATE TABLE` | ✅ 已完成（含分区、分布、约束、表选项、ILM策略、压缩） |
| `CREATE TABLE AS` | ✅ 已完成 |
| `CREATE INDEX` | ✅ 已完成 |
| `CREATE GLOBAL INDEX` | ✅ 已完成 |
| `CREATE VIEW` | ✅ 已完成 |
| `CREATE MATERIALIZED VIEW` | ✅ 已完成（含 REFRESH MATERIALIZED VIEW） |
| `CREATE FUNCTION` | ✅ 已完成（含 PL/pgSQL 体、参数、返回类型） |
| `CREATE PROCEDURE` | ✅ 已完成（含 PL/pgSQL 体） |
| `CREATE PACKAGE / PACKAGE BODY` | ✅ 已完成（Oracle 兼容包） |
| `CREATE TRIGGER` | ✅ 已完成 |
| `CREATE SCHEMA` | ✅ 已完成 |
| `CREATE DATABASE` | ✅ 已完成 |
| `CREATE DATABASE LINK` | ✅ 已完成 |
| `CREATE TABLESPACE` | ✅ 已完成 |
| `CREATE SEQUENCE` | ✅ 已完成 |
| `CREATE TYPE` | ✅ 已完成（复合、枚举、范围类型） |
| `CREATE DOMAIN` | ✅ 已完成 |
| `CREATE CAST` | ✅ 已完成 |
| `CREATE EXTENSION` | ✅ 已完成 |
| `CREATE ROLE / USER / GROUP` | ✅ 已完成 |
| `CREATE FOREIGN TABLE / SERVER / FDW` | ✅ 已完成 |
| `CREATE PUBLICATION / SUBSCRIPTION` | ✅ 已完成 |
| `CREATE SYNONYM` | ✅ 已完成（Oracle 兼容） |
| `CREATE AGGREGATE / OPERATOR` | ✅ 已完成 |
| `CREATE MODEL` | ✅ 已完成（AI 模型管理） |
| GaussDB 专有 CREATE | ✅ 已完成（NODE、NODE GROUP、RESOURCE POOL、WORKLOAD GROUP、AUDIT/MASKING/RLS POLICY、DATA SOURCE、EVENT、OP CLASS/FAMILY、STREAM、KEY、DIRECTORY、LANGUAGE、WEAK PASSWORD DICTIONARY、TEXT SEARCH CONFIG/DICT、CONT QUERY、APP WORKLOAD GROUP MAPPING） |
| `ALTER TABLE` | ✅ 已完成 |
| `ALTER INDEX / SEQUENCE / VIEW` | ✅ 已完成 |
| `ALTER FUNCTION / PROCEDURE` | ✅ 已完成 |
| `ALTER SCHEMA / DATABASE / TABLESPACE` | ✅ 已完成 |
| `ALTER ROLE / USER / GROUP` | ✅ 已完成 |
| `ALTER TRIGGER / EXTENSION` | ✅ 已完成 |
| `ALTER FOREIGN TABLE / SERVER / FDW` | ✅ 已完成 |
| `ALTER PUBLICATION / SUBSCRIPTION` | ✅ 已完成 |
| `ALTER TYPE / DOMAIN` | ✅ 已完成 |
| GaussDB 专有 ALTER | ✅ 已完成（NODE、NODE GROUP、RESOURCE POOL、WORKLOAD GROUP、AUDIT/MASKING/RLS POLICY、DATA SOURCE、EVENT、OP FAMILY、OPERATOR、MATERIALIZED VIEW、GLOBAL CONFIG、SESSION、DATABASE LINK、DIRECTORY、LANGUAGE、PACKAGE、COORDINATOR、APP WORKLOAD GROUP MAPPING、SYNONYM、TEXT SEARCH CONFIG/DICT） |
| `DROP` 语句 | ✅ 已完成（支持 30+ 种对象类型） |
| `TRUNCATE` | ✅ 已完成 |
| `COMMENT` | ✅ 已完成 |
| `GRANT / REVOKE` | ✅ 已完成（含角色授权） |

### 7.3 PL/pgSQL 语句

| 语句 | 说明 |
|------|------|
| `DO $$ ... $$` | DO 语句 |
| 匿名块 | `DECLARE ... BEGIN ... END` |
| 变量声明 | 变量、常量、游标、RECORD、TYPE、%TYPE、%ROWTYPE |
| 赋值 | `variable := expression` |
| `IF / ELSIF / ELSE` | 条件分支 |
| `CASE` | 搜索型和简单 CASE 表达式 |
| `LOOP / WHILE / FOR / FOREACH` | 所有循环结构（支持标签） |
| `EXIT / CONTINUE` | 带可选 WHEN 条件和标签 |
| `RETURN` | 带可选表达式 |
| `RAISE` | 所有级别：DEBUG、LOG、INFO、NOTICE、WARNING、EXCEPTION |
| `EXECUTE / PERFORM` | 动态 SQL 执行 |
| 游标操作 | OPEN、FETCH、CLOSE、MOVE |
| `GET DIAGNOSTICS` | 包括 STACKED DIAGNOSTICS |
| 异常处理 | `WHEN ... THEN` 处理器 |
| `GOTO` | GOTO 标签 |
| 事务控制 | 块内 COMMIT、ROLLBACK、SAVEPOINT |
| `FORALL / PIPE ROW` | 批量操作 |

### 7.4 其他语句

- `EXPLAIN` / `EXPLAIN ANALYZE`
- `GRANT` / `REVOKE` / `GRANT ROLE` / `REVOKE ROLE`
- `BEGIN` / `COMMIT` / `ROLLBACK` / `SAVEPOINT`
- `COPY`
- `VACUUM` / `REINDEX` / `CLUSTER` / `ANALYZE`
- `SET` / `SHOW` / `RESET`
- `CALL`
- `PREPARE` / `EXECUTE` / `DEALLOCATE`
- `COMMENT`
- `LOCK`
- `DECLARE CURSOR` / `FETCH` / `CLOSE`
- `CHECKPOINT` / `DISCARD`
- `LISTEN` / `NOTIFY` / `UNLISTEN`
- `SHUTDOWN` / `BARRIER`
- `PURGE` / `TIMECAPSULE` / `SNAPSHOT`
- `SHRINK` / `VERIFY` / `CLEAN CONNECTION`
- `COMPILE`
- `REPLACE`（Oracle 兼容 INSERT OR REPLACE）
- `INSERT ALL / INSERT FIRST`（Oracle 多表插入）
- `PREDICT BY`（AI 预测）
- `EXECUTE DIRECT`（分布式直接执行）
- `SECURITY LABEL`
- `EXPDP DATABASE/TABLE` / `IMPDP DATABASE/TABLE`（数据导入导出）
- 以及更多 openGauss/GaussDB 特有语句

---

## 8. 多字符集支持

OGSQL Parser 支持多种字符编码的 SQL 文本，自动检测和转换：

| 编码 | 说明 |
|------|------|
| UTF-8 | 默认编码 |
| EUC-JP | 日语 |
| EUC-KR | 韩语 |
| GB18030 | 中文（包含 GB2312 和 GBK） |
| BIG5 | 繁体中文 |
| UTF-16 | Unicode 双字节 |

CLI 工具自动检测输入文件编码，无需额外配置。

---

## 10. 常见问题 (FAQ)

### Q: OGSQL Parser 支持标准 PostgreSQL SQL 吗？

是的。由于 openGauss/GaussDB 基于 PostgreSQL，OGSQL Parser 支持绝大多数 PostgreSQL SQL 语法，同时包含 openGauss/GaussDB 特有的扩展。

### Q: 解析遇到错误时会怎样？

解析器会尽可能继续解析后续语句，将遇到的错误收集到 `errors` 数组中返回，而不是直接中止。这意味着即使部分语句有语法错误，仍然可以获取其他正确解析的语句。

错误（如语法不匹配、分词错误）表示语法问题；警告（如保留关键字用作标识符）表示潜在问题但不阻止解析。

### Q: validate 命令和 parse 命令有什么不同？

`parse` 专注于输出 AST 结构，适合程序化处理和 JSON 往返转换。`validate` 专为语法校验设计，额外执行以下检查：

1. **MERGE 语义校验** — 检测 GaussDB 不支持的 MERGE 模式
2. **包一致性校验** — 检查 PACKAGE 与 PACKAGE BODY 是否匹配
3. **PL 变量校验** — 检测 PL 块中未声明的变量
4. **严格模式** — `--strict` 下额外检测未定义函数调用

`validate` 更适合 CI/CD 流水线中作为质量门禁使用。

### Q: 如何在 CI/CD 中使用校验功能？

```bash
# 批量校验 SQL 文件目录，CSV 输出 + 统计
ogsql validate -d ./sql-files --csv --stats

# 启用严格校验 + Lint（自动成为质量门禁，有错误时退出码 1）
ogsql validate -d ./sql-files --strict --lint --min-level caution --csv --stats

# 使用自定义 lint 配置
ogsql validate -d ./sql-files --lint --lint-config ./ci/.ogsql-lint.toml --csv --stats
```

有错误时退出码为 `1`，CI 可据此判断是否阻断流水线。警告不会导致非零退出码。

### Q: 什么是严格模式 (--strict)？什么时候应该使用？

`--strict` 模式在正常 PL 变量校验之外，额外检测 PL 块中调用的函数是否在已知函数列表中。已知函数包括：

- OGSQL 内置的 449 个函数（完整 function_registry）
- 同一文件中通过 `CREATE FUNCTION` / `CREATE PROCEDURE` / `PACKAGE` 定义的子程序

**使用场景：**
- **推荐使用** — 当 SQL 文件是完整的功能单元（包含所有依赖定义），严格模式可以捕获拼写错误和缺失依赖
- **谨慎使用** — 当文件引用了外部模块的函数时，严格模式可能产生误报（需要配合 `--schema-json` 或预先扫描依赖文件）

### Q: Lint 规则太多，如何只关注重要的？

使用 `--min-level` 和 `--suppress` 精细化控制：

```bash
# 只报告 Prohibition（禁止项）和 Performance（性能）级别
ogsql validate -d ./sql-files --lint --min-level performance

# 排除特定规则
ogsql validate -d ./sql-files --lint --suppress R001,P012,S006
```

也可以通过配置文件永久设置，见 [11.3 配置文件](#113-配置文件)。

### Q: JSON 往返转换是真正无损的吗？

是语义无损，而非文本无损。往返转换保留所有 SQL 语义信息，但不保留注释、原始关键字大小写、原始格式和空白。还原后的 SQL 在语义上与原始 SQL 完全等价。

> 如需程序化处理 AST JSON，请参阅 [Crate 开发者指南](./crate-guide.md) 和 [AST JSON 参考](./ast-json-reference.md)。

### Q: 如何处理 MyBatis 的动态 SQL 标签（如 `<if>`、`<foreach>`）？

`parse-xml` 命令会将 MyBatis 动态标签展开为平坦 SQL。对于条件性标签（如 `<if>`），所有分支都会被展开；对于循环标签（如 `<foreach>`），会生成代表性输出。

### Q: 支持哪些 openGauss 版本？

参考的是 openGauss 的 gram.y（35,325 行）和 PL/pgSQL gram.y（15,770 行），涵盖当前 openGauss/GaussDB 的主流语法。所有 1,409 个回归测试均通过。

### Q: 许可证是什么？

MIT OR Apache-2.0 双许可，您可以选择其中任一使用。

### Q: 如何贡献代码？

欢迎通过 Issue 和 Pull Request 参与贡献。详见 [贡献指南](../CONTRIBUTING.md)。

---

## 11. SQL 反模式检测 (Lint)

OGSQL Parser 内置 SQL 静态检测引擎 (`SqlLinter`)，可识别 61 条反模式规则，覆盖**禁止项 (Prohibition)**、**性能 (Performance)**、**注意事项 (Caution)** 和**建议 (Suggestion)** 四个等级。

### 11.1 快速开始

通过 `--lint` 全局选项启用，可配合 `parse`、`validate`、`parse-xml`、`parse-java` 使用：

```bash
# 单文件 lint
echo "SELECT * FROM users" | ogsql parse -j --lint

# 目录批量 lint + 统计
ogsql validate -d ./sql-files --lint --stats

# 只报告 Prohibition 和 Performance 级别
ogsql validate -d ./sql-files --lint --min-level performance

# 只报告 Full confidence 的结果（排除 iBatis 动态 SQL 的 Partial 结果）
ogsql validate -d ./sql-files --lint --min-confidence full

# 禁用指定规则（逗号分隔）
ogsql validate -d ./sql-files --lint --suppress R001,S007

# 使用自定义配置文件（详见 11.3）
ogsql validate -d ./sql-files --lint --lint-config ./.ogsql-lint.toml
```

### 11.2 命令行选项

| 选项 | 值 | 默认 | 说明 |
|------|-----|------|------|
| `--lint` | — | 关闭 | 启用 SQL 反模式检测 |
| `--min-level` | `prohibition`, `performance`, `caution`, `suggestion` | `suggestion` | 最低报告级别 |
| `--min-confidence` | `full`, `partial` | `partial` | 最低可信度（full 排除 iBatis 动态 SQL） |
| `--suppress` | 逗号分隔规则 ID | — | 禁用指定规则（如 `R001,S007`） |
| `--in-list-threshold` | 正整数 | `500` | P003: IN 列表大小阈值 |
| `--subquery-depth-limit` | 正整数 | `3` | P014: 子查询嵌套深度阈值 |
| `--non-equi-join-limit` | 正整数 | `2` | P007: 非等值 JOIN 数量阈值 |
| `--group-by-column-limit` | 正整数 | `10` | R002: GROUP BY / ORDER BY 列数阈值 |
| `--max-insert-values-rows` | 正整数 | `65535` | C018: INSERT VALUES 行数×列数阈值 |
| `--foreach-estimated-rows` | 正整数 | `1000` | C018（iBatis 变体）: `<foreach>` 预估迭代次数 |
| `--sql-length-limit` | 正整数 | `4096` | S008: SQL 文本长度阈值 |
| `--lint-config` | 文件路径 | 自动查找 | 配置文件路径（详见 11.3） |

### 11.3 配置文件

OGSQL Lint 支持通过 `.ogsql-lint.toml` 配置文件自定义规则行为。文件搜索顺序：

1. 当前工作目录下的 `.ogsql-lint.toml`
2. `~/.config/ogsql/lint.toml` (XDG 约定)

**配置示例：**

```toml
# .ogsql-lint.toml
min_level = "caution"       # 只报告 Caution 及以上级别
min_confidence = "full"     # 只报告 Full confidence
suppress = ["R001", "S007"] # 禁用 SELECT * 和字面量类型建议
in_list_threshold = 200     # IN 列表超过 200 个值时报告
subquery_depth_limit = 5    # 子查询嵌套超过 5 层时报告
sql_length_limit = 4096     # SQL 超过 4096 字符时建议拆分
non_equi_join_limit = 3     # 非等值 JOIN 超过 3 个时报告
group_by_column_limit = 10  # GROUP BY 超过 10 列时报告
max_insert_values_rows = 65535 # INSERT VALUES 行数×列数阈值
foreach_estimated_rows = 1000   # iBatis <foreach> 预估迭代次数（用于 C018 动态变体）
```

配置项的优先级：**命令行参数 > 配置文件 > 默认值**。

### 11.4 规则清单

> **等级说明**：`Prohibition`(禁止项) > `Performance`(性能) > `Caution`(注意) > `Suggestion`(建议)。
> 共 61 条规则（11 Prohibition + 25 Performance + 17 Caution + 8 Suggestion = 61）。
> 编号不连续（如 C002–C004、S003–S004 不存在）是规划中预留的规则位。

#### 禁止项 (Prohibition) — 违反 GaussDB 编码规范，可能引发数据安全风险（11 条）

| ID | 名称 | 适用语句 | 说明 |
|----|------|----------|------|
| R001 | `select-star` | SELECT | `SELECT *` 违反编码规范。外层 `SELECT *` 仅在包裹内层显式列子查询（作为透传）时允许；单层独立 `SELECT *`、内层/嵌套/CTE 体中的 `SELECT *`、以及 UNION 分支中的 `SELECT *` 均报警。建议明确列出所需字段名 |
| R002 | `large-column-sort` | SELECT | GROUP BY / ORDER BY 包含超过阈值（默认 10）个表达式，可能导致性能问题。建议简化分组/排序列数量 |
| R003 | `lock-table` | All | `LOCK TABLE` 可能导致死锁风险。建议避免在事务中使用 LOCK TABLE，优先使用 `SELECT ... FOR UPDATE` |
| R004 | `drop-cascade` | All | `DROP ... CASCADE` 可能误删依赖对象。建议确认依赖关系后再使用 CASCADE |
| R005 | `implicit-type-conversion` | DML | WHERE 中可能存在隐式类型转换，导致索引失效。需要 schema 信息辅助判断，无 schema 时跳过 |
| R006 | `function-on-where-column` | DML | WHERE 中对列使用函数或表达式运算，将导致索引失效。建议将运算移到等号另一侧或使用函数索引 |
| R007 | `like-leading-wildcard` | DML | `LIKE '%...'` 前导通配符导致无法使用索引，触发全表扫描。建议避免以通配符开头的 LIKE 模式 |
| R010 | `function-side-effect` | All | 自定义 Function 中包含非 SELECT DML（INSERT/UPDATE/DELETE/MERGE/TRUNCATE 等）、事务控制语句（COMMIT/ROLLBACK/SAVEPOINT/SET TRANSACTION），或调用了含事务语句的其他函数/过程。函数应避免修改数据和提交/回滚事务，考虑将副作用操作移至过程中 |
| R013 | `implicit-join` | SELECT | 使用 Oracle 隐式连接语法（逗号分隔 FROM，如 `FROM a, b`），可读性较差。建议改为显式 ANSI JOIN（INNER JOIN / LEFT JOIN） |

#### 性能 (Performance) — 可识别的性能陷阱，有明确的优化路径（25 条）

| ID | 名称 | 适用语句 | 说明 |
|----|------|----------|------|
| R009 | `scalar-subquery-in-select` | SELECT | SELECT 列中包含标量子查询，每行都会执行一次子查询。建议改用 JOIN 替代标量子查询 |
| P001 | `union-without-all` | SELECT | `UNION` 未使用 `ALL`，存在不必要的去重排序。如果确认无重叠，改 `UNION ALL` |
| P002 | `not-in-subquery` | DML | `NOT IN (子查询)` 性能较差，且 NULL 值会导致结果不符合预期。建议改为 `NOT EXISTS` 或 `LEFT JOIN ... IS NULL` |
| P003 | `in-list-too-large` | DML | IN 列表值数量超过阈值（默认 500），导致解析缓慢。建议改用临时表或 INNER JOIN |
| P004 | `or-to-union-all` | DML | WHERE 顶层为 OR 条件，可能导致优化器放弃索引。建议考虑将 OR 改写为 UNION ALL |
| P005 | `now-function-non-pushable` | DML | `now()` / `current_timestamp` / `sysdate` 不可下推，导致分布式查询性能下降。建议用时间宏或参数化查询替代 |
| P006 | `count-star-large-table` | SELECT | `COUNT(*)` 在大表上性能较差。建议考虑使用 `pg_class.reltuples` 或近似统计信息 |
| P007 | `too-many-non-equi-joins` | SELECT | 非等值 JOIN 条件过多（默认超过 2 个），性能较差。建议优先使用等值查询 |
| P008 | `group-by-without-hashagg` | SELECT | GROUP BY 操作未使用 `hash_agg` 提示，可能需要调大 `work_mem`。建议添加 `/*+ hash_agg */` 提示 |
| P009 | `function-instead-of-case` | DML | `NVL`/`NVL2`/`DECODE`/`IIF` 函数可用 CASE 表达式替代，可能更高效 |
| P010 | `multi-column-update-subquery` | UPDATE | 多列 `UPDATE SET (列1,列2) = (SELECT ...)` 效率较低。建议改用 `UPDATE ... FROM ... WHERE` 的 JOIN 风格 |
| P011 | `correlated-subquery` | DML | 关联子查询可能导致每行执行一次子查询。建议改写为等值 JOIN |
| P012 | `unnecessary-distinct` | SELECT | DISTINCT 存在，需结合唯一键判断是否必要。如果已含唯一列则可移除（需 schema 信息确认） |
| P013 | `cartesian-product` | SELECT | CROSS JOIN 或缺少 JOIN 条件，可能产生笛卡尔积。建议补充 JOIN 条件或确认确实需要 CROSS JOIN |
| P014 | `deeply-nested-subquery` | DML | 子查询嵌套深度超过阈值（默认 3 层），性能可能较差。建议拆分为临时表或 CTE |
| P015 | `range-equals-same-value` | DML | `BETWEEN x AND x` 上下界相同，应简化为 `= x` |
| P016 | `update-from-no-join-condition` | UPDATE | `UPDATE FROM` 无 WHERE 子句，可能产生笛卡尔积更新。建议在 WHERE 中关联 FROM 表 |
| P017 | `merge-without-unique-index` | MERGE | MERGE 语句 ON 条件需要唯一索引保证确定性。建议确保 ON 条件列有唯一索引（需 schema 确认） |
| P018 | `insert-select-no-columns` | INSERT | `INSERT INTO ... SELECT` 未指定目标列名，依赖列顺序。建议显式指定目标列名 |
| P019 | `multi-table-update` | UPDATE | 多表 UPDATE，可能产生非预期结果。建议拆分为多条单表 UPDATE |
| P020 | `insert-all-multi-table` | All | `INSERT ALL` / `INSERT FIRST` 多表插入。建议评估是否可用单条 `INSERT ... SELECT` 替代 |
| P021 | `row-by-row-insert-in-loop` | PL Block | 循环体内包含 INSERT，应使用 FORALL 批量操作或 `INSERT ... SELECT` 替代逐行插入 |
| P022 | `explain-in-production` | All | EXPLAIN 语句不应出现在生产代码中。建议仅用于调试环境 |
| P023 | `connect-by-performance` | All | CONNECT BY 层级查询在数据量大或递归层次深时性能可能显著下降；缺少 START WITH 可能导致全表扫描。建议考虑使用 WITH RECURSIVE CTE 替代 |
| P024 | `rownum-pagination` | All | `ROWNUM` 为 Oracle 专有分页伪列，迁移至 openGauss 时应改用 `LIMIT/OFFSET` 或 `FETCH FIRST`（在存储过程体内同样检测） |
| P025 | `in-subquery-to-exists` | DML | 正向 `IN (子查询)` 性能可能不佳，可考虑改用 `EXISTS` 或 `JOIN` 替代（Suggestion 级别，区别于 P002 的 NOT IN） |

#### 注意事项 (Caution) — 语法合法但容易忽略，需要上下文判断（17 条）

| ID | 名称 | 适用语句 | 说明 |
|----|------|----------|------|
| R008 | `same-table-column-compare` | DML | 同表列比较（如 `t.a = t.b`）：可能未正确使用索引。建议检查是否应使用不同表的列进行比较 |
| C001 | `hint-unknown` | DML | Hint 不在 GaussDB 已知 Hint 列表中，将被静默忽略。建议检查 Hint 拼写是否正确 |
| C005 | `hint-contradictory` | DML | Hint 矛盾（如 `tablescan` 与 `indexscan` 同时存在，或 `nestloop` 与 `hashjoin` 互斥）。建议只保留一个扫描方式/连接方式的 Hint |
| C006 | `hint-table-not-in-from` | DML | Hint 引用的表不在 FROM 子句中。建议检查 Hint 中的表名是否与 FROM 子句一致 |
| C007 | `update-without-where` | UPDATE | `UPDATE` 无 WHERE 子句，可能影响全表数据。建议确认是否真的需要更新全表 |
| C008 | `delete-without-where` | DELETE | `DELETE` 无 WHERE 子句，将删除全表数据。建议确认是否应使用 TRUNCATE 或添加 WHERE 条件 |
| C009 | `insert-no-column-list` | INSERT | INSERT 未指定目标列名，依赖表定义顺序。建议显式指定目标列名以避免表结构变化导致数据错误 |
| C010 | `unlogged-table` | DDL | `UNLOGGED TABLE` 在故障恢复时数据会丢失。建议评估是否可以使用普通表替代 |
| C011 | `goto-statement` | PL Block | PL/pgSQL 中使用了 `GOTO` 语句，不符合结构化编程建议。建议使用 IF/EXIT/CONTINUE 替代 |
| C012 | `execute-concat-sql-injection` | PL Block | `EXECUTE IMMEDIATE` 中使用字符串拼接（`\|\|`），可能存在 SQL 注入风险。建议使用 `USING` 参数化查询替代 |
| C013 | `exception-swallow` | PL Block | `WHEN OTHERS THEN` 异常处理中未重新抛出异常，可能静默吞错。建议在处理中添加 RAISE 重新抛出 |
| C014 | `pl-commit-rollback` | PL Block | PL/pgSQL 块中包含 COMMIT/ROLLBACK，可能复杂化事务控制。建议将事务控制交给外层 |
| C015 | `select-for-update-blocking` | SELECT | `SELECT ... FOR UPDATE` 未使用 NOWAIT/SKIP LOCKED，可能长时间阻塞。建议考虑使用 NOWAIT 或 SKIP LOCKED |
| C016 | `autonomous-transaction` | PL Block | `PRAGMA AUTONOMOUS_TRANSACTION` 会创建独立事务，性能开销较大。建议评估是否真的需要独立事务 |
| C017 | `raise-in-exception-clears-variables` | PL Block | RAISE 在 EXCEPTION 块中会清空所有局部变量值（包括 OUT 参数），导致调用方无法获取错误信息。建议使用 RAISE INFO 传递具体错误信息，或在 RAISE 前将输出值保存到临时表/全局变量 |
| C018 | `excessive-insert-values` | INSERT | `INSERT VALUES` 的 `行数 × 列数` 超过阈值（默认 65535）。建议拆分为更小批次插入以减少锁持有时间，或使用 COPY。iBatis/MyBatis 变体还会检测 `<foreach>` 动态批量插入中的参数膨胀风险 |
| C019 | `commit-inside-loop` | PL Block | PL/pgSQL 循环体内包含 COMMIT/ROLLBACK，导致事务不原子且频繁提交影响性能（在存储过程体内同样检测）。建议将事务控制移出循环或使用批量操作 |

#### 建议 (Suggestion) — 改善可维护性和健壮性，不影响正确性（8 条）

| ID | 名称 | 适用语句 | 说明 |
|----|------|----------|------|
| S001 | `delete-full-table-use-truncate` | DELETE | DELETE 无 WHERE 删除全表可用 TRUNCATE 替代，释放空间更快且不产生大量 WAL 日志 |
| S002 | `limit-offset-use-cursor` | SELECT | OFFSET 分页在大偏移时性能较差。建议考虑使用游标翻页替代 LIMIT/OFFSET |
| S005 | `prefer-percent-type` | PL Block | PL/pgSQL 变量使用普通类型而非 `%TYPE`/`%ROWTYPE` 锚定。建议使用 `table.column%TYPE` 或 `table%ROWTYPE` 以提高可维护性 |
| S006 | `limit-without-order-by` | SELECT | `LIMIT` 无 `ORDER BY`，结果顺序不确定。建议添加 ORDER BY 保证结果确定性 |
| S007 | `explicit-type-for-literals` | DML | WHERE 中字符串常量与列比较时类型可能不匹配，可能导致隐式转换。建议使用显式类型转换：`'val'::type` 或 `CAST('val' AS type)`（需要 schema 信息辅助判断） |
| S008 | `complex-sql-consider-split` | All | SQL 文本长度超过阈值（默认 4096 字符，对齐 GaussDB `track_activity_query_size`），建议拆分为多个简单查询或使用 CTE |
| S009 | `tautological-condition` | DML | WHERE 子句中存在恒真条件（如 `1=1`、`'a' = 'a'`），为冗余条件。建议移除 |

> 如需在 Rust 代码中以库 API 方式使用 Lint 功能，请参阅 [Crate 开发者指南 - SQL Lint](./crate-guide.md#5-sql-lint-库-api)。

---

## 13. 语义校验规则

除语法解析外，OGSQL Parser 提供多种语义校验，在 `validate` 命令中自动启用。

### 13.1 PL 变量校验（严格模式）

检测 PL/pgSQL 块中引用了未声明的变量或函数。

**正常模式（默认）：** 只检查变量是否已声明（检测 `DECLARE` 块中定义的变量）。

**严格模式（`--strict`）：** 同时检查函数调用是否在已知函数列表中。已知函数包括：
- OGSQL 内置的 449 个函数（通过 `function_registry` 注册）
- 文件中已 `CREATE FUNCTION` / `CREATE PROCEDURE` 定义的函数
- 文件中 PACKAGE / PACKAGE BODY 中定义的子程序

**示例：**

```bash
# 正常模式：只报告未声明变量
echo "CREATE FUNCTION test() RETURNS VOID AS \$\$
BEGIN
  result := undeclared_var;  -- 报告: 未声明变量
  PERFORM known_func();      -- 不报告（可能是外部函数）
END; \$\$ LANGUAGE plpgsql" | ogsql validate

# 严格模式：同时报告未定义函数
echo "CREATE FUNCTION test() RETURNS VOID AS \$\$
BEGIN
  PERFORM unknown_func();    -- 报告: 函数不在已知列表中
END; \$\$ LANGUAGE plpgsql" | ogsql validate --strict
```

### 13.2 MERGE 语义校验

检测 MERGE 语句的 GaussDB 兼容性问题：

| 种类 | 说明 |
|------|------|
| `DeleteNotSupported` | GaussDB 不支持 `MERGE ... WHEN MATCHED THEN DELETE` |
| `OnColumnUpdated` | GaussDB 不允许在 WHEN MATCHED 中更新 ON 子句引用的列 |

```bash
# 检测 MERGE 语义错误
echo "MERGE INTO t USING s ON t.id = s.id WHEN MATCHED THEN DELETE" | ogsql validate
# → MERGE (GaussDB does not support MERGE ... WHEN MATCHED THEN DELETE)
```

### 13.3 包一致性校验

检查 PACKAGE 与 PACKAGE BODY 定义的一致性：

| 种类 | 说明 |
|------|------|
| `MissingInBody` | PACKAGE 中声明了子程序，但 BODY 中缺少实现 |
| `ExtraInBody` | BODY 中实现了子程序，但 PACKAGE 中未声明 |
| `SignatureMismatch` | 子程序签名（参数类型/数量）不一致 |
| `CursorMismatch` | PACKAGE 中声明的游标在 BODY 中不匹配 |

```bash
# 校验包一致性
ogsql -f package.sql validate
# → package pkg: proc_name — MissingInBody
```

> 如需以 Rust 库 API 方式调用语义校验功能，请参阅 [Crate 开发者指南 - 语义校验规则](./crate-guide.md#6-语义校验规则)。

---

## 相关文档

| 文档 | 面向人群 | 说明 |
|------|----------|------|
| [用户指南](./user-guide.md) | 终端用户 | 本文档 — CLI、HTTP API、MCP 使用指南 |
| [Crate 开发者指南](./crate-guide.md) | Rust 开发者 | Crate API 使用、AST JSON 参考、错误类型参考 |
| [AST JSON 参考](./ast-json-reference.md) | AST 消费者 | `ogsql parse -j` 输出的完整 JSON 结构 |
| [贡献指南](../CONTRIBUTING.md) | 项目贡献者 | 架构概览、从源码构建、开发规范 |
| [GaussDB SQL 功能](./gaussdb-sql-features.md) | 全部 | GaussDB/openGauss SQL 语法支持详情 |
