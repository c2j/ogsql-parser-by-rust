# ogsql-parser Java 连接器设计（DuckDB 式打包 + stdio NDJSON）

> 目标：Java 应用以“一个 Maven 依赖、零配置、开箱即用”的方式使用 ogsql-parser 的高斯 SQL 解析能力，
> 体验对齐 DuckDB JDBC（自动平台识别、jar 内解压原生产物、可外部覆盖路径），
> 通信采用子进程 stdio 长连接 + NDJSON（对齐 MCP stdio / LSP 模式），保留进程隔离。

## 进度

| 里程碑 | 状态 |
|---|---|
| M1 serve-stdio 子命令 + 协议文档 + Rust 测试（18 tests） | ✅ 完成（fmt / clippy --all-features / cargo test --all-features 全绿） |
| M2 Java 骨架：平台命名/解压/spawn/hello/五 op 门面 + JUnit（16 tests） | ✅ 完成（含崩溃自愈、TOO_DEEP、并发串行测试） |
| M3 健壮性打磨（核心项已在 M2 内置：EOF 重启、超时、shutdown hook） | ✅ 完成（stderr 转发 java.util.logging、重启上限语义、2,000 次压力测试 ≈0.37ms/次） |
| M4 CI 平台矩阵 + Maven 打包 + 冒烟 | 🔧 进行中（osx-amd64 改 macos-14；fat-jar 断言 + linux 零外部依赖冒烟；`-Pgithub` 恢复 Packages，`-Pcentral` 隔离 Central；待 GH 实跑打出可消费 fat jar） |
| M5（可选）win7 artifacts、parse_xml/parse_java op、批量 op、基准 | 🔲 按需（Win7 构建路径已在 release.yml 存在，接入 classifier 即可） |


## 1. 总体架构

```
┌─────────────────────────── JVM ───────────────────────────┐
│  Java 应用                                                 │
│    Ogsql facade (parse/format/validate/tokenize/json2sql) │
│    OgsqlProcessManager ── 子进程监督(EOF重启/超时/shutdown) │
│    NativeBinaryLoader ──── DuckDB 式平台命名+解压加载       │
└───────────────┬───────────────────────────────────────────┘
                │ stdin/stdout 管道，NDJSON 每行一个 JSON
┌───────────────▼───────────────────────────────────────────┐
│  ogsql serve-stdio（Rust 静态二进制，随 jar 分发）           │
│    逐行读请求 → 复用 parse_input 等公共逻辑 → 逐行写响应      │
│    每请求 catch_unwind 兜底，panic 不退出循环               │
└───────────────────────────────────────────────────────────┘
```

## 2. Rust 侧：serve-stdio 子命令（属 cli feature，零新依赖）

新增文件：`src/bin/serve_stdio.rs`（或 `src/bin/ogsql.rs` 内 `mod serve_stdio`），挂到 `Commands::ServeStdio`。

### 2.1 协议规范（另落 docs/stdio-protocol.md）

- **帧**：stdin/stdout 每行一个 JSON 对象（UTF-8，LF 结尾）。serde 序列化会转义字符串内换行 → 帧无歧义。单行上限 16 MiB（超限回 PROTOCOL_ERROR，防内存 DoS）。
- **请求**：`{"id": <int>, "op": "...", ...op 参数}`
- **响应**：`{"id": <int>, "ok": true, "result": {...}}` 或 `{"id": <int>, "ok": false, "error": {"code": "...", "message": "..."}}`
- **错误语义（关键约定）**：
  - 语法/语义问题**不是协议错误**：`parse` 返回 ok:true 且 `result.errors` 非空（解析器带错误恢复）；`validate` 返回 ok:true 且 `result.valid:false`。
  - ok:false 仅用于协议层故障：JSON 解析失败 / 未知 op / 参数非法 / 内部 panic（INTERNAL_ERROR）。
- **op 集合**：
  - `hello`：握手，返回 `{version, protocol: 1, ops: [...]}`；Java 侧校验 protocol 与二进制版本。
  - `parse`（sql, preserve_comments, mybatis, procedure?, extract_sql, lint?）
  - `format`（sql, indent?, keyword_case?, comma_style?, line_width?, mybatis?, no_select_newline? 等，复用 CLI 格式化配置）
  - `tokenize`（sql, preserve_comments?, mybatis?）
  - `validate`（sql, mybatis?, strict?, lint?）
  - `json2sql`（json 字符串）
  - `ping` / `shutdown`（健康检查与优雅退出，退出码 0）
- **顺序**：逐行串行处理，响应顺序 = 请求顺序；id 保留用于未来并发/pipeline。
- **panic 兜底**：每个 op 的 dispatch 包 `catch_unwind`；未捕获 panic → ok:false INTERNAL_ERROR + stderr 堆栈，**循环继续**。
- **输入编码**：协议为 UTF-8 JSON；SQL 文本按 UTF-8 传输。非 UTF-8（GB18030/EUC-JP 等）SQL 后续以 base64 扩展 op 参数支持（tokenizer 内部已有编码检测能力，src/token/encoding.rs）。
- **stdin EOF**：正常退出（exit 0）；stderr 仅输出日志。

### 2.2 实现要点

- 复用 `src/bin/ogsql.rs` 的 `parse_input`（:432）、格式化/校验/tokenize/json2sql 的 CLI 实现路径——与 HTTP serve handlers 同一套公共逻辑，避免行为分叉。
- 请求/响应结构体用 serde derive 定义在 serve_stdio 模块内（不依赖 serve feature 的 schema.rs，保证二进制不含 axum）。
- 只依赖核心库 + serde_json（已是 core dep），无需新 Cargo 依赖。
- 测试：`tests/serve_stdio.rs` —— 起进程/直调 dispatch，覆盖：正常五 op、语法错误返回、未知 op、超长行、极端输入触发 panic 兜底后循环可用、shutdown、hello 版本。

## 3. Java 侧：java-connector 模块（本仓库顶层目录，推荐）

```
java-connector/
├── pom.xml                              # artifact: ogsql-parser-java
├── src/main/java/io/github/c2j/ogsql/
│   ├── Ogsql.java                       # 门面：newInstance() + parse/format/validate/tokenize/json2sql + close()
│   ├── OgsqlException.java              # 运行时异常（含协议/进程/超时根因）
│   ├── NativeBinaryLoader.java          # ★ DuckDB 式平台命名 + 解压/外部路径加载
│   ├── OgsqlProcessManager.java         # ★ 子进程监督：spawn/读线程/id 路由/EOF 重启/shutdown hook
│   └── OgsqlRequest.java / ParseResult.java / Validation.java / FormatOptions.java 等模型
├── src/main/resources/
│   ├── ogsql_linux_amd64
│   ├── ogsql_linux_arm64
│   ├── ogsql_osx_amd64
│   ├── ogsql_osx_arm64
│   ├── ogsql_windows_amd64.exe
│   └── （可选 win7 变体以 classifier 单独发布）
└── src/test/java/io/github/c2j/ogsql/   # JUnit 5 测试
```

### 3.1 平台命名与加载（对齐 DuckDB DuckDBNative 三级回退）

1. `-Dogsql.lib.path=/path/to/ogsql` 指定外部二进制 → 直接使用（对应 DuckDB `-nolib` / `java.library.path` 思路，支持热升级只换二进制）。
2. jar 内资源 → 按 `os.name`+属性`os.arch` 映射（x86_64/amd64→amd64, aarch64→arm64）取 `ogsql_<os>_<arch>`，解压到版本化缓存目录（`<tmp>/ogsql-<version>-<hash>/`，多 JVM 复用、`setExecutable(true)`、不随 JVM 退出删除以便复用），失败回退 `Files.createTempFile`+deleteOnExit。
3. 全部失败 → `OgsqlException`，错误信息给出清晰指引（DuckDB 同款体验）。
- macOS：单 jar 内置 amd64+arm64 双资源，按 os.arch 选——等价 DuckDB universal 体验，免 lipo。

### 3.2 进程监督与 NDJSON 客户端（OgsqlProcessManager）

- `ProcessBuilder(extractedBinary, "serve-stdio")`；stdout = NDJSON 响应（专用读线程 `BufferedReader.readLine`）；stderr = 日志线程（默认丢弃/打印，可接 SLF4J）。
- **id 关联 + 流水线**：写侧单锁（请求递增 id，注册 `CompletableFuture`，超时由 `ScheduledExecutor` 调度）；读侧按 id 完成 future → 乱序到达也能正确路由；门面方法同步阻塞。
- **状态机**：NEW → STARTING（hello 握手）→ READY →（EOF/异常）→ RECOVERING（指数退避重启，可配 maxRetries/禁用）→ CLOSED。
- **故障语义**：进程死亡 → 全部 in-flight future 以 `OgsqlException` 完成；EOF 检测是核心健康信号；`ping` 可选做主动心跳。
- **资源回收**：`AutoCloseable` + `Runtime.addShutdownHook`：先发 `shutdown`，超时则 `destroyForcibly`；保证无僵尸进程。
- **并发**：默认流水线（多个调用可并发在途）；可配置串行模式。parser 无全局状态，天然安全。

### 3.3 门面 API（目标：与 DuckDB 一样"一行拿到能力"）

```java
Ogsql ogsql = Ogsql.newInstance();                    // 自动解压 + spawn + hello 握手
ParseResult r = ogsql.parse("SELECT * FROM t WHERE id=#{id}", true); // MyBatis 模式
String ast = r.statementsJson();                      // 与 HTTP/文档同款 AST JSON
String sql = ogsql.format("select 1", FormatOptions.builder().keywordCase(UPPER).build());
Validation v = ogsql.validate("SELECT FROM");          // v.valid()==false, v.errors()
List<Token> toks = ogsql.tokenize("SELECT 1");
String back = ogsql.json2sql(ast);
ogsql.close();
```

- JSON 依赖：Jackson databind（生态事实标准；DuckDB 零依赖但我们不是 JDBC 驱动，单传递依赖可接受）。
- 输出契约：AST JSON 与 HTTP serve / MCP / docs/ast-json-reference.md 完全一致 → Java 模型层可复用。

## 4. 构建与发布

- **Rust CI（GH Actions 矩阵）**：`cargo build --release --features cli`，产出 linux amd64/arm64、osx amd64/arm64、windows amd64 二进制 → 上传 release artifact。
- **win7（可选，后续）**：nightly + `-Zbuild-std` + `x86_64-win7-windows-msvc` / `i686-win7-windows-msvc`（AGENTS.md 现成流程），以 classifier 单独发布。
- **Maven 打包**：Maven 构建从 release artifact / 本地目录把二进制拷入 `src/main/resources`（或 resource 过滤注入版本）；`mvn package` 出胖 jar。
- **发布**：Maven Central（Central Portal：central-publishing-maven-plugin + User Token + GPG 签名；pom 已含 licenses/developers/scm + sources/javadoc/gpg/central 插件）。CI 触发版本对齐（Cargo.toml 版本 = pom 版本）。

## 5. 测试与验收

- Rust：tests/serve_stdio.rs（协议级测试，见 2.2）。
- Java：JUnit 5 —— 五 op 正常路径、语法错误路径（ok:true + errors）、协议错误路径、**kill 子进程后自动重启恢复**、超时、并发（多线程流水线）、close 无残留进程、`-Dogsql.lib.path` 覆盖。
- 冒烟/压力：10k 次 parse 循环，无内存增长、无崩溃；三平台（Linux/macOS/Windows）冒烟。
- **验收标准**：一个 Maven 依赖 + `Ogsql.newInstance()` → parse 可用；延迟 µs–百 µs 级；子进程崩溃 JVM 无感自愈。

## 6. 里程碑与工作量（单人）

| 里程碑 | 内容 | 预估 |
|---|---|---|
| M1 | serve-stdio 子命令 + 协议文档 + Rust 测试 | 1–2 天 |
| M2 | Java 骨架：平台命名/解压/spawn/hello/五 op 同步门面 + JUnit | 2–3 天 |
| M3 | 健壮性：EOF 重启、超时、shutdown hook、并发、故障语义 | 1–2 天 |
| M4 | CI 平台矩阵 + Maven 打包 + docs/java-connector.md + 冒烟 | 1–2 天 |
| M5（可选） | win7 artifacts、parse_xml/parse_java op、批量 op、基准报告 | 按需 |

## 7. 待确认决策点（默认推荐）

1. 模块位置：本仓库 `java-connector/`（推荐，与 Rust 同仓同版本、单 PR 流程） vs 独立 repo。
2. 平台范围：首批 linux amd64/arm64 + osx amd64/arm64 + windows amd64（推荐）；win7 是否首批（涉及 nightly build-std CI）。
3. JSON 依赖：Jackson databind（推荐） vs 零依赖手写 JSON。
4. 并发模型：id 流水线 + 同步门面（推荐） vs 纯串行。
5. 发布渠道：**Maven Central（Central Portal）**（已定并实施，pom/CI 就绪）。

## 8. 已确认决策（评审结论）

| 决策点 | 结论 |
|---|---|
| 模块位置 | 本仓库 `java-connector/` 目录，与 Rust 同仓同版本 |
| 平台范围（首批） | linux amd64/arm64 + osx amd64/arm64 + windows amd64；Win7 以 classifier 扩展点后续追加 |
| JSON 依赖 | Jackson databind |
| 并发模型 | **纯串行**：Java 侧每次调用独占一次往返（单锁），协议仍保留 id 字段（序列号，为未来流水线预留），实现最简单 |
| 发布渠道 | **Maven Central（Central Portal）**，`mvn deploy` 直接发布；pom 与 CI 已就绪，待一次性账号/密钥准备后首发 |

> 协议层设计不受并发模型影响：NDJSON + id 已按流水线友好方式定义；Java 侧仅需一把写锁 + 顺序读响应即可。

## 9. 参考资料（仓库内）

- MCP stdio 先例：src/bin/ogsql-mcp.rs、src/mcp/mod.rs（rmcp::transport::stdio）
- HTTP handlers 公共逻辑：src/bin/ogsql.rs parse_input(:432)、src/bin/serve/
- AST JSON 契约：docs/ast-json-reference.md
- Win7 构建：AGENTS.md；编码检测：src/token/encoding.rs
