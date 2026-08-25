# ogsql-parser Java 开发者指南（java-connector）

> 面向 **Java 应用开发者**的完整集成文档：把 openGauss/GaussDB 的 SQL 解析能力以
“一个 Maven 依赖、零配置、开箱即用”的方式接入 Java 工程，体验对齐 DuckDB JDBC。

> - 通信机制：子进程 `ogsql serve-stdio` + stdio 长连接 + NDJSON（协议细节见 [stdio-protocol.md](./stdio-protocol.md)）
> - 设计文档：[java-connector-design.md](./java-connector-design.md)（含里程碑与决策记录）
> - 简明上手：[java-connector.md](./java-connector.md)

## 目录

- [1. 快速开始](#1-快速开始)
- [2. 架构与工作原理](#2-架构与工作原理)
- [3. Maven 依赖与发布渠道](#3-maven-依赖与发布渠道)
- [4. 平台与二进制（DuckDB 式加载）](#4-平台与二进制duckdb-式加载)
- [5. API 参考](#5-api-参考)
- [6. 错误语义与异常](#6-错误语义与异常)
- [7. 并发、生命周期与资源管理](#7-并发生命周期与资源管理)
- [8. 性能数据](#8-性能数据)
- [9. 故障排查 FAQ](#9-故障排查-faq)
- [10. Spring Boot 集成示例](#10-spring-boot-集成示例)
- [11. 版本兼容与升级](#11-版本兼容与升级)

---

## 1. 快速开始

```xml
<dependency>
  <groupId>io.github.c2j</groupId>
  <artifactId>ogsql-parser-java</artifactId>
  <version>0.10.0</version>
</dependency>
```

```java
import io.github.c2j.ogsql.*;

// 打开即用：自动定位二进制 → 启动子进程 → hello 握手
try (Ogsql ogsql = Ogsql.newInstance()) {

    // 1) 解析：AST JSON（与 HTTP/MCP 输出同构，可无损还原 SQL）
    ParseResult r = ogsql.parse("SELECT * FROM t WHERE id = #{id}", true); // mybatis=true 保留 #{param}
    r.statementCount();   // 语句数
    r.errorCount();       // 语法问题数（注意：不是异常！见 §6）
    r.statements();       // AST JSON（Jackson JsonNode）

    // 2) 格式化：关键字大写 + 2 空格缩进
    String sql = ogsql.format("select a,b from t where x=1",
        FormatOptions.builder().keywordCase("upper").build());

    // 3) 校验：返回 valid 布尔，不抛异常
    Validation v = ogsql.validate("SELECT FROM WHERE");   // v.valid() == false

    // 4) 分词
    List<TokenInfo> tokens = ogsql.tokenize("SELECT 1");

    // 5) AST JSON → SQL 无损往返
    String back = ogsql.json2sql(r.resultJson());

    // 6) 版本与健康
    String ver = ogsql.version();   // Rust crate 版本，如 0.10.0
    boolean alive = ogsql.isAlive();

} // close()：优雅 shutdown → destroy → 兜底 destroyForcibly，无僵尸进程
```

**Java 要求**：JDK 11+；唯一运行时依赖 Jackson databind（Spring 应用通常已自带）。

## 2. 架构与工作原理

```
┌────────────────────────────── JVM ──────────────────────────────┐
│  你的 Java 应用                                                 │
│    Ogsql (门面) → OgsqlProcessManager (子进程监督)               │
│                       │ stdin/stdout 管道                       │
│                       │ NDJSON：每行一个请求/响应                │
└───────────────────────┼───────────────────────────────────────┘
                        ▼
              ogsql serve-stdio（Rust 静态二进制，随 jar 分发）
              解析核心与 CLI/HTTP/MCP 完全同一份代码
```

- **为什么用子进程 + stdio 而不是 JNI / HTTP？**
  - vs JNI：无 JVM 崩溃风险（Rust panic/栈溢出不会带走 JVM），无需每平台原生库打包与 FFI 内存协议
  - vs HTTP：无网络栈开销，进程内管道往返 ~0.4ms，比 HTTP 快一个数量级
  - 隔离性 = HTTP 级，延迟 = 进程内级，工程成本 ≈ CLI 级
- **进程隔离 + 自愈**：子进程崩溃（含无法捕获的栈溢出 abort）→ stdout EOF 检测 → 指数退避重启 → 失败请求自动重试一次。JVM 全程无感。

## 3. Maven 依赖与发布渠道

| 渠道 | 何时用 | 怎么消费 |
|---|---|---|
| **GitHub Packages**（常驻） | SNAPSHOT、内部、fastaas vendor、每个 `v*` tag | `https://maven.pkg.github.com/c2j/ogsql-parser` + `settings.xml` server `github`（`packages:read`） |
| **Maven Central**（额外公开通道） | 公开坐标，手动 `deploy_central`（凭据齐了之后） | 普通 Maven Central，无需额外 `<repository>` |

**推荐**：Central 一旦发布，公开项目用 Central。在此之前以及内部/SNAPSHOT，用 Packages。

`-Dogsql.lib.path` 是可选覆盖（热替换 / 调试），**不是**默认前提。连接器没有 `OGSQL_BIN` 环境变量——那是下游包装层，不是本模块 API。

本地 `mvn -f java-connector/pom.xml package` 产出 **thin** jar（~20KB，不含 `ogsql_*`），**禁止** vendor / 当作可独立 parse 的发布物。只有 CI 断言过的 fat jar（≥1MB，内嵌五平台 `ogsql_*`）才是发布坐标。

### 3.1 从 GitHub Packages 消费

`~/.m2/settings.xml`：

```xml
<settings>
  <servers>
    <server>
      <id>github</id>
      <username>YOUR_GITHUB_USERNAME</username>
      <password>YOUR_GITHUB_TOKEN</password>
    </server>
  </servers>
</settings>
```

工程 POM：

```xml
<repositories>
  <repository>
    <id>github</id>
    <url>https://maven.pkg.github.com/c2j/ogsql-parser</url>
  </repository>
</repositories>

<dependency>
  <groupId>io.github.c2j</groupId>
  <artifactId>ogsql-parser-java</artifactId>
  <version>0.10.0</version>
</dependency>
```

Token 需要 `packages:read`。

### 3.2 从 Maven Central 消费（公开后）

```xml
<dependency>
  <groupId>io.github.c2j</groupId>
  <artifactId>ogsql-parser-java</artifactId>
  <version>0.10.0</version>
</dependency>
```

### 3.3 Central 首次发布的一次性准备工作（手动）

1. **注册账号**：访问 [central.sonatype.com](https://central.sonatype.com)，用 GitHub 账号登录。
2. **验证命名空间** `io.github.c2j`：Portal → Namespaces → 按提示验证归属。
3. **生成 PGP 密钥**并发布到 keyserver。
4. **生成 User Token**：Portal → User Tokens → Generate。
5. **仓库 Secrets**：`SONATYPE_USERNAME` / `SONATYPE_PASSWORD`、`GPG_PRIVATE_KEY`、`GPG_PASSPHRASE`、`GPG_KEY_ID`。

### 3.4 发布流程（CI）

- **GitHub Packages**：推送 `v*` tag，或 `workflow_dispatch` 勾选 `deploy_github`。CI 组装 fat jar、断言五平台资源后 `mvn -Pgithub deploy`。
- **Maven Central**：`workflow_dispatch` 勾选 `deploy_central`（需 §3.3 Secrets）。`mvn -Pcentral deploy` 签名后上传 Portal。

> Central 要求（pom `-Pcentral` 已满足）：非 SNAPSHOT、licenses/developers/scm、sources + javadoc、GPG 签名。Packages 不需要 GPG。

## 4. 平台与二进制（DuckDB 式加载）

jar 内嵌各平台二进制，运行时按 `os.name` + `os.arch` 自动选择并解压到临时目录：

| 平台 | jar 内资源路径 |
|---|---|
| Linux x86_64 | `/ogsql_linux_amd64` |
| Linux arm64 | `/ogsql_linux_arm64` |
| macOS x86_64 | `/ogsql_osx_amd64` |
| macOS arm64 | `/ogsql_osx_arm64` |
| Windows amd64 | `/ogsql_windows_amd64.exe` |

**解析顺序（三级回退，对齐 DuckDB 的 `-nolib` 思路）**：

1. `-Dogsql.lib.path=/path/to/ogsql`：指定外部二进制 —— 部署方自管二进制、**热升级只换二进制**（对应 DuckDB `-nolib` / `java.library.path`）；
2. jar 内平台资源：解压到临时文件 + `setExecutable` + `deleteOnExit`；
3. 均不可用：抛 `OgsqlException`，错误信息含完整指引。

> 平台矩阵由 CI（`.github/workflows/java-connector.yml`）构建 5 个平台的 `--features cli` 二进制并打进 jar；Win7（x86_64/i686）构建路径已在 release.yml 存在，后续以带 classifier 的变体接入。

## 5. API 参考

### 5.1 工厂方法（Ogsql）

| 方法 | 说明 |
|---|---|
| `static Ogsql newInstance()` | 默认：超时 30s，自动重启上限 3 次 |
| `static Ogsql newInstance(Duration timeout)` | 自定义单次调用超时 |
| `static Ogsql newInstance(Duration timeout, int maxRestarts)` | `maxRestarts=0` 关闭自动重启 |

### 5.2 解析 / 格式化 / 分词 / 校验 / 还原

```java
ParseResult parse(String sql);
ParseResult parse(String sql, boolean mybatis);              // mybatis=true 保留 #{param}/${expr}
ParseResult parse(String sql, boolean mybatis, boolean preserveComments);

String format(String sql);
String format(String sql, FormatOptions opts);

List<TokenInfo> tokenize(String sql);
List<TokenInfo> tokenize(String sql, boolean mybatis, boolean preserveComments);

Validation validate(String sql);
Validation validate(String sql, boolean mybatis, boolean strict);  // strict=true 检测 PL 块未定义函数

String json2sql(String astJson);   // 传 ParseResult.resultJson() 或 {"statements":[...]}

String version();                  // Rust crate 版本
boolean isAlive();                 // 子进程健康
void close();                      // 幂等，可重复调用
```

### 5.3 模型类

**ParseResult**：`statements()`（AST JSON，JsonNode 数组，与 HTTP/MCP 同构）、`errors()`、`queryFingerprints()`、`comments()`、`statementCount()`、`errorCount()`、`resultJson()`（可直接喂给 json2sql）。

**Validation**：`valid()`、`errors()`、`packageErrors()`、`undefinedVariableErrors()`、`statements()`、`errorCount()`。

**TokenInfo**：`type()`（Keyword/Ident/Integer/String/Op/MyBatisParam/…）、`value()`、`line()`、`column()`。

**FormatOptions**（Builder）：`indent`(默认2)、`keywordCase`(preserve/upper/lower)、`commaStyle`(trailing/leading)、`lineWidth`(120)、`uppercase`、`mybatis`、`noSelectNewline`、`noLogicalNewline`、`noSemicolonNewline`。

## 6. 错误语义与异常

**最重要的一条约定：SQL 的语法/语义问题不是异常。**

| 场景 | 表现 |
|---|---|
| SQL 有语法问题 | `parse` 正常返回，`errors()` 非空；`validate` 返回 `valid()==false`；**不抛异常** |
| 协议层故障 | 抛 `OgsqlException`，用 `e.code()` 判断类型 |

`OgsqlException` 错误码：

| code | 含义 | 处理建议 |
|---|---|---|
| `TOO_DEEP` | 表达式括号嵌套超 32 层（防解析器栈溢出） | 拆解 SQL；属防御性限制 |
| `TIMEOUT` | 单次调用超时（默认 30s） | 加大 timeout 或检查子进程状态 |
| `PROCESS_EXITED` | 子进程退出；已自动重启（上限内重试成功则调用正常返回） | 一般无需处理；反复出现查 stderr 日志 |
| `PROTOCOL_ERROR` / `INVALID_RESPONSE` | 协议/二进制异常 | 检查 jar 与二进制版本是否匹配 |
| `BAD_PARAM` / `UNKNOWN_OP` / `NOT_FOUND` | 请求参数问题 | 属于客户端 bug，反馈给维护方 |
| `INVALID_SQL` / `TOKENIZE_ERROR` / `BAD_JSON` | format/tokenize/json2sql 输入问题 | 修正输入 |
| `INTERNAL_ERROR` | 解析器内部 panic（已隔离，不影响服务） | 记录输入并反馈给维护方 |

## 7. 并发、生命周期与资源管理

- **并发模型：纯串行**。内部一把锁串行化所有调用（同一时刻只有一个请求在途），多线程并发调用安全但吞吐受单请求往返限制；**高吞吐请使用多个 `Ogsql` 实例**（每个实例 = 一个子进程）。
- **超时**：默认 30s/次；超时抛 `TIMEOUT`，可配置。
- **自动重启**：子进程 EOF → 指数退避（100ms 起，2 倍递增，上限 5s）→ 重启 + hello 握手 → 失败请求重试一次（五个 op 均为纯函数，重试安全）。`maxRestarts` 耗尽后该次调用抛 `PROCESS_EXITED`。
- **关闭**：`close()` 先发 `shutdown` 优雅退出 → `destroy()` → 超时 `destroyForcibly()`；幂等；构造函数注册了 JVM shutdown hook 兜底。**建议单例复用 + 应用退出时 close**。
- **资源**：每个实例常驻一个子进程（RSS 约 6MB，无状态），开销极小。

## 8. 性能数据

本地实测（macOS arm64，Java 17，debug 构建的 Rust 二进制）：

| 指标 | 数值 |
|---|---|
| 单次 parse 往返（含 JVM/Jackson/管道） | **≈0.4ms** |
| 2000 次连续往返压力测试 | 0.75s，零失败零泄漏 |
| 子进程崩溃 → 自动重启 → 重试成功 | ≈0.1s |

对比参考：同机 HTTP 方案数百 µs~ms 级起，JNI 方案 5–20µs（但代价是 JVM 崩溃风险与原生库分发工程）。

## 9. 故障排查 FAQ

**Q1：启动报 “no bundled ogsql binary for platform …”**
→ 你用的 jar 未包含当前平台二进制。解决：`-Dogsql.lib.path=/path/to/ogsql` 指向外部二进制；或改用 CI 打出的含 5 平台二进制的 jar。

**Q2：`-Dogsql.lib.path` 设置了仍报错？**
→ 确认路径可执行（Linux/macOS 需 `chmod +x`）、指向 `ogsql`（无需额外参数）。

**Q3：调用偶发 `PROCESS_EXITED`？**
→ 子进程被 kill 或崩溃；连接器会自动重启并重试一次，重试成功则调用正常返回。若频繁出现，把 `java.util.logging` 调到 FINE 查看 stderr 日志（logger 名 `io.github.c2j.ogsql`）。

**Q4：`protocol mismatch`？**
→ jar 与二进制版本不匹配：升级到同版本，或保持 `-Dogsql.lib.path` 与 jar 版本一致。

**Q5：SQL 很深（>32 层括号嵌套）被拒 `TOO_DEEP`？**
→ 防御性限制：递归下降解析器在 ~50 层即栈溢出且不可捕获，32 层留了安全余量。真实业务 SQL 极少触及；确需更深请拆解表达式。

**Q6：Windows 下解压到临时目录失败？**
→ 确认临时目录可写；如被安全策略限制，改用 `-Dogsql.lib.path` 指到本地二进制。

**Q7：需要更高吞吐？**
→ 起 N 个 `Ogsql` 实例做连接池（每个独立子进程），比 JNI 扩容简单得多。

## 10. Spring Boot 集成示例

```java
@Configuration
public class OgsqlConfig {

    @Bean(destroyMethod = "close")
    public Ogsql ogsql() {
        return Ogsql.newInstance();   // 单例复用；应用关闭时自动 close
    }
}

@Service
public class SqlAuditService {
    private final Ogsql ogsql;

    public SqlAuditService(Ogsql ogsql) { this.ogsql = ogsql; }

    /** 上线前 SQL 静态审查：解析 + 校验（配合 lint 扩展） */
    public void audit(String sql) {
        ParseResult r = ogsql.parse(sql, true);          // MyBatis 工程传入 mybatis=true
        if (r.errorCount() > 0) {
            throw new IllegalArgumentException("SQL 语法错误: " + r.errors());
        }
        Validation v = ogsql.validate(sql);
        if (!v.valid()) {
            throw new IllegalArgumentException("SQL 校验未通过: " + v.errors());
        }
    }
}
```

## 11. 版本兼容与升级

- `protocol` 字段：协议不兼容变更时递增；客户端握手时校验，不匹配抛 `OgsqlException`。
- 新增 op 属兼容变更（`hello` 的 `ops` 列表用于能力探测）。
- 升级建议：**jar 与二进制保持同版本**；使用外部二进制时，先升级二进制（`serve-stdio` 向后兼容）再视需要升级 jar。

## 相关文档

- [stdio-protocol.md](./stdio-protocol.md) — 线协议规范（客户端实现者必读）
- [java-connector-design.md](./java-connector-design.md) — 设计决策与里程碑
- [ast-json-reference.md](./ast-json-reference.md) — AST JSON 结构（Java 侧解析 AST 时参考）
- [gaussdb-sql-features.md](./gaussdb-sql-features.md) — 支持的 GaussDB 语法范围
