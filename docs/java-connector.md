# ogsql-parser Java 连接器（java-connector）

> 用 openGauss/GaussDB SQL 解析能力的 Java 集成模块，体验对齐 DuckDB JDBC：
> **一个 Maven 依赖、零配置、开箱即用**。通信走子进程 `ogsql serve-stdio` 的
> stdio 长连接 NDJSON 协议（见 [stdio-protocol.md](./stdio-protocol.md)），
> 保留进程隔离：解析器崩溃不会影响 JVM，子进程自动重启。
>
> 📖 **完整参考**：[java-developer-guide.md](./java-developer-guide.md)（API 参考、错误码、FAQ、Spring 集成、性能数据）

## 快速开始

```xml
<dependency>
  <groupId>io.github.c2j</groupId>
  <artifactId>ogsql-parser-java</artifactId>
  <version>0.10.0</version>
</dependency>
```

```java
import io.github.c2j.ogsql.*;

try (Ogsql ogsql = Ogsql.newInstance()) {          // 自动定位二进制 + 启动 + 握手

    ParseResult r = ogsql.parse("SELECT * FROM t WHERE id = #{id}", true); // MyBatis 占位符
    r.errorCount(); r.statementCount(); r.statements();                    // AST JSON (JsonNode)

    String sql = ogsql.format("select a,b from t where x=1",
        FormatOptions.builder().keywordCase("upper").build());

    Validation v = ogsql.validate("SELECT FROM WHERE");  // v.valid() == false
    List<TokenInfo> tokens = ogsql.tokenize("SELECT 1");

    String back = ogsql.json2sql(r.resultJson());        // AST JSON → SQL 往返
}
```

## 二进制定位（DuckDB 式三级回退）

| 顺序 | 机制 |
|---|---|
| 1 | `-Dogsql.lib.path=/path/to/ogsql` — 外部二进制（热升级只换二进制，对应 DuckDB `-nolib`） |
| 2 | jar 内资源 `/ogsql_<os>_<arch>` — 自动解压到临时文件并赋予可执行权限 |
| 3 | 均不可用 → `OgsqlException`（错误信息含指引） |

平台命名：`linux_amd64`、`linux_arm64`、`osx_amd64`、`osx_arm64`、`windows_amd64.exe`。
CI 发布时把对应平台二进制打进 jar（见 `src/main/resources/README.md`）。

## 语义约定

- **语法/语义问题不是异常**：`parse` 返回 `ParseResult`（`errors()` 非空即有问题），
  `validate` 返回 `valid()==false`；**不抛异常**。
- **异常（`OgsqlException`，含 `code()`）** 仅用于协议层故障：`TOO_DEEP`（嵌套超限）、
  `TIMEOUT`、`PROCESS_EXITED`（子进程死亡）、`BAD_PARAM`、`UNKNOWN_OP` 等。

## 健壮性

- 子进程崩溃（含无法捕获的栈溢出 abort）→ stdout EOF → **自动重启（指数退避）+ 失败请求重试一次**；
  `Ogsql.newInstance(timeout, maxRestarts)` 可调；`maxRestarts=0` 关闭自动重启。
- 并发安全：**纯串行**模型，多线程并发调用由内部锁串行化（单次往返一个请求）。
- 超时：默认 30s/次，可配置；`close()` 先发 `shutdown` 优雅退出，超时 `destroyForcibly`，
  并有 JVM shutdown hook 兜底。

## 本地开发 / 测试

```bash
# 先构建 Rust 二进制（debug 即可）
cargo build --features cli
# Java 测试自动回退使用 ../target/debug/ogsql
mvn -f java-connector/pom.xml test
# 或显式指定二进制
mvn -f java-connector/pom.xml test -Dogsql.lib.path=/path/to/ogsql
```

## 发布

**GitHub Packages 常驻**（`maven.pkg.github.com/c2j/ogsql-parser`，每个 `v*` tag 与手动 `deploy_github`）；
**Maven Central** 为额外公开通道（手动 `deploy_central`）。详见 [java-developer-guide.md](./java-developer-guide.md) §3。

本地 `mvn -f java-connector/pom.xml package` 产出的是 **thin** jar（~20KB，不含 `ogsql_*`），**不能**当作可独立 parse 的发布物。只有 CI 断言过的 fat jar（≥1MB，内嵌五平台二进制）才是发布坐标。

`-Dogsql.lib.path` 是可选覆盖，不是默认前提。连接器没有 `OGSQL_BIN` 环境变量。
