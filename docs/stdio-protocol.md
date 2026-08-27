# ogsql serve-stdio 协议规范（NDJSON over stdio）

> `serve-stdio` 是 `ogsql` 的一个子命令（属 `cli` feature，无额外依赖）：在 stdin/stdout 上提供
> 长连接的换行分隔 JSON（NDJSON）请求/响应通道，供 Java/Python/Node 等客户端以子进程方式嵌入。
> 设计对齐 MCP stdio / LSP 模式，并保留进程隔离（子进程崩溃不影响宿主 JVM）。
>
> - 版本：protocol v1（`hello` 返回）
> - 状态：已实现

## 1. 传输与帧

- stdin 每行一个**请求** JSON 对象，stdout 每行一个**响应** JSON 对象（UTF-8，LF 结尾）。
- JSON 字符串内的换行由序列化器转义（`\\n`），因此“一行一对象”无歧义。
- 单行上限 **16 MiB**；超限响应 `LINE_TOO_LONG`（id = -1）。
- 服务端严格按序处理，响应顺序 = 请求顺序。
- stdin 关闭（EOF）→ 服务正常退出（exit 0）。
- 所有日志输出到 stderr，不污染 stdout 协议通道。

## 2. 请求 / 响应

请求（未知字段忽略，字段缺失/类型错误按各 op 规则处理）：

```json
{"id": 1, "op": "parse", "sql": "SELECT 1", "mybatis": true}
```

响应成功：

```json
{"id": 1, "ok": true, "result": { ... }}
```

响应失败（仅协议层故障）：

```json
{"id": 1, "ok": false, "error": {"code": "BAD_PARAM", "message": "missing required param: sql"}}
```

### 2.1 错误码

| code | 含义 |
|---|---|
| `PROTOCOL_ERROR` | 请求不是合法 JSON（id = -1） |
| `LINE_TOO_LONG` | 请求行超 16 MiB（id = -1） |
| `UNKNOWN_OP` | 未知 op |
| `BAD_PARAM` | 缺少必需参数 |
| `NOT_FOUND` | `parse.procedure` 指定的过程/函数不存在 |
| `INVALID_SQL` | `format` 分词失败 |
| `TOKENIZE_ERROR` | `tokenize` 分词失败 |
| `BAD_JSON` | `json2sql` 输入 JSON 无效 |
| `TOO_DEEP` | 表达式括号嵌套超过 32 层（防解析器栈溢出，见 §7） |
| `INTERNAL_ERROR` | 请求处理中发生内部 panic（服务循环继续运行） |

### 2.2 错误语义（重要约定）

- **语法/语义问题不是协议错误**：`parse` 对非法 SQL 返回 `ok: true` 且 `result.errors` 非空
  （解析器带错误恢复）；`validate` 返回 `ok: true` 且 `result.valid: false`。
- `ok: false` 仅用于 2.1 中协议层故障。

## 3. op 一览

| op | 必需参数 | 可选参数 | result |
|---|---|---|---|
| `hello` | — | `client_version` | `version`(Rust crate 版本)、`protocol`(=1)、`ops` |
| `ping` | — | — | `pong: true` |
| `shutdown` | — | — | `bye: true`，随后进程退出 0 |
| `parse` | `sql` | `preserve_comments`、`mybatis`、`procedure` | `statements`、`errors`、`query_fingerprints`、`comments` |
| `format` | `sql` | `indent`、`keyword_case`(preserve/upper/lower)、`comma_style`(trailing/leading)、`line_width`、`uppercase`、`mybatis`、`no_select_newline`、`no_logical_newline`、`no_semicolon_newline` | `sql`（格式化结果） |
| `tokenize` | `sql` | `preserve_comments`、`mybatis` | `tokens`: `[{type,value,line,column}]` |
| `validate` | `sql` | `mybatis`、`strict` | `valid`、`statements`、`errors`、`package_errors`、`undefined_variable_errors` |
| `json2sql` | `json`（字符串） | — | `sql`（还原的 SQL，多条以 `;\n` 连接） |

- `parse` 的 `statements` 为 StatementInfo 数组（含 `sql_text`），可直接回传给 `json2sql`。
- AST JSON 结构与 HTTP serve / MCP 输出一致，详见 [ast-json-reference.md](./ast-json-reference.md)。

## 4. 示例会话

```bash
$ ogsql serve-stdio
{"id":1,"op":"hello"}
{"id":1,"ok":true,"result":{"version":"0.10.1","protocol":1,"ops":["hello","ping","shutdown","parse","format","tokenize","validate","json2sql"]}}
{"id":2,"op":"parse","sql":"SELECT id, name FROM users WHERE status = 'active'"}
{"id":2,"ok":true,"result":{"statements":[{...}],"errors":[],"query_fingerprints":[...],"comments":null}}
{"id":3,"op":"validate","sql":"SELECT FROM"}
{"id":3,"ok":true,"result":{"valid":false,"statements":[],"errors":[{...}],"package_errors":[],"undefined_variable_errors":[]}}
{"id":4,"op":"shutdown"}
{"id":4,"ok":true,"result":{"bye":true}}
```

## 5. 客户端要求

- 必须以**行缓冲 + 每请求 flush** 方式写入 stdin，并在独立线程持续读取 stdout（防止管道缓冲区满死锁）。
- 启动后应先发 `hello` 校验 `protocol` 与二进制版本兼容。
- 进程退出（stdout EOF）后应重启子进程（建议指数退避），并把在途请求标记失败。
- 关闭时先发 `shutdown`，超时未退出再 `destroyForcibly`。

## 6. 版本演进

- `protocol` 字段在出现不兼容变更时递增；客户端据此决定拒绝或降级。
- 新增 op 视为兼容变更，不递增 `protocol`（`hello` 的 `ops` 列表用于能力探测）。

## 7. 健壮性

- **嵌套深度守卫**：递归下降解析器对极深括号嵌套会栈溢出（实测约 50 层即崩溃，且
  栈溢出不可捕获、直接终止进程）。服务端在解析前对 token 流做括号深度检查，
  超过 32 层返回 `TOO_DEEP` 错误，进程不受影响。
- **panic 兜底**：每个请求的处理包在 `catch_unwind` 中，单个请求的内部 panic 返回
  `INTERNAL_ERROR` 后循环继续。
- **进程级兜底**：即使发生无法捕获的崩溃（栈溢出、abort），进程退出 = stdout EOF，
  客户端据此重启子进程（见 §5）。这是设计内行为，不是缺陷。
- 单行 16 MiB 上限防止内存耗尽；未知字段忽略、类型不符按缺省处理。
