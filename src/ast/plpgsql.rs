//! PL/pgSQL AST types for procedural language blocks.

// [PERMANENT] AST enums naturally have heterogeneous variant sizes.
// See ast/mod.rs for full rationale.
#![allow(clippy::large_enum_variant)]

use crate::ast::Spanned;
use serde::{Deserialize, Serialize};
use std::fmt;

// ── Block Structure ──

/// A PL/pgSQL block: [label:] [DECLARE decls] BEGIN stmts [EXCEPTION handlers] END [label]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlBlock {
    pub label: Option<String>,
    pub declarations: Vec<PlDeclaration>,
    pub body: Vec<PlStatement>,
    pub exception_block: Option<PlExceptionBlock>,
    pub end_label: Option<String>,
}

// ── Declarations ──

/// Declaration in a DECLARE section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlDeclaration {
    Variable(PlVarDecl),
    Cursor(PlCursorDecl),
    Record(PlRecordDecl),
    Type(PlTypeDecl),
    NestedProcedure(crate::ast::PackageProcedure),
    NestedFunction(crate::ast::PackageFunction),
    Pragma { name: String, arguments: String },
}

/// Variable declaration: name [CONSTANT] type [NOT NULL] [:= expr | DEFAULT expr] [COLLATE name]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlVarDecl {
    pub name: String,
    pub data_type: PlDataType,
    pub default: Option<crate::ast::Expr>,
    pub constant: bool,
    pub not_null: bool,
    pub collate: Option<String>,
}

/// PL/pgSQL data types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlDataType {
    /// Regular type name (e.g., INTEGER, TEXT, VARCHAR(100))
    TypeName(String),
    /// Anchored type: table.column%TYPE
    PercentType { table: String, column: String },
    /// Anchored row type: table%ROWTYPE
    PercentRowType(String),
    /// RECORD type
    Record,
    /// CURSOR type (for cursor variables)
    Cursor,
    /// REFCURSOR type
    RefCursor,
}

/// Cursor declaration: cursor_name [([args])] CURSOR [(return_type)] FOR query
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlCursorDecl {
    pub name: String,
    pub arguments: Vec<PlCursorArg>,
    pub return_type: Option<PlDataType>,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_query: Option<Box<crate::ast::Statement>>,
    pub scrollable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlCursorArg {
    pub name: String,
    pub data_type: PlDataType,
    pub mode: PlArgMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlArgMode {
    In,
    Out,
    InOut,
}

/// Record type declaration: name RECORD
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlRecordDecl {
    pub name: String,
}

/// TYPE declaration: name IS RECORD (fields) | name IS TABLE OF type | name IS VARRAY(n) OF type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlTypeDecl {
    Record { name: String, fields: Vec<PlTypeField> },
    TableOf { name: String, elem_type: PlDataType, index_by: Option<PlDataType> },
    VarrayOf { name: String, size: Box<crate::ast::Expr>, elem_type: PlDataType },
    RefCursor { name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlTypeField {
    pub name: String,
    pub data_type: PlDataType,
}

// ── Statements ──

/// A PL/pgSQL statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlStatement {
    Block(Spanned<PlBlock>),

    Assignment {
        target: crate::ast::Expr,
        expression: crate::ast::Expr,
    },

    If(Spanned<PlIfStmt>),
    Case(Spanned<PlCaseStmt>),
    Loop(Spanned<PlLoopStmt>),
    While(Spanned<PlWhileStmt>),
    For(Spanned<PlForStmt>),
    ForEach(Spanned<PlForEachStmt>),

    Exit {
        label: Option<String>,
        condition: Option<crate::ast::Expr>,
    },

    Continue {
        label: Option<String>,
        condition: Option<crate::ast::Expr>,
    },

    Return {
        expression: Option<crate::ast::Expr>,
    },

    ReturnNext {
        expression: crate::ast::Expr,
    },

    ReturnQuery(Spanned<PlReturnQueryStmt>),
    Raise(Spanned<PlRaiseStmt>),
    Execute(Spanned<PlExecuteStmt>),

    Perform {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        span: Option<crate::ast::SourceSpan>,
        query: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed_query: Option<Box<crate::ast::Statement>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed_expr: Option<Box<crate::ast::Expr>>,
    },

    Open(Spanned<PlOpenStmt>),
    Fetch(Spanned<PlFetchStmt>),

    Close {
        cursor: crate::ast::Expr,
    },

    Move {
        cursor: crate::ast::Expr,
        direction: Option<FetchDirection>,
    },

    GetDiagnostics(Spanned<PlGetDiagStmt>),
    Commit {
        #[serde(default)]
        and_chain: bool,
    },
    Rollback {
        to_savepoint: Option<String>,
        #[serde(default)]
        and_chain: bool,
    },
    Savepoint {
        name: String,
    },
    ReleaseSavepoint {
        name: String,
    },
    SetTransaction {
        isolation_level: Option<PlIsolationLevel>,
        read_only: Option<bool>,
        deferrable: Option<bool>,
    },
    Null,

    Goto {
        label: String,
    },

    ProcedureCall(Spanned<PlProcedureCall>),

    #[serde(rename = "sql_text")]
    Sql(String),

    SqlStatement {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        span: Option<crate::ast::SourceSpan>,
        sql_text: String,
        #[serde(flatten)]
        statement: Box<crate::ast::Statement>,
    },

    ForAll(Spanned<PlForAllStmt>),

    PipeRow {
        expression: crate::ast::Expr,
    },

    VariableSet(Spanned<crate::ast::VariableSetStatement>),

    VariableReset(Spanned<crate::ast::VariableResetStatement>),
}

// ── Statement Detail Types ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlProcedureCall {
    pub name: crate::ast::ObjectName,
    pub arguments: Vec<crate::ast::Expr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<crate::ast::BuiltinFuncMeta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlIfStmt {
    pub condition: crate::ast::Expr,
    pub then_stmts: Vec<PlStatement>,
    pub elsifs: Vec<PlElsif>,
    pub else_stmts: Vec<PlStatement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlElsif {
    pub condition: crate::ast::Expr,
    pub stmts: Vec<PlStatement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlCaseStmt {
    pub expression: Option<crate::ast::Expr>,
    pub whens: Vec<PlCaseWhen>,
    pub else_stmts: Vec<PlStatement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlCaseWhen {
    pub condition: crate::ast::Expr,
    pub stmts: Vec<PlStatement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlLoopStmt {
    pub label: Option<String>,
    pub body: Vec<PlStatement>,
    pub end_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlWhileStmt {
    pub label: Option<String>,
    pub condition: crate::ast::Expr,
    pub body: Vec<PlStatement>,
    pub end_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlForStmt {
    pub label: Option<String>,
    pub variable: String,
    pub kind: PlForKind,
    pub body: Vec<PlStatement>,
    pub end_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlForKind {
    /// FOR i IN low..high [BY step] LOOP
    Range { low: crate::ast::Expr, high: crate::ast::Expr, step: Option<crate::ast::Expr>, reverse: bool },
    /// FOR rec IN query LOOP
    Query {
        query: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed_query: Option<Box<crate::ast::Statement>>,
        #[serde(default)]
        using_args: Vec<PlUsingArg>,
    },
    /// FOR rec IN cursor_name [([args])] LOOP
    Cursor { cursor_name: crate::ast::Expr, arguments: Vec<crate::ast::Expr> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlForEachStmt {
    pub label: Option<String>,
    pub variable: String,
    pub expression: crate::ast::Expr,
    pub slice: Option<i32>,
    pub body: Vec<PlStatement>,
    pub end_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlForAllStmt {
    pub variable: String,
    pub bounds: String,
    pub save_exceptions: bool,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlRaiseStmt {
    pub level: Option<RaiseLevel>,
    pub message: Option<String>,
    pub params: Vec<crate::ast::Expr>,
    pub options: Vec<RaiseOption>,
    pub condname: Option<String>,
    pub sqlstate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RaiseLevel {
    Debug,
    Log,
    Info,
    Notice,
    Warning,
    Exception,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaiseOption {
    pub name: String,
    pub value: crate::ast::Expr,
}

/// Parameter passing mode for EXECUTE IMMEDIATE ... USING arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlUsingMode {
    In,
    Out,
    InOut,
}

/// A single argument in a USING clause with its passing mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlUsingArg {
    pub mode: PlUsingMode,
    pub argument: crate::ast::Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlExecuteStmt {
    /// Whether the IMMEDIATE keyword was present
    pub immediate: bool,
    /// The dynamic SQL string expression (may be a concatenation)
    pub string_expr: crate::ast::Expr,
    /// INTO target variables for query results
    #[serde(default)]
    pub into_targets: Vec<crate::ast::Expr>,
    /// USING arguments with IN/OUT/INOUT mode
    #[serde(default)]
    pub using_args: Vec<PlUsingArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_query: Option<Box<crate::ast::Statement>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlReturnQueryStmt {
    pub query: String,
    pub is_dynamic: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_expr: Option<crate::ast::Expr>,
    #[serde(default)]
    pub using_args: Vec<PlUsingArg>,
    /// Parsed AST of a static `RETURN QUERY <dml>` body (for linter/analyzer traversal).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parsed_query: Option<Box<crate::ast::Statement>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlOpenStmt {
    pub cursor: crate::ast::Expr,
    pub kind: PlOpenKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlOpenKind {
    /// OPEN cursor [([args])]
    Simple { arguments: Vec<crate::ast::Expr> },
    /// OPEN cursor [NO] SCROLL FOR query
    ForQuery {
        #[serde(skip_serializing_if = "Option::is_none")]
        scroll: Option<bool>,
        query: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parsed_query: Option<Box<crate::ast::Statement>>,
    },
    /// OPEN cursor FOR EXECUTE query_string [USING expr, ...]
    ForExecute {
        query: crate::ast::Expr,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        using_args: Vec<crate::ast::Expr>,
    },
    /// OPEN cursor FOR USING expr, ...
    ForUsing { expressions: Vec<crate::ast::Expr> },
}

/// Direction keyword for FETCH and MOVE statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FetchDirection {
    Next,
    Prior,
    First,
    Last,
    Absolute(i64),
    Relative(i64),
    Forward(Option<i64>),  // None = bare FORWARD
    Backward(Option<i64>), // None = bare BACKWARD
    ForwardAll,
    BackwardAll,
    All,
}

impl fmt::Display for FetchDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchDirection::Next => write!(f, "NEXT"),
            FetchDirection::Prior => write!(f, "PRIOR"),
            FetchDirection::First => write!(f, "FIRST"),
            FetchDirection::Last => write!(f, "LAST"),
            FetchDirection::Absolute(n) => write!(f, "ABSOLUTE {}", n),
            FetchDirection::Relative(n) => write!(f, "RELATIVE {}", n),
            FetchDirection::Forward(None) => write!(f, "FORWARD"),
            FetchDirection::Forward(Some(n)) => write!(f, "FORWARD {}", n),
            FetchDirection::Backward(None) => write!(f, "BACKWARD"),
            FetchDirection::Backward(Some(n)) => write!(f, "BACKWARD {}", n),
            FetchDirection::ForwardAll => write!(f, "FORWARD ALL"),
            FetchDirection::BackwardAll => write!(f, "BACKWARD ALL"),
            FetchDirection::All => write!(f, "ALL"),
        }
    }
}

/// GET DIAGNOSTICS item kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetDiagItemKind {
    RowCount,
    ResultStatus,
    ReturnedSqlstate,
    MessageText,
    Detail,
    Hint,
    Context,
    SchemaName,
    TableName,
    ColumnName,
    DatatypeName,
    ConstraintName,
    PgExceptionContext,
}

impl fmt::Display for GetDiagItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetDiagItemKind::RowCount => write!(f, "ROW_COUNT"),
            GetDiagItemKind::ResultStatus => write!(f, "RESULT_STATUS"),
            GetDiagItemKind::ReturnedSqlstate => write!(f, "RETURNED_SQLSTATE"),
            GetDiagItemKind::MessageText => write!(f, "MESSAGE_TEXT"),
            GetDiagItemKind::Detail => write!(f, "DETAIL"),
            GetDiagItemKind::Hint => write!(f, "HINT"),
            GetDiagItemKind::Context => write!(f, "CONTEXT"),
            GetDiagItemKind::SchemaName => write!(f, "SCHEMA_NAME"),
            GetDiagItemKind::TableName => write!(f, "TABLE_NAME"),
            GetDiagItemKind::ColumnName => write!(f, "COLUMN_NAME"),
            GetDiagItemKind::DatatypeName => write!(f, "DATATYPE_NAME"),
            GetDiagItemKind::ConstraintName => write!(f, "CONSTRAINT_NAME"),
            GetDiagItemKind::PgExceptionContext => write!(f, "PG_EXCEPTION_CONTEXT"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlFetchStmt {
    pub cursor: crate::ast::Expr,
    pub direction: Option<FetchDirection>,
    #[serde(default)]
    pub bulk_collect: bool,
    #[serde(default)]
    pub into: Vec<crate::ast::Expr>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlGetDiagStmt {
    pub stacked: bool,
    pub items: Vec<PlGetDiagItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlGetDiagItem {
    pub target: crate::ast::Expr,
    pub item: GetDiagItemKind,
}

// ── Exception Handling ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlExceptionBlock {
    pub handlers: Vec<PlExceptionHandler>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlExceptionHandler {
    pub conditions: Vec<String>,
    pub statements: Vec<PlStatement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlIsolationLevel {
    ReadCommitted,
    RepeatableRead,
    Serializable,
}
