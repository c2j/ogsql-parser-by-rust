# Changelog

All notable changes to ogsql-parser will be documented in this file.

## [0.10.1]

### Fixed
- Parse `INSERT ... ON DUPLICATE KEY UPDATE NOTHING` — the openGauss Oracle-compat
  "do nothing on conflict" upsert form. `OnDuplicateKeyUpdate` is now an enum with
  `Nothing` / `Update { assignments, where_clause }` variants (mirroring
  `ConflictAction`); the formatter and JSON round-trip cover both forms (#328).
- Accept schema-qualified aggregate names in `CREATE AGGREGATE schema.name(...)`
  — the name is now parsed as a qualified object name (e.g. `public.group_concat`),
  matching the `parse_object_name` convention used by other CREATE parsers. Unquoted
  and quoted identifiers are stored as raw values, consistent with the rest of the
  codebase (#329).

## [0.10.0]

### Added
- Publish `ogsql-parser-java` (`io.github.c2j:ogsql-parser-java`) to Maven Central
  via the Central Portal: source/javadoc jars, GPG-signed artifacts, and a deploy
  pipeline in `.github/workflows/java-connector.yml` (secrets: SONATYPE_USERNAME/
  PASSWORD, GPG_PRIVATE_KEY, GPG_PASSPHRASE, GPG_KEY_ID). One-time manual steps
  (namespace registration, GPG key, user token) documented in
  `docs/java-developer-guide.md` §3.

### Fixed
- Parse inline DDL statements in PL/pgSQL bodies as a structured AST (previously
  inline DDL in PL bodies was not captured as structured AST). Scope push/pop in
  `parse_procedure_body` is now balanced via a wrapper pattern — `?` early-returns
  no longer leak a pushed scope frame that left phantom variable declarations after
  the inline-DDL helper backtracks from a malformed `CREATE PROCEDURE`; trailing
  semicolons are stripped from inline DDL `sql_text` (`;;` case). Regression guards
  added.

## [0.9.0]

### Added
- `serve-stdio` subcommand: long-lived NDJSON line protocol over stdin/stdout for
  embedded clients (Java/Python/Node). Ops: `hello`, `ping`, `shutdown`, `parse`,
  `format`, `tokenize`, `validate`, `json2sql`. Per-request panic isolation,
  16 MiB line cap, and a paren-nesting guard (32) that rejects input that would
  stack-overflow the recursive-descent parser. Protocol spec: `docs/stdio-protocol.md`.
- Integration tests for the stdio protocol: `tests/serve_stdio.rs` (18 tests).
- `java-connector/` Maven module (`io.github.c2j:ogsql-parser-java`): DuckDB-style
  platform-aware binary loading (`-Dogsql.lib.path` override → bundled jar resource),
  long-lived child-process supervisor with auto-restart/backoff and JVM shutdown hook,
  synchronous facade (`Ogsql.newInstance()` → parse/format/validate/tokenize/json2sql),
  17 JUnit tests (incl. crash recovery, TOO_DEEP, 2,000-iteration soak), and a CI workflow
  building 5 platform binaries + assembling the fat jar + GitHub Packages deploy.

### Changed
- MSRV raised from 1.70 to 1.71: the core proc-macro stack (syn/quote/serde_json/
  ryu/unicode-ident) moved to Rust 1.71 upstream, so Rust 1.70 could no longer
  build the library.
- `utoipa-swagger-ui` pinned to 9.0.0: 9.0.2 depends on zip ^3.0.0 (unresolvable
  by older Cargo), 9.0.1 depends on zip ^2.6 (2.6.0/2.6.1 are yanked); 9.0.0's
  zip ^2 → 2.4.2 restores resolution across supported toolchains.

## [0.8.33]

### Changed
- Codebase cleanup: add dual license files (LICENSE-MIT, LICENSE-APACHE)
- Relocate GaussDB reference docs from repo root to `docs/references/`
- Remove committed metadata files (.DS_Store, benchmark artifact) and stale/dead files
- Consolidate plan directories (`docs/plans/` → `.sisyphus/plans/`)
- Fix `BEST-PRATICE.md` typo → `BEST-PRACTICE.md`
- Merge `testcases/` into `tests/fixtures/`
- Fix placeholder repository URL in Cargo.toml
- Remove empty `[dev-dependencies]` section

### Added
- CHANGELOG.md
- CI: MSRV (Rust 1.70) verification job
- CI: weekly scheduled run
- CONTRIBUTING.md: documented minimal-vs-full submodule clone strategy

### Fixed
- Removed duplicate `stub_struct!` macro definition
- Removed unnecessary `Vec::new()` initializations in grant parser
- Removed duplicate `SetTransaction` match arm in formatter
- Removed duplicate `INT_P`/`INTEGER` in `matches!` macro
- Removed unreachable wildcard arm in token-to-string conversion
- Removed dead `#[cfg_attr(feature = "serde")]` wrappers (serde is always-on)
- Replaced standalone `into_iter()` with `impl IntoIterator for Parser`
- Scoped `large_enum_variant` allow to AST modules with `[PERMANENT]` rationale
- Fixed `&mut Vec` → `&mut [_]` for more general signature

## [0.8.32]

### Added
- Oracle `(+)` outer-join marker preserved in AST for full round-trip fidelity
- `Ident` quote-style preservation (`ObjectName` → `Vec<Ident>`) retains quoted-identifier style across parse/format round-trip
- JDBC `?` placeholder token (`JdbcParam`) for prepared-statement SQL
- iBatis/MyBatis XML mapper parsing with dynamic SQL AST
- Java source SQL extraction (tree-sitter based)
- MCP (Model Context Protocol) server for Claude Desktop, Cursor, etc.
- HTTP API server (`serve` / `serve-minimal` features) with RESTful endpoints
- Interactive TUI playground (`tui` feature)
- SQL anti-pattern linter with 53 rules (4 severity levels)
- Strict validation mode (detect undefined functions in PL blocks)
- MERGE semantic validation (non-deterministic/invalid pattern detection)
- Dynamic SQL analysis with variable tracing and EXECUTE IMMEDIATE resolution
- Schema loading and resolution from JSON
- Return cursor analysis for PL/pgSQL
- PL variable validation
- Query fingerprint computation
- Transaction analysis for PL blocks
- Package consistency validation (PACKAGE vs PACKAGE BODY)
- AST Visitor pattern (walk statements, PL blocks, expressions)
- Windows 7 support via Tier 3 target `x86_64-win7-windows-msvc`
- iBatis callable stored procedure support
- Full PL/pgSQL support (DO blocks, anonymous blocks, control flow, exception handling, GOTO)
- Complete DDL: CREATE/ALTER/DROP for all GaussDB object types
- Two-stage SQL formatter: structured AST pretty-print + configurable token formatter
- Multi-encoding support: UTF-8, EUC-JP, EUC-KR, GB18030, BIG5, UTF-16
- Token-level formatter with FormatConfig (indent, keyword case, comma style, line width)
- JSON serde round-trip (SQL → AST → JSON → AST → SQL)
- Benchmark suite comparing ogsql-parser vs sqlparser-rs, pglast, JSqlParser

### Fixed
- Numerous parser coverage improvements for GaussDB-specific syntax

---

[0.8.32]: https://github.com/c2j/ogsql-parser/releases/tag/v0.8.32
