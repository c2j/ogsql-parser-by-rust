// [PERMANENT] AST enums naturally have heterogeneous variant sizes
// (e.g. SelectStatement vs NullLiteral). Boxing large variants would
// break serde compatibility and add allocation overhead on the hot path.
#![allow(clippy::large_enum_variant)]

pub mod ident;
pub mod plpgsql;

use crate::token::SourceLocation;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct SourceSpan {
    pub start: SourceLocation,
    pub end: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Spanned<T> {
    #[serde(flatten)]
    pub node: T,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub span: Option<SourceSpan>,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Option<SourceSpan>) -> Self {
        Self { node, span }
    }

    pub fn without_span(node: T) -> Self {
        Self { node, span: None }
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.node
    }
}

impl<T> std::ops::DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.node
    }
}

// ── Builtin Function Metadata ──

/// Metadata for a recognized built-in function, attached to `FunctionCall` AST nodes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinFuncMeta {
    pub category: String,
    pub domain: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IlmPolicy {
    pub after_n: u64,
    pub unit: String,
    pub condition: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTableStatement {
    pub temporary: bool,
    pub unlogged: bool,
    pub if_not_exists: bool,
    pub name: ObjectName,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<TableConstraint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub like_clauses: Vec<LikeClause>,
    pub inherits: Vec<ObjectName>,
    pub partition_by: Option<PartitionClause>,
    pub subpartition_by: Option<PartitionClause>,
    pub subpartitions_count: Option<u32>,
    pub distribute_by: Option<DistributeClause>,
    pub to_group: Option<String>,
    pub tablespace: Option<String>,
    pub on_commit: Option<OnCommitAction>,
    pub options: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub table_options: Vec<(String, String)>,
    pub compress: Option<bool>,
    pub ilm: Option<IlmPolicy>,
    pub row_movement: Option<bool>,
}

/// LIKE source_table clause inside CREATE TABLE column list.
/// Syntax: `LIKE source_table [like_option ...]`
/// like_option: `{ INCLUDING | EXCLUDING } { DEFAULTS | CONSTRAINTS | INDEXES | STORAGE | COMMENTS | PARTITION | RELOPTIONS | DISTRIBUTION | ALL }`
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LikeClause {
    pub source_table: ObjectName,
    /// Each option is (is_including, option_name).
    /// is_including: true = INCLUDING, false = EXCLUDING
    pub options: Vec<(bool, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTableAsStatement {
    pub temporary: bool,
    pub unlogged: bool,
    pub if_not_exists: bool,
    pub name: ObjectName,
    pub column_names: Vec<String>,
    pub query: Box<SelectStatement>,
    pub as_table: Option<ObjectName>,
    pub with_data: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DistributeClause {
    Hash { columns: Vec<String> },
    Replication,
    RoundRobin { columns: Vec<String> },
    Modulo { columns: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PartitionClause {
    Range {
        columns: Vec<ObjectName>,
        interval: Option<Expr>,
        is_columns: bool,
        partitions_count: Option<u32>,
        partitions: Vec<PartitionDef>,
    },
    List {
        columns: Vec<ObjectName>,
        is_columns: bool,
        partitions: Vec<PartitionDef>,
    },
    Hash {
        columns: Vec<ObjectName>,
        partitions_count: Option<u32>,
        partitions: Vec<PartitionDef>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OnCommitAction {
    PreserveRows,
    DeleteRows,
    Drop,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GeneratedColumn {
    pub expr: Expr,
    pub stored: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub constraints: Vec<ColumnConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compress_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<GeneratedColumn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_with: Option<EncryptedWith>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EncryptedWith {
    pub column_encryption_key: String,
    pub encryption_type: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IntervalType {
    pub from: String,
    pub from_precision: Option<u32>,
    pub to: Option<String>,
    pub to_precision: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DataType {
    Boolean,
    TinyInt(Option<u32>),
    SmallInt(Option<u32>),
    Integer(Option<u32>),
    BigInt(Option<u32>),
    Real,
    Float(Option<u32>),
    Double,
    Numeric(Option<u32>, Option<u32>),
    Char(Option<u32>),
    Varchar(Option<u32>),
    Text,
    Bytea,
    Timestamp(Option<u32>, Option<TimeZoneInfo>),
    Timestamptz(Option<u32>),
    Date,
    Time(Option<u32>, Option<TimeZoneInfo>),
    Interval(Option<IntervalType>),
    Json,
    Jsonb,
    Uuid,
    Bit(Option<u32>),
    Varbit(Option<u32>),
    Serial,
    SmallSerial,
    BigSerial,
    BinaryFloat,
    BinaryDouble,
    Array(Box<DataType>),
    Custom(ObjectName, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TimeZoneInfo {
    WithTimeZone,
    WithoutTimeZone,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ColumnConstraint {
    NotNull,
    Null,
    Default(Expr),
    Unique,
    PrimaryKey,
    Check(Expr),
    References {
        ref_table: ObjectName,
        ref_columns: Vec<String>,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TableConstraint {
    PrimaryKey {
        columns: Vec<String>,
        using_index: Option<String>,
    },
    Unique {
        columns: Vec<String>,
        deferrable: bool,
        with_options: Vec<(String, String)>,
        using_index: Option<String>,
    },
    Check(Expr),
    ForeignKey {
        columns: Vec<String>,
        ref_table: ObjectName,
        ref_columns: Vec<String>,
        on_delete: Option<ReferentialAction>,
        on_update: Option<ReferentialAction>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ReferentialAction {
    Cascade,
    Restrict,
    SetNull,
    SetDefault,
    NoAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTablespaceStatement {
    pub name: String,
    pub action: AlterTablespaceAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterTablespaceAction {
    RenameTo { new_name: String },
    OwnerTo { new_owner: String },
    SetOptions { options: Vec<(String, String)> },
    ResetOptions { options: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTableStatement {
    pub if_exists: bool,
    pub name: ObjectName,
    pub actions: Vec<AlterTableAction>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterTableAction {
    AddColumn(ColumnDef),
    DropColumn {
        name: String,
        if_exists: bool,
        cascade: bool,
    },
    DropIndex {
        name: String,
        if_exists: bool,
    },
    AlterColumn {
        name: String,
        action: AlterColumnAction,
    },
    AddConstraint {
        name: Option<String>,
        constraint: TableConstraint,
    },
    AddConstraintIfExists {
        name: String,
    },
    DropConstraint {
        name: String,
        if_exists: bool,
        cascade: bool,
    },
    RenameColumn {
        old: String,
        new: String,
    },
    RenameTo {
        new_name: String,
    },
    OwnerTo {
        owner: String,
    },
    SetSchema {
        schema: String,
    },
    SetOptions {
        options: Vec<(String, String)>,
    },
    SetTablespace {
        tablespace: String,
    },
    SetWithoutOids,
    ResetOptions {
        options: Vec<String>,
    },
    AddPartition {
        name: String,
        values: PartitionValues,
        tablespace: Option<String>,
    },
    DropPartition {
        name: String,
        if_exists: bool,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    TruncatePartition {
        name: String,
        for_values: Option<Vec<Expr>>,
        cascade: bool,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    MergePartitions {
        names: Vec<String>,
        into_name: String,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    SplitPartition {
        name: String,
        at_value: Option<Expr>,
        into: Vec<PartitionDef>,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    ExchangePartition {
        name: String,
        table: ObjectName,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
        with_validation: Option<bool>,
        verbose: bool,
    },
    RenamePartition {
        old_name: String,
        new_name: String,
    },
    AddSubPartition {
        partition_name: String,
        name: String,
        values: Option<PartitionValues>,
    },
    DropSubPartition {
        name: String,
        if_exists: bool,
    },
    TruncateSubPartition {
        name: String,
        cascade: bool,
    },
    MergeSubPartitions {
        names: Vec<String>,
        into_name: String,
    },
    SplitSubPartition {
        name: String,
        at_value: Option<Expr>,
        into: Vec<PartitionDef>,
    },
    ExchangeSubPartition {
        name: String,
        table: ObjectName,
    },
    RenameSubPartition {
        old_name: String,
        new_name: String,
    },
    MovePartition {
        name: String,
        tablespace: String,
    },
    MovePartitionFor {
        expr: Expr,
        tablespace: String,
    },
    SplitPartitionFor {
        expr: Expr,
        at_value: Option<Expr>,
        into: Vec<PartitionDef>,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    MoveSubPartition {
        name: String,
        tablespace: String,
    },
    DropPartitionFor {
        expr: Expr,
        if_exists: bool,
        update_global_index: bool,
        update_distributed_global_index: Option<bool>,
    },
    RenamePartitionFor {
        expr: Expr,
        new_name: String,
    },
    EnableRowLevelSecurity,
    DisableRowLevelSecurity,
    EnableRowMovement,
    DisableRowMovement,
    SetCharset {
        charset: String,
        collation: Option<String>,
    },
    EnableTrigger {
        name: Option<String>,
    },
    DisableTrigger {
        name: Option<String>,
    },
    ValidateConstraint {
        name: String,
    },
    AddConstraintUsingIndex {
        name: String,
        index_name: String,
    },
    Inherit {
        parent: ObjectName,
    },
    NoInherit {
        parent: ObjectName,
    },
    ClusterOn {
        index_name: String,
    },
    SetWithoutCluster,
    ReplicaIdentity(ReplicaIdentity),
    SetCompress,
    SetNoCompress,
    ForceRowLevelSecurity,
    NoForceRowLevelSecurity,
    OfType {
        type_name: ObjectName,
    },
    NotOfType {
        type_name: ObjectName,
    },
    AddNode {
        node_name: String,
    },
    DeleteNode {
        node_name: String,
    },
    SetComment {
        comment: String,
    },
    IlmAddPolicy(IlmPolicy),
    IlmEnablePolicy,
    IlmEnableAllPolicies,
    IlmDisablePolicy,
    IlmDisableAllPolicies,
    IlmDeletePolicy,
    IlmDeleteAllPolicies,
    // Multi-column MODIFY
    ModifyColumns(Vec<ModifyColumnInfo>),
    ModifyPartition {
        name: String,
        action: Box<AlterTableAction>,
    },
    ModifySubPartition {
        name: String,
        action: Box<AlterTableAction>,
    },
    // Multi-column ADD
    AddColumns(Vec<ColumnDef>),
    // Statistics operations
    StatisticsOp {
        op: StatisticsOpKind,
        columns: Vec<String>,
    },
    // ALTER COLUMN ... SET STATISTICS [PERCENT] integer
    AlterColumnStatistics {
        column: String,
        percent: bool,
        value: i64,
    },
    // ALTER COLUMN ... SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN }
    AlterColumnStorage {
        column: String,
        storage: String,
    },
    // GSIWAITALL
    GsiWaitAll,
    // ENCRYPTION KEY ROTATION
    EncryptionKeyRotation,
    // ENABLE/DISABLE RULE
    SetRule {
        enable: bool,
        mode: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModifyColumnInfo {
    pub name: String,
    pub data_type: DataType,
    pub nullability: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StatisticsOpKind {
    Add,
    Delete,
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ReplicaIdentity {
    Default,
    Nothing,
    Full,
    Index { name: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PartitionValues {
    LessThan(Vec<Expr>),
    InValues(Vec<Expr>),
    StartEnd { start: Expr, end: Expr, every: Option<Expr> },
    StartOnly { start: Expr },
    EndOnly { end: Expr, every: Option<Expr> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PartitionDef {
    pub name: String,
    pub values: Option<PartitionValues>,
    pub tablespace: Option<String>,
    pub subpartitions: Vec<PartitionDef>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterColumnAction {
    SetDataType(DataType),
    SetDefault(Expr),
    DropDefault,
    SetNotNull,
    DropNotNull,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DropStatement {
    pub object_type: ObjectType,
    pub if_exists: bool,
    pub names: Vec<ObjectName>,
    pub cascade: bool,
    pub purge: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObjectType {
    Table,
    Index,
    Sequence,
    View,
    Schema,
    Database,
    Tablespace,
    Function,
    Procedure,
    Trigger,
    Extension,
    MaterializedView,
    ForeignTable,
    ForeignServer,
    Fdw,
    Aggregate,
    Cast,
    Conversion,
    Operator,
    OperatorClass,
    OperatorFamily,
    Rule,
    Language,
    TextSearchConfig,
    TextSearchDict,
    Domain,
    Policy,
    User,
    Role,
    Group,
    ResourcePool,
    ResourceLabel,
    WorkloadGroup,
    AuditPolicy,
    MaskingPolicy,
    RlsPolicy,
    DataSource,
    Directory,
    Event,
    Publication,
    Subscription,
    Synonym,
    Model,
    SecurityLabel,
    UserMapping,
    WeakPasswordDictionary,
    PolicyLabel,
    Node,
    NodeGroup,
    App,
    Global,
    OpClass,
    OpFamily,
    Type,
    Package,
    DatabaseLink,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateIndexStatement {
    pub unique: bool,
    pub if_not_exists: bool,
    pub concurrent: bool,
    pub name: Option<ObjectName>,
    pub table: ObjectName,
    pub using_method: Option<String>,
    pub columns: Vec<IndexColumn>,
    pub tablespace: Option<String>,
    pub where_clause: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IndexColumn {
    pub name: Option<String>,
    pub expr: Option<Expr>,
    pub collation: Option<String>,
    pub opclass: Option<String>,
    pub asc: Option<bool>,
    pub nulls: Option<IndexNulls>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateGlobalIndexStatement {
    pub unique: bool,
    pub concurrent: bool,
    pub if_not_exists: bool,
    pub name: Option<ObjectName>,
    pub table: ObjectName,
    pub using_method: Option<String>,
    pub columns: Vec<GlobalIndexColumn>,
    pub containing: Vec<String>,
    pub distribute_by: Option<DistributeClause>,
    pub with_options: Vec<(String, String)>,
    pub tablespace: Option<String>,
    pub visible: Option<bool>,
    pub where_clause: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlobalIndexColumn {
    pub name: String,
    pub length: Option<u32>,
    pub collation: Option<String>,
    pub opclass: Option<String>,
    pub ordering: Option<IndexOrdering>,
    pub nulls: Option<IndexNulls>,
    pub expression: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndexOrdering {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IndexNulls {
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateSequenceStatement {
    pub if_not_exists: bool,
    pub name: ObjectName,
    pub start: Option<Expr>,
    pub increment: Option<Expr>,
    pub min_value: Option<Expr>,
    pub max_value: Option<Expr>,
    pub cache: Option<Expr>,
    pub cycle: bool,
    pub owned_by: Option<ObjectName>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TruncateStatement {
    pub tables: Vec<ObjectName>,
    pub cascade: bool,
    pub restart_identity: bool,
    pub continue_identity: bool,
}

// ========== CREATE TYPE ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTypeStatement {
    pub name: ObjectName,
    pub type_kind: TypeKind,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeKind {
    Composite { attributes: Vec<TypeAttribute> },
    Enum { labels: Vec<String> },
    Base { options: Vec<(String, String)> },
    Table { element_type: String },
    Range { options: Vec<(String, String)> },
    Shell,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeAttribute {
    pub name: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Statement {
    Select(Spanned<SelectStatement>),
    Insert(Spanned<InsertStatement>),
    InsertAll(Spanned<InsertAllStatement>),
    InsertFirst(Spanned<InsertFirstStatement>),
    Update(Spanned<UpdateStatement>),
    Delete(Spanned<DeleteStatement>),
    Merge(Spanned<MergeStatement>),
    CreateTable(Spanned<CreateTableStatement>),
    CreateTableAs(Spanned<CreateTableAsStatement>),
    AlterTable(Spanned<AlterTableStatement>),
    AlterTablespace(Spanned<AlterTablespaceStatement>),
    Drop(Spanned<DropStatement>),
    Truncate(Spanned<TruncateStatement>),
    CreateIndex(Spanned<CreateIndexStatement>),
    CreateGlobalIndex(Spanned<CreateGlobalIndexStatement>),
    CreateSchema(Spanned<CreateSchemaStatement>),
    CreateDatabase(Spanned<CreateDatabaseStatement>),
    CreateDatabaseLink(Spanned<CreateDatabaseLinkStatement>),
    CreateTablespace(Spanned<CreateTablespaceStatement>),
    CreateFunction(Spanned<CreateFunctionStatement>),
    CreateProcedure(Spanned<CreateProcedureStatement>),
    CreateType(Spanned<CreateTypeStatement>),
    AlterIndex(Spanned<AlterIndexStatement>),
    CreatePackage(Spanned<CreatePackageStatement>),
    CreateView(Spanned<CreateViewStatement>),
    CreateMaterializedView(Spanned<CreateMaterializedViewStatement>),
    CreateSequence(Spanned<CreateSequenceStatement>),
    CreateTrigger(Spanned<CreateTriggerStatement>),
    CreateExtension(Spanned<CreateExtensionStatement>),
    CreateRole(Spanned<CreateRoleStatement>),
    CreateUser(Spanned<CreateUserStatement>),
    CreateGroup(Spanned<CreateGroupStatement>),
    Grant(Spanned<GrantStatement>),
    Revoke(Spanned<RevokeStatement>),
    Transaction(Spanned<TransactionStatement>),
    Copy(Spanned<CopyStatement>),
    Explain(Spanned<ExplainStatement>),
    Vacuum(Spanned<VacuumStatement>),
    VariableSet(Spanned<VariableSetStatement>),
    VariableShow(Spanned<VariableShowStatement>),
    VariableReset(Spanned<VariableResetStatement>),
    Do(Spanned<DoStatement>),
    Call(Spanned<CallFuncStatement>),
    Prepare(Spanned<PrepareStatement>),
    Execute(Spanned<ExecuteStatement>),
    Deallocate(Spanned<DeallocateStatement>),
    Comment(Spanned<CommentStatement>),
    Lock(Spanned<LockStatement>),
    DeclareCursor(Spanned<DeclareCursorStatement>),
    ClosePortal(Spanned<ClosePortalStatement>),
    Fetch(Spanned<FetchStatement>),
    Checkpoint,
    Discard(Spanned<DiscardStatement>),
    Cluster(Spanned<ClusterStatement>),
    Reindex(Spanned<ReindexStatement>),
    Listen(Spanned<ListenStatement>),
    Notify(Spanned<NotifyStatement>),
    Unlisten(Spanned<UnlistenStatement>),
    Rule(Spanned<RuleStatement>),
    DropRule(Spanned<DropStatement>),
    CreateCast(Spanned<CreateCastStatement>),
    CreateConversion(Spanned<CreateConversionStatement>),
    CreateDomain(Spanned<CreateDomainStatement>),
    AlterDomain(Spanned<AlterDomainStatement>),
    CreateForeignTable(Spanned<CreateForeignTableStatement>),
    CreateForeignServer(Spanned<CreateForeignServerStatement>),
    CreateFdw(Spanned<CreateFdwStatement>),
    CreatePublication(Spanned<CreatePublicationStatement>),
    CreateSubscription(Spanned<CreateSubscriptionStatement>),
    CreateSynonym(Spanned<CreateSynonymStatement>),
    CreateModel(Spanned<CreateModelStatement>),
    CreateAm(Spanned<CreateAmStatement>),
    CreateDirectory(Spanned<CreateDirectoryStatement>),
    CreateNode(Spanned<CreateNodeStatement>),
    CreateNodeGroup(Spanned<CreateNodeGroupStatement>),
    CreateResourcePool(Spanned<CreateResourcePoolStatement>),
    CreateWorkloadGroup(Spanned<CreateWorkloadGroupStatement>),
    CreateAuditPolicy(Spanned<CreateAuditPolicyStatement>),
    CreateMaskingPolicy(Spanned<CreateMaskingPolicyStatement>),
    CreateRlsPolicy(Spanned<CreateRlsPolicyStatement>),
    CreateDataSource(Spanned<CreateDataSourceStatement>),
    CreateEvent(Spanned<CreateEventStatement>),
    CreateOpClass(Spanned<CreateOpClassStatement>),
    CreateOpFamily(Spanned<CreateOpFamilyStatement>),
    CreateContQuery(Spanned<CreateContQueryStatement>),
    CreateStream(Spanned<CreateStreamStatement>),
    CreateKey(Spanned<CreateKeyStatement>),
    CreatePackageBody(Spanned<CreatePackageBodyStatement>),
    AlterFunction(Spanned<AlterFunctionStatement>),
    AlterProcedure(Spanned<AlterProcedureStatement>),
    AlterSchema(Spanned<AlterSchemaStatement>),
    AlterDatabase(Spanned<AlterDatabaseStatement>),
    AlterRole(Spanned<AlterRoleStatement>),
    AlterUser(Spanned<AlterUserStatement>),
    AlterGroup(Spanned<AlterGroupStatement>),
    CreateAggregate(Spanned<CreateAggregateStatement>),
    CreateOperator(Spanned<CreateOperatorStatement>),
    AlterDefaultPrivileges(Spanned<AlterDefaultPrivilegesStatement>),
    CreateUserMapping(Spanned<CreateUserMappingStatement>),
    AlterUserMapping(Spanned<AlterUserMappingStatement>),
    DropUserMapping(Spanned<DropUserMappingStatement>),
    AlterSequence(Spanned<AlterSequenceStatement>),
    AlterExtension(Spanned<AlterExtensionStatement>),
    AlterCompositeType(Spanned<AlterCompositeTypeStatement>),
    AlterView(Spanned<AlterViewStatement>),
    AlterTrigger(Spanned<AlterTriggerStatement>),
    AlterForeignTable(Spanned<AlterForeignTableStatement>),
    AlterForeignServer(Spanned<AlterForeignServerStatement>),
    AlterFdw(Spanned<AlterFdwStatement>),
    AlterPublication(Spanned<AlterPublicationStatement>),
    AlterSubscription(Spanned<AlterSubscriptionStatement>),
    AlterNode(Spanned<AlterNodeStatement>),
    AlterNodeGroup(Spanned<AlterNodeGroupStatement>),
    AlterResourcePool(Spanned<AlterResourcePoolStatement>),
    AlterWorkloadGroup(Spanned<AlterWorkloadGroupStatement>),
    AlterAuditPolicy(Spanned<AlterAuditPolicyStatement>),
    AlterMaskingPolicy(Spanned<AlterMaskingPolicyStatement>),
    AlterRlsPolicy(Spanned<AlterRlsPolicyStatement>),
    AlterDataSource(Spanned<AlterDataSourceStatement>),
    AlterEvent(Spanned<AlterEventStatement>),
    AlterOpFamily(Spanned<AlterOpFamilyStatement>),
    AlterOperator(Spanned<AlterOperatorStatement>),
    AlterMaterializedView(Spanned<AlterMaterializedViewStatement>),
    AlterGlobalConfig(Spanned<AlterGlobalConfigStatement>),
    RefreshMaterializedView(Spanned<RefreshMatViewStatement>),
    Shutdown(Spanned<ShutdownStatement>),
    Barrier(Spanned<BarrierStatement>),
    Purge(Spanned<PurgeStatement>),
    TimeCapsule(Spanned<TimeCapsuleStatement>),
    Snapshot(Spanned<SnapshotStatement>),
    Shrink(Spanned<ShrinkStatement>),
    Verify(Spanned<VerifyStatement>),
    CleanConn(Spanned<CleanConnStatement>),
    Compile(Spanned<CompileStatement>),
    GetDiag(Spanned<GetDiagStatement>),
    ShowEvent(Spanned<ShowEventStatement>),
    AnonyBlock(Spanned<AnonyBlockStatement>),
    RemovePackage(Spanned<RemovePackageStatement>),
    SecLabel(Spanned<SecLabelStatement>),
    CreateWeakPasswordDictionary,
    DropWeakPasswordDictionary,
    CreatePolicyLabel(Spanned<CreatePolicyLabelStatement>),
    AlterPolicyLabel(Spanned<AlterPolicyLabelStatement>),
    DropPolicyLabel(Spanned<DropPolicyLabelStatement>),
    GrantRole(Spanned<GrantRoleStatement>),
    RevokeRole(Spanned<RevokeRoleStatement>),
    Analyze(Spanned<AnalyzeStatement>),
    Abort,
    Values(Spanned<ValuesStatement>),
    ExecuteDirect(Spanned<ExecuteDirectStatement>),
    AlterSynonym(Spanned<AlterSynonymStatement>),
    AlterTextSearchConfig(Spanned<AlterTextSearchConfigStatement>),
    AlterTextSearchDict(Spanned<AlterTextSearchDictStatement>),
    AlterCoordinator(Spanned<AlterCoordinatorStatement>),
    AlterAppWorkloadGroupMapping(Spanned<AlterAppWorkloadGroupMappingStatement>),
    AlterDatabaseLink(Spanned<AlterDatabaseLinkStatement>),
    AlterDirectory(Spanned<AlterDirectoryStatement>),
    AlterLanguage(Spanned<AlterLanguageStatement>),
    AlterLargeObject(Spanned<AlterLargeObjectStatement>),
    AlterPackage(Spanned<AlterPackageStatement>),
    AlterSession(Spanned<AlterSessionStatement>),
    AlterSystemKillSession(Spanned<AlterSystemKillSessionStatement>),
    CreateLanguage(Spanned<CreateLanguageStatement>),
    CreateWeakPasswordDictionaryWithValues(Spanned<CreateWeakPasswordDictStatement>),
    PredictBy(Spanned<PredictByStatement>),
    Replace(Spanned<InsertStatement>),
    Move(Spanned<MoveStatement>),
    LockBuckets(Spanned<LockBucketsStatement>),
    MarkBuckets(Spanned<MarkBucketsStatement>),
    SetSessionAuthorization(Spanned<SetSessionAuthorizationStatement>),
    CreateAppWorkloadGroupMapping(Spanned<CreateAppWorkloadGroupMappingStatement>),
    DropAppWorkloadGroupMapping(Spanned<DropAppWorkloadGroupMappingStatement>),
    CreateTextSearchConfig(Spanned<CreateTextSearchConfigStatement>),
    CreateTextSearchDict(Spanned<CreateTextSearchDictStatement>),
    AlterTextSearchConfigFull(Spanned<AlterTextSearchConfigFullStatement>),
    AlterTextSearchDictFull(Spanned<AlterTextSearchDictFullStatement>),
    ExpdpDatabase(Spanned<ExpdpDatabaseStatement>),
    ExpdpTable(Spanned<ExpdpTableStatement>),
    ImpdpDatabase(Spanned<ImpdpDatabaseStatement>),
    ImpdpTable(Spanned<ImpdpTableStatement>),
    ReassignOwned(Spanned<ReassignOwnedStatement>),
    Empty,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StatementInfo {
    pub sql_text: String,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
    #[serde(flatten)]
    pub statement: Statement,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GroupByItem {
    Expr(Expr),
    GroupingSets(Vec<Vec<Expr>>),
    Rollup(Vec<Expr>),
    Cube(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConnectByClause {
    pub nocycle: bool,
    pub condition: Expr,
    pub start_with: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectIntoTable {
    pub unlogged: bool,
    pub table_name: ObjectName,
}

/// Parsed optimizer hint from `/*+ ... */` comment.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct HintInfo {
    pub name: String,
    #[serde(default)]
    pub negated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queryblock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct SelectStatement {
    pub hints: Vec<HintInfo>,
    pub with: Option<WithClause>,
    pub distinct: bool,
    pub distinct_on: Vec<Expr>,
    pub targets: Vec<SelectTarget>,
    pub into_targets: Option<Vec<SelectTarget>>,
    pub bulk_collect: bool,
    pub into_table: Option<SelectIntoTable>,
    pub from: Vec<TableRef>,
    pub where_clause: Option<Expr>,
    pub connect_by: Option<ConnectByClause>,
    pub group_by: Vec<GroupByItem>,
    pub having: Option<Expr>,
    pub order_by: Vec<OrderByItem>,
    pub order_siblings: bool,
    pub limit: Option<Expr>,
    pub offset: Option<Expr>,
    pub fetch: Option<FetchClause>,
    pub lock_clause: Option<LockClause>,
    pub window_clause: Vec<NamedWindow>,
    pub set_operation: Option<SetOperation>,
    pub raw_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchClause {
    pub count: Option<Expr>,
    pub with_ties: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LockClause {
    Update { tables: Vec<ObjectName>, nowait: bool, skip_locked: bool, wait: Option<Expr> },
    Share { tables: Vec<ObjectName>, nowait: bool, skip_locked: bool, wait: Option<Expr> },
    NoKeyUpdate { tables: Vec<ObjectName>, nowait: bool, skip_locked: bool, wait: Option<Expr> },
    KeyShare { tables: Vec<ObjectName>, nowait: bool, skip_locked: bool, wait: Option<Expr> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SetOperation {
    Union { all: bool, right: Box<SelectStatement> },
    Intersect { all: bool, right: Box<SelectStatement> },
    Except { all: bool, right: Box<SelectStatement> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WithClause {
    pub recursive: bool,
    pub ctes: Vec<Cte>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cte {
    pub name: String,
    pub columns: Vec<String>,
    pub query: Box<SelectStatement>,
    pub raw_body: Option<String>,
    pub materialized: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SelectTarget {
    Expr(Expr, Option<Ident>),
    Star(Option<Ident>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TableRef {
    Table {
        name: ObjectName,
        alias: Option<Ident>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        column_aliases: Vec<String>,
        partition: Option<TablePartitionRef>,
        timecapsule: Option<Expr>,
        tablesample: Option<TableSampleClause>,
    },
    FunctionCall {
        name: ObjectName,
        args: Vec<Expr>,
        alias: Option<Ident>,
        column_defs: Vec<ColumnDef>,
        #[serde(skip_serializing_if = "Option::is_none")]
        builtin: Option<BuiltinFuncMeta>,
    },
    Subquery {
        query: Box<SelectStatement>,
        alias: Option<Ident>,
        #[serde(default)]
        lateral: bool,
    },
    Values {
        values: Box<ValuesStatement>,
        alias: Option<Ident>,
        column_names: Vec<String>,
        #[serde(default)]
        lateral: bool,
    },
    Join {
        left: Box<TableRef>,
        right: Box<TableRef>,
        join_type: JoinType,
        condition: Option<Expr>,
        natural: bool,
        using_columns: Vec<String>,
    },
    Pivot {
        source: Box<TableRef>,
        pivot: PivotClause,
    },
    Unpivot {
        source: Box<TableRef>,
        unpivot: UnpivotClause,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TablePartitionRef {
    pub for_values: Option<Vec<Expr>>,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableSampleClause {
    pub method: String,
    pub arguments: Vec<Expr>,
    pub repeatable: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PivotClause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xml: Option<bool>,
    pub aggregate: Expr,
    pub for_column: ObjectName,
    pub values: Vec<PivotValue>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PivotValue {
    pub value: Expr,
    pub alias: Option<Ident>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnpivotClause {
    pub include_nulls: Option<bool>,
    pub value_column: ObjectName,
    pub for_column: ObjectName,
    pub columns: Vec<PivotValue>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrderByItem {
    pub expr: Expr,
    pub asc: Option<bool>,
    pub nulls_first: Option<bool>,
    pub using: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScalarSublinkType {
    Any,
    Some,
    All,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SequenceFunc {
    Nextval,
    Currval,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Literal(Literal),
    ColumnRef(ObjectName),
    /// Oracle-style `(+)` outer-join marker attached to a column reference.
    /// Preserved for round-trip fidelity; the formatter re-emits `(+)` after the name.
    /// Treated semantically equivalent to [`Expr::ColumnRef`] everywhere except formatting.
    ColumnRefOuterJoin(ObjectName),
    QualifiedStar(String),
    BinaryOp {
        left: Box<Expr>,
        op: String,
        right: Box<Expr>,
    },
    Like {
        expr: Box<Expr>,
        pattern: Box<Expr>,
        escape: Option<Box<Expr>>,
        negated: bool,
        case_insensitive: bool,
    },
    UnaryOp {
        op: String,
        expr: Box<Expr>,
    },
    /// Standard function call with comma-separated arguments.
    ///
    /// Covers all user-defined functions and most built-in functions.
    /// For functions with keyword-separated syntax (e.g. `EXTRACT`, `SUBSTRING`),
    /// see [`SpecialFunction`](Expr::SpecialFunction) — downstream consumers must
    /// handle **both** variants when looking for "all function calls" in the AST.
    FunctionCall {
        name: ObjectName,
        args: Vec<Expr>,
        distinct: bool,
        over: Option<WindowSpec>,
        filter: Option<Box<Expr>>,
        within_group: Vec<OrderByItem>,
        separator: Option<Box<Expr>>,
        default: Option<Box<Expr>>,
        conversion_format: Option<Box<Expr>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        agg_from: Option<Vec<TableRef>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        builtin: Option<BuiltinFuncMeta>,
    },
    Case {
        operand: Option<Box<Expr>>,
        whens: Vec<WhenClause>,
        else_expr: Option<Box<Expr>>,
    },
    Between {
        expr: Box<Expr>,
        low: Box<Expr>,
        high: Box<Expr>,
        negated: bool,
    },
    InList {
        expr: Box<Expr>,
        list: Vec<Expr>,
        negated: bool,
    },
    InSubquery {
        expr: Box<Expr>,
        subquery: Box<SelectStatement>,
        negated: bool,
    },
    Exists(Box<SelectStatement>),
    Subquery(Box<SelectStatement>),
    ScalarSublink {
        expr: Box<Expr>,
        op: String,
        sublink_type: ScalarSublinkType,
        subquery: Box<SelectStatement>,
    },
    IsNull {
        expr: Box<Expr>,
        negated: bool,
    },
    IsBoolean {
        expr: Box<Expr>,
        value: bool,
        negated: bool,
    },
    TypeCast {
        expr: Box<Expr>,
        type_name: DataType,
        default: Option<Box<Expr>>,
        format: Option<Box<Expr>>,
    },
    Treat {
        expr: Box<Expr>,
        type_name: DataType,
    },
    CollationFor {
        expr: Box<Expr>,
    },
    Parameter(i32),
    MyBatisParam(String),
    MyBatisRawExpr(String),
    /// JDBC-style positional parameter: ?
    JdbcParam,
    Array(Vec<Expr>),
    Subscript {
        object: Box<Expr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lower: Option<Box<Expr>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upper: Option<Box<Expr>>,
        #[serde(default)]
        is_slice: bool,
    },
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    Parenthesized(Box<Expr>),
    RowConstructor(Vec<Expr>),
    Prior(Box<Expr>),
    Default,
    XmlElement {
        entity_escaping: Option<bool>,
        evalname: Option<Box<Expr>>,
        name: Option<String>,
        attributes: Option<XmlAttributes>,
        content: Vec<XmlContent>,
    },
    XmlConcat(Vec<Expr>),
    XmlForest(Vec<XmlContent>),
    XmlParse {
        option: XmlOption,
        expr: Box<Expr>,
        wellformed: bool,
    },
    XmlPi {
        name: Option<String>,
        content: Option<Box<Expr>>,
    },
    XmlRoot {
        expr: Box<Expr>,
        version: Option<Box<Expr>>,
        standalone: Option<Option<bool>>,
    },
    XmlSerialize {
        option: XmlOption,
        expr: Box<Expr>,
        type_name: DataType,
    },
    /// SQL functions that use keyword-separated syntax (or have ambiguous syntax forms)
    /// and therefore cannot be represented as a regular [`FunctionCall`].
    ///
    /// # Why SpecialFunction instead of FunctionCall?
    ///
    /// Standard function calls use comma-separated arguments: `func(a, b, c)`.
    /// Some SQL functions use **keywords** to separate arguments instead of commas,
    /// e.g. `SUBSTRING(str FROM 1 FOR 3)`, `EXTRACT(year FROM date)`. These require
    /// dedicated parsing logic and a simplified AST representation that only captures
    /// positional arguments (the keywords are implied by position).
    ///
    /// `substr` and `substring` with comma syntax (`substr('hello', 1, 3)`) produce
    /// [`FunctionCall`] instead of `SpecialFunction`. Keyword syntax
    /// (`SUBSTRING(str FROM 1 FOR 3)`) produces `SpecialFunction`.
    ///
    /// # Complete list of functions that produce SpecialFunction
    ///
    /// | Function | `name` value | Syntax forms | Notes |
    /// |----------|-------------|--------------|-------|
    /// | `SUBSTRING` / `SUBSTR` | `"substring"` or `"substr"` | `FROM`/`FOR` keywords | Comma syntax → `FunctionCall`; keyword syntax → `SpecialFunction` |
    /// | `OVERLAY` | `"overlay"` | `PLACING`/`FROM`/`FOR` keywords | |
    /// | `POSITION` | `"position"` | `IN` keyword | |
    /// | `EXTRACT` | `"extract"` | `field FROM expr` | |
    /// | `TRIM` | `"trim"` | `LEADING`/`TRAILING`/`BOTH` + `FROM` | Only when keyword syntax is used; `TRIM(expr)` becomes `FunctionCall` |
    /// | `CONVERT` | `"convert"` | `expr USING charset` | Only with `USING` keyword; `CONVERT(a, b)` becomes `FunctionCall` |
    /// | `INTERVAL` | `"interval"` | `'value' unit` | |
    /// | `CURRENT_TIME` | `"current_time"` | Optional precision `(n)` | Without parentheses → `ColumnRef` |
    /// | `CURRENT_TIMESTAMP` | `"current_timestamp"` | Optional precision `(n)` | Without parentheses → `ColumnRef` |
    /// | `LOCALTIME` | `"localtime"` | Optional precision `(n)` | Without parentheses → `ColumnRef` |
    /// | `LOCALTIMESTAMP` | `"localtimestamp"` | Optional precision `(n)` | Without parentheses → `ColumnRef` |
    ///
    /// # Downstream consumer guidance
    ///
    /// When walking the AST for "all function calls", you **must** handle both
    /// `Expr::FunctionCall` and `Expr::SpecialFunction`. For example:
    ///
    /// ```rust,ignore
    /// match expr {
    ///     Expr::FunctionCall { name, args, .. } => { /* regular function */ }
    ///     Expr::SpecialFunction { name, args, .. } => { /* special function */ }
    ///     _ => {}
    /// }
    /// ```
    ///
    /// `SpecialFunction` carries `builtin` metadata, populated from `function_registry`.
    /// It does **not** carry `over`, `filter`, or `within_group` clauses.
    SpecialFunction {
        name: String,
        args: Vec<Expr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        builtin: Option<BuiltinFuncMeta>,
    },
    /// `expr AT TIME ZONE zone` — timezone conversion expression
    AtTimeZone {
        expr: Box<Expr>,
        zone: Box<Expr>,
    },
    /// WHERE CURRENT OF cursor_name — for positioned UPDATE/DELETE
    CurrentOf {
        cursor_name: String,
    },
    /// PREDICT BY model_name (FEATURES col1, col2, ...) — openGauss ML prediction expression
    PredictBy {
        model_name: String,
        features: Vec<Expr>,
    },
    SysDate,
    SequenceValue {
        sequence: ObjectName,
        function: SequenceFunc,
    },
    /// PL/pgSQL cursor attribute: cursor_name%NOTFOUND, cursor_name%FOUND, etc.
    CursorAttribute {
        cursor: Box<Expr>,
        attribute: CursorAttributeKind,
    },
    /// PL/pgSQL variable reference
    PlVariable(ObjectName),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorAttributeKind {
    NotFound,
    Found,
    IsOpen,
    RowCount,
    BulkExceptions,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WhenClause {
    pub condition: Expr,
    pub result: Expr,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Literal {
    Integer(i64),
    Float(String),
    String(String),
    EscapeString(String),
    BitString(String),
    HexString(String),
    NationalString(String),
    DollarString { tag: Option<String>, body: String },
    Boolean(bool),
    Null,
}

pub use ident::Ident;

pub type ObjectName = Vec<Ident>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum XmlOption {
    Document,
    Content,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XmlAttributes {
    pub entity_escaping: Option<bool>,
    pub items: Vec<XmlAttribute>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XmlAttribute {
    pub value: Expr,
    pub name: Option<Ident>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XmlContent {
    pub expr: Expr,
    pub alias: Option<Ident>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowSpec {
    pub window_name: Option<String>,
    pub partition_by: Vec<Expr>,
    pub order_by: Vec<OrderByItem>,
    pub frame: Option<WindowFrame>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WindowFrameMode {
    Rows,
    Range,
    Groups,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WindowFrameDirection {
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRow,
    Preceding(i64),
    Following(i64),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowFrame {
    pub mode: WindowFrameMode,
    pub start: Option<WindowFrameBound>,
    pub end: Option<WindowFrameBound>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WindowFrameBound {
    pub direction: WindowFrameDirection,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NamedWindow {
    pub name: String,
    pub spec: WindowSpec,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DmlPartitionClause {
    Partition(Vec<String>),
    Subpartition(Vec<String>),
    PartitionFor(Vec<Expr>),
    SubpartitionFor(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertStatement {
    pub hints: Vec<HintInfo>,
    pub with: Option<WithClause>,
    pub table: ObjectName,
    pub alias: Option<Ident>,
    pub partition: Option<DmlPartitionClause>,
    pub columns: Vec<String>,
    pub source: InsertSource,
    pub on_duplicate_key: Option<OnDuplicateKeyUpdate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub on_conflict: Option<OnConflict>,
    pub returning: Vec<SelectTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub into_targets: Option<Vec<SelectTarget>>,
    #[serde(default)]
    pub bulk_collect: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OnDuplicateKeyUpdate {
    Nothing,
    Update { assignments: Vec<UpdateAssignment>, where_clause: Option<Expr> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OnConflict {
    pub target: Option<ConflictTarget>,
    pub action: ConflictAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConflictTarget {
    Columns(Vec<String>),
    OnConstraint(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConflictAction {
    DoNothing,
    DoUpdate { assignments: Vec<UpdateAssignment>, where_clause: Option<Expr> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum InsertSource {
    Values(Vec<Vec<Expr>>),
    Select(Box<SelectStatement>),
    DefaultValues,
    Set(Vec<UpdateAssignment>),
    /// INSERT INTO t VALUES record_variable — GaussDB/openGauss Oracle-compatible
    /// syntax where a %ROWTYPE or RECORD variable is used without parentheses.
    RecordVariable(Expr),
}

// INSERT ALL / INSERT FIRST multi-table insert types

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertAllTarget {
    pub table: ObjectName,
    pub columns: Vec<String>,
    pub values: Vec<Vec<Expr>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertAllCondition {
    pub condition: Expr,
    pub targets: Vec<InsertAllTarget>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertAllStatement {
    pub targets: Vec<InsertAllTarget>,
    pub conditions: Vec<InsertAllCondition>,
    pub else_targets: Vec<InsertAllTarget>,
    pub source: Box<SelectStatement>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InsertFirstStatement {
    pub when_clauses: Vec<InsertAllCondition>,
    pub else_targets: Vec<InsertAllTarget>,
    pub source: Box<SelectStatement>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateStatement {
    pub hints: Vec<HintInfo>,
    pub with: Option<WithClause>,
    pub tables: Vec<TableRef>,
    pub partition: Option<DmlPartitionClause>,
    pub assignments: Vec<UpdateAssignment>,
    pub from: Vec<TableRef>,
    pub where_clause: Option<Expr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub order_by: Option<Vec<OrderByItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub limit: Option<Expr>,
    pub returning: Vec<SelectTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub into_targets: Option<Vec<SelectTarget>>,
    #[serde(default)]
    pub bulk_collect: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateAssignment {
    /// Column targets. Single assignment: `columns: vec![vec!["col"]]`.
    /// Multi-column: `SET (a, b, c) = (...)` → `columns: vec![vec!["a"], vec!["b"], vec!["c"]]`.
    pub columns: Vec<ObjectName>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteStatement {
    pub hints: Vec<HintInfo>,
    pub with: Option<WithClause>,
    pub tables: Vec<TableRef>,
    pub using: Vec<TableRef>,
    pub where_clause: Option<Expr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub order_by: Option<Vec<OrderByItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub limit: Option<Expr>,
    pub returning: Vec<SelectTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub into_targets: Option<Vec<SelectTarget>>,
    #[serde(default)]
    pub bulk_collect: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeStatement {
    pub hints: Vec<HintInfo>,
    pub target: TableRef,
    pub partition: Option<DmlPartitionClause>,
    pub source: TableRef,
    pub source_partition: Option<DmlPartitionClause>,
    pub on_condition: Expr,
    pub when_clauses: Vec<MergeWhenClause>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeWhenClause {
    pub matched: bool,
    pub action: MergeAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MergeAction {
    Update(Vec<UpdateAssignment>),
    Delete,
    Insert { columns: Vec<ObjectName>, values: Vec<Expr> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TransactionStatement {
    pub kind: TransactionKind,
    pub modes: Vec<TransactionMode>,
    pub savepoint_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransactionKind {
    Begin,
    Commit,
    Rollback,
    Savepoint,
    ReleaseSavepoint,
    PrepareTransaction,
    CommitPrepared,
    RollbackPrepared,
    SetTransaction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransactionMode {
    IsolationLevel(IsolationLevel),
    ReadOnly,
    ReadWrite,
    Deferrable,
    NotDeferrable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariableSetStatement {
    pub local: bool,
    pub session: bool,
    pub global: bool,
    pub name: String,
    pub value: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariableShowStatement {
    pub name: String,
    pub like_pattern: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VariableResetStatement {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiscardStatement {
    pub target: DiscardTarget,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiscardTarget {
    All,
    Plans,
    Sequences,
    Temp,
}

// ── COPY statement ──

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CopyOption {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CopyStatement {
    pub relation: Option<ObjectName>,
    pub query: Option<SelectStatement>,
    pub columns: Vec<String>,
    pub is_from: bool,
    pub filename: Option<String>,
    pub is_program: bool,
    pub options: Vec<CopyOption>,
}

// ── EXPLAIN statement ──

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExplainOption {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExplainStatement {
    pub analyze: bool,
    pub verbose: bool,
    pub performance: bool,
    pub plan: bool,
    pub statement_id: Option<String>,
    pub options: Vec<ExplainOption>,
    pub query: Box<Statement>,
}

// ── CALL statement ──

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallArg {
    Positional(Expr),
    Named { name: String, arg: Expr, uses_arrow: bool },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CallFuncStatement {
    pub func_name: ObjectName,
    pub args: Vec<CallArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinFuncMeta>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateViewStatement {
    pub replace: bool,
    pub temporary: bool,
    pub recursive: bool,
    pub name: ObjectName,
    pub columns: Vec<String>,
    pub query: Box<SelectStatement>,
    pub check_option: Option<CheckOption>,
    pub security: Option<ViewSecurity>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CheckOption {
    Local,
    Cascaded,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ViewSecurity {
    Barrier,
    Invoker,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateSchemaStatement {
    pub if_not_exists: bool,
    pub name: Option<String>,
    pub authorization: Option<String>,
    pub character_set: Option<String>,
    pub collate: Option<String>,
    pub elements: Vec<SchemaElement>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SchemaElement {
    Table(CreateTableStatement),
    Index(CreateIndexStatement),
    View(CreateViewStatement),
    Sequence(CreateSequenceStatement),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDatabaseStatement {
    pub name: String,
    pub owner: Option<String>,
    pub template: Option<String>,
    pub encoding: Option<String>,
    pub locale: Option<String>,
    pub lc_collate: Option<String>,
    pub lc_ctype: Option<String>,
    pub tablespace: Option<String>,
    pub allow_connections: Option<bool>,
    pub connection_limit: Option<i32>,
    pub is_template: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTablespaceStatement {
    pub name: String,
    pub owner: Option<String>,
    pub relative: bool,
    pub location: String,
    pub maxsize: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDatabaseLinkStatement {
    pub name: String,
    pub public_link: bool,
    pub user: Option<String>,
    pub password: Option<String>,
    pub using_clause: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnonyBlockStatement {
    pub block: crate::ast::plpgsql::PlBlock,
}

pub mod visitor;

macro_rules! stub_struct {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
            pub struct $name { pub _stub: () }
        )+
    };
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateForeignTableStatement {
    pub name: ObjectName,
    pub columns: Vec<ColumnDef>,
    pub server_name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateForeignServerStatement {
    pub name: String,
    pub server_type: Option<String>,
    pub version: Option<String>,
    pub fdw_name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateFdwStatement {
    pub name: String,
    pub handler: Option<String>,
    pub validator: Option<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreatePublicationStatement {
    pub name: String,
    pub tables: Vec<ObjectName>,
    pub all_tables: bool,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateSubscriptionStatement {
    pub name: String,
    pub connection: String,
    pub publications: Vec<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateNodeStatement {
    pub name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateNodeGroupStatement {
    pub name: String,
    pub nodes: Vec<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateResourcePoolStatement {
    pub name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateWorkloadGroupStatement {
    pub name: String,
    pub pool_name: Option<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateAuditPolicyStatement {
    pub name: String,
    pub policy_type: String,
    pub privileges: Vec<String>,
    pub labels: Vec<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FilterClause {
    pub kind: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateMaskingPolicyStatement {
    pub name: String,
    pub masking_function: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub function_args: Vec<Expr>,
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_clauses: Vec<FilterClause>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateRlsPolicyStatement {
    pub name: String,
    pub table: ObjectName,
    pub permissive: bool,
    pub using_expr: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RoutineParam {
    pub name: String,
    pub mode: Option<String>,
    pub data_type: String,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FunctionOptions {
    pub language: Option<String>,
    pub volatility: Option<Volatility>,
    pub strict: Option<bool>,
    pub cost: Option<u32>,
    pub rows: Option<u32>,
    pub leakproof: Option<bool>,
    pub security: Option<SecurityMode>,
    pub parallel: Option<ParallelMode>,
    pub fenced: Option<bool>,
    pub shippable: Option<bool>,
    pub extra: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Volatility {
    Immutable,
    Stable,
    Volatile,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SecurityMode {
    Invoker,
    Definer,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ParallelMode {
    Safe,
    Unsafe,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateFunctionStatement {
    pub replace: bool,
    pub name: ObjectName,
    pub parameters: Vec<RoutineParam>,
    pub return_type: Option<String>,
    pub options: FunctionOptions,
    pub block: Option<crate::ast::plpgsql::PlBlock>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateProcedureStatement {
    pub replace: bool,
    pub name: ObjectName,
    pub parameters: Vec<RoutineParam>,
    pub options: FunctionOptions,
    pub block: Option<crate::ast::plpgsql::PlBlock>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageAuthid {
    CurrentUser,
    Definer,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreatePackageStatement {
    pub replace: bool,
    pub name: ObjectName,
    pub authid: Option<PackageAuthid>,
    pub items: Vec<PackageItem>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreatePackageBodyStatement {
    pub replace: bool,
    pub name: ObjectName,
    pub items: Vec<PackageItem>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PackageItem {
    Procedure(PackageProcedure),
    Function(PackageFunction),
    Variable(crate::ast::plpgsql::PlVarDecl),
    Type(crate::ast::plpgsql::PlTypeDecl),
    Cursor(crate::ast::plpgsql::PlCursorDecl),
    Raw(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageProcedure {
    pub name: ObjectName,
    pub parameters: Vec<RoutineParam>,
    pub block: Option<crate::ast::plpgsql::PlBlock>,
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub end_line: usize,
}

impl PartialEq for PackageProcedure {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.parameters == other.parameters && self.block == other.block
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageFunction {
    pub name: ObjectName,
    pub parameters: Vec<RoutineParam>,
    pub return_type: Option<String>,
    pub block: Option<crate::ast::plpgsql::PlBlock>,
    #[serde(default)]
    pub start_line: usize,
    #[serde(default)]
    pub end_line: usize,
}

impl PartialEq for PackageFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.parameters == other.parameters
            && self.return_type == other.return_type
            && self.block == other.block
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateExtensionStatement {
    pub replace: bool,
    pub if_not_exists: bool,
    pub name: String,
    pub schema: Option<String>,
    pub version: Option<String>,
    pub cascade: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDomainStatement {
    pub name: ObjectName,
    pub data_type: DataType,
    pub default_value: Option<Expr>,
    pub not_null: bool,
    pub check: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CastMethod {
    WithFunction(String),
    WithoutFunction,
    WithInout,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CastContext {
    Implicit,
    Assignment,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateCastStatement {
    pub source_type: DataType,
    pub target_type: DataType,
    pub method: CastMethod,
    pub context: Option<CastContext>,
}

// ========== Wave 6: GRANT / REVOKE ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GrantStatement {
    pub privileges: Vec<Privilege>,
    pub target: GrantTarget,
    pub grantees: Vec<String>,
    pub with_grant_option: bool,
    pub granted_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Privilege {
    All,
    Select,
    SelectColumns(Vec<String>),
    Insert,
    Update,
    UpdateColumns(Vec<String>),
    Delete,
    Usage,
    Create,
    Connect,
    Temporary,
    Execute,
    Trigger,
    References,
    Alter,
    Drop,
    Comment,
    Index,
    Vacuum,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum GrantTarget {
    Table(Vec<ObjectName>),
    Schema(Vec<String>),
    Database(Vec<String>),
    Function(Vec<ObjectName>),
    Sequence(Vec<ObjectName>),
    Tablespace(Vec<String>),
    AllTablesInSchema(Vec<String>),
    AllFunctionsInSchema(Vec<String>),
    AllSequencesInSchema(Vec<String>),
    Language(Vec<String>),
    LargeObject(Vec<String>),
    Type(Vec<ObjectName>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RevokeStatement {
    pub privileges: Vec<Privilege>,
    pub target: GrantTarget,
    pub grantees: Vec<String>,
    pub cascade: bool,
    pub granted_by: Option<String>,
}

// ========== Wave 8: CREATE TRIGGER + MATERIALIZED VIEW ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTriggerStatement {
    pub name: String,
    pub or_replace: bool,
    pub constraint: bool,
    pub timing: TriggerTiming,
    pub table: ObjectName,
    pub events: Vec<TriggerEvent>,
    pub for_each: TriggerForEach,
    pub when: Option<Expr>,
    pub func_name: ObjectName,
    pub func_args: Vec<Expr>,
    pub execute_kind: ExecuteKind,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    UpdateOf(Vec<String>),
    Delete,
    Truncate,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ExecuteKind {
    Function,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TriggerForEach {
    Row,
    Statement,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateMaterializedViewStatement {
    pub if_not_exists: bool,
    pub name: ObjectName,
    pub columns: Vec<String>,
    pub query: Box<SelectStatement>,
    pub tablespace: Option<String>,
    pub with_data: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RefreshMatViewStatement {
    pub concurrent: bool,
    pub name: ObjectName,
}

// ========== Wave 9: VACUUM / ANALYZE / COMMENT ON / LOCK TABLE ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VacuumStatement {
    pub full: bool,
    pub verbose: bool,
    pub analyze: bool,
    pub freeze: bool,
    pub tables: Vec<VacuumTarget>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VacuumTarget {
    pub name: ObjectName,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnalyzeStatement {
    pub verbose: bool,
    pub tables: Vec<VacuumTarget>,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommentStatement {
    pub object_type: String,
    pub name: ObjectName,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LockStatement {
    pub tables: Vec<ObjectName>,
    pub mode: String,
    pub nowait: bool,
}

// ========== Wave 10: PREPARE / EXECUTE / DEALLOCATE / DO ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrepareStatement {
    pub name: String,
    pub data_types: Vec<String>,
    pub statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_statement: Option<Box<Statement>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecuteStatement {
    pub name: String,
    pub params: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecuteDirectStatement {
    pub node_name: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ValuesStatement {
    pub rows: Vec<Vec<Expr>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub order_by: Vec<OrderByItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<Expr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeallocateStatement {
    pub name: Option<String>,
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DoStatement {
    pub language: Option<String>,
    pub code: String,
    pub block: Option<crate::ast::plpgsql::PlBlock>,
}

// ========== Wave 11: ALTER DATABASE/SCHEMA/SEQUENCE/FUNCTION/ROLE/USER/SYSTEM ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDatabaseStatement {
    pub name: String,
    pub action: AlterDatabaseAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterDatabaseAction {
    Set { parameter: String, value: String },
    Reset { parameter: String },
    RenameTo { new_name: String },
    OwnerTo { owner: String },
    WithConnectionLimit { limit: i64 },
    EnablePrivateObject,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSchemaStatement {
    pub name: String,
    pub action: AlterSchemaAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterSchemaAction {
    RenameTo { new_name: String },
    OwnerTo { owner: String },
    CharacterSet { charset: String, collate: Option<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSequenceStatement {
    pub name: ObjectName,
    pub options: Vec<SequenceOption>,
    pub is_large: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SequenceOption {
    IncrementBy(i64),
    MinValue(Option<i64>),
    MaxValue(Option<i64>),
    StartWith(i64),
    Restart(bool),
    Cache(i64),
    Cycle(bool),
    NoCycle,
    OwnedBy { owner: ObjectName },
    OwnerTo { owner: ObjectName },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterFunctionStatement {
    pub name: ObjectName,
    pub action: AlterFunctionAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterFunctionAction {
    RenameTo { new_name: String },
    OwnerTo { owner: String },
    SetSchema { schema: String },
    Set { parameter: String, value: String },
    Reset { parameter: String },
    Compile,
    Immutable,
    Stable,
    Volatile,
    Leakproof { not: bool },
    Strict,
    CalledOnNullInput,
    ReturnsNullOnNullInput,
    Shippable { not: bool },
    Package { not: bool },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterProcedureStatement {
    pub name: ObjectName,
    pub action: AlterFunctionAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterRoleStatement {
    pub name: String,
    pub options: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterUserStatement {
    pub name: String,
    pub options: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterGroupStatement {
    pub name: String,
    pub action: AlterGroupAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterGroupAction {
    AddUser(String),
    DropUser(String),
    RenameTo(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterGlobalConfigStatement {
    pub action: AlterGlobalConfigAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterGlobalConfigAction {
    Set { parameter: String, value: String },
    Reset { parameter: String },
}

// ========== Wave 12: CURSOR / LISTEN / NOTIFY / RULE / CLUSTER / REINDEX ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorSensitivity {
    Sensitive,
    Insensitive,
    Asensitive,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorScrollability {
    Default,
    Scroll,
    NoScroll,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorHoldability {
    Default,
    WithHold,
    WithoutHold,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorReturnability {
    Default,
    WithReturn,
    WithoutReturn,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CursorReturnTo {
    Default,
    ToCaller,
    ToClient,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeclareCursorStatement {
    pub name: String,
    pub binary: bool,
    pub sensitivity: CursorSensitivity,
    pub scrollability: CursorScrollability,
    pub holdability: CursorHoldability,
    pub returnability: CursorReturnability,
    pub return_to: CursorReturnTo,
    pub query: Box<SelectStatement>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FetchStatement {
    pub direction: FetchDirection,
    pub cursor_name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FetchDirection {
    Next,
    Prior,
    First,
    Last,
    Absolute(i64),
    Relative(i64),
    Forward,
    ForwardCount(i64),
    ForwardAll,
    Backward,
    BackwardCount(i64),
    BackwardAll,
    Count(i64),
    All,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CloseTarget {
    Name(String),
    All,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClosePortalStatement {
    pub target: CloseTarget,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ListenStatement {
    pub channel: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NotifyStatement {
    pub channel: String,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnlistenStatement {
    pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuleStatement {
    pub name: String,
    pub table: ObjectName,
    pub event: RuleEvent,
    pub condition: Option<Expr>,
    pub instead: bool,
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_actions: Option<Vec<Statement>>,
}

use std::fmt;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RuleEvent {
    Select,
    Insert,
    Update,
    Delete,
}

impl fmt::Display for RuleEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleEvent::Select => write!(f, "SELECT"),
            RuleEvent::Insert => write!(f, "INSERT"),
            RuleEvent::Update => write!(f, "UPDATE"),
            RuleEvent::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClusterStatement {
    pub table: Option<ObjectName>,
    pub verbose: bool,
    pub using_index: Option<String>,
    pub partition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReindexStatement {
    pub target: ReindexTarget,
    pub verbose: bool,
    pub concurrent: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ReindexTarget {
    Table(ObjectName),
    Index(ObjectName),
    Schema(String),
    Database(String),
    System,
}

// ========== CREATE TYPE ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterIndexStatement {
    pub if_exists: bool,
    pub name: ObjectName,
    pub action: AlterIndexAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterIndexAction {
    RenameTo(String),
    RenamePartition { old_name: String, new_name: String },
    SetTablespace(String),
    Set(Vec<(String, String)>),
    Reset(Vec<String>),
    Unusable,
    Rebuild,
    MovePartition { partition_name: String, tablespace: Option<String> },
    RebuildPartition { partition_name: String },
    NoOp,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterTypeAction {
    AddAttribute { name: String, data_type: String, cascade: bool },
    DropAttribute { name: String, if_exists: bool, cascade: bool },
    RenameAttribute { old_name: String, new_name: String, cascade: bool },
    RenameTo(String),
    SetSchema(String),
    OwnerTo(String),
    AddEnumValue { if_not_exists: bool, value: String, before: Option<String>, after: Option<String> },
    RenameEnumValue { old_value: String, new_value: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterCompositeTypeStatement {
    pub name: ObjectName,
    pub action: AlterTypeAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterViewAction {
    RenameTo(String),
    Set(Vec<(String, String)>),
    Reset(Vec<String>),
    SetSchema(String),
    OwnerTo(String),
    AlterColumnDefault { column: String, set_default: Option<String> },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterViewStatement {
    pub name: ObjectName,
    pub action: AlterViewAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTriggerStatement {
    pub name: String,
    pub table: Option<ObjectName>,
    pub new_name: Option<String>,
    pub enable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterExtensionStatement {
    pub name: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateAggregateStatement {
    pub name: String,
    pub base_types: Vec<DataType>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateOperatorStatement {
    pub name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDefaultPrivilegesStatement {
    pub role: Option<String>,
    pub schema: Option<String>,
    pub action: DefaultPrivilegeAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DefaultPrivilegeAction {
    Grant(GrantStatement),
    Revoke(RevokeStatement),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateUserMappingStatement {
    pub if_not_exists: bool,
    pub user_name: String,
    pub server: ObjectName,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterUserMappingStatement {
    pub user_name: String,
    pub server: ObjectName,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DropUserMappingStatement {
    pub if_exists: bool,
    pub user_name: String,
    pub server: ObjectName,
}

// ========== Remaining stubs (not yet implemented) ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RoleOption {
    Superuser(bool),
    CreateDb(bool),
    CreateRole(bool),
    Inherit(bool),
    Login(bool),
    Replication(bool),
    BypassRls(bool),
    ConnectionLimit(i64),
    EncryptedPassword(String),
    UnencryptedPassword(String),
    ValidUntil(String),
    InRole(Vec<String>),
    Role(Vec<String>),
    Admin(Vec<String>),
    User(Vec<String>),
    Sysid(i64),
    DefaultTablespace(String),
    Tablespace(String),
    Profile(String),
    AccountLock(bool),
    AuditAdmin(bool),
    MonAdmin(bool),
    OprAdmin(bool),
    PolAdmin(bool),
    Persistence(bool),
    Independent(bool),
    Useft(bool),
    VcAdmin(bool),
    Permit(bool),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateRoleStatement {
    pub name: String,
    pub options: Vec<RoleOption>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateUserStatement {
    pub name: String,
    pub options: Vec<RoleOption>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateGroupStatement {
    pub name: String,
    pub options: Vec<RoleOption>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterResourcePoolStatement {
    pub name: String,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateConversionStatement {
    pub name: String,
    pub source_encoding: String,
    pub dest_encoding: String,
    pub function_name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateSynonymStatement {
    pub replace: bool,
    pub name: ObjectName,
    pub target: ObjectName,
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateModelStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateAmStatement {
    pub name: String,
    pub method: String,
    pub handler: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDirectoryStatement {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateDataSourceStatement {
    pub name: String,
    pub ds_type: Option<String>,
    pub version: Option<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateEventStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateOpClassStatement {
    pub name: String,
    pub method: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateOpFamilyStatement {
    pub name: String,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateContQueryStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateStreamStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateKeyStatement {
    pub raw_rest: String,
}

// ========== Real implementations for ALTER stubs ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterForeignTableStatement {
    pub name: ObjectName,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterForeignServerStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterFdwStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterPublicationStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSubscriptionStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterNodeStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterNodeGroupStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterWorkloadGroupStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterAuditPolicyStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterRlsPolicyStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDataSourceStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterEventStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterOpFamilyStatement {
    pub name: String,
    pub method: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterOperatorStatement {
    pub name: String,
    pub left_type: String,
    pub right_type: Option<String>,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterMaterializedViewStatement {
    pub name: ObjectName,
    pub raw_rest: String,
}

// ========== ALTER SYNONYM ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSynonymStatement {
    pub name: String,
    pub action: AlterSynonymAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterSynonymAction {
    Compile { debug: bool },
    OwnerTo { new_owner: String },
}

// ========== ALTER TEXT SEARCH CONFIGURATION ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTextSearchConfigStatement {
    pub name: ObjectName,
    pub raw_rest: String,
}

// ========== ALTER TEXT SEARCH DICTIONARY ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTextSearchDictStatement {
    pub name: ObjectName,
    pub options: Vec<(String, String)>,
}

// ========== ALTER COORDINATOR ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterCoordinatorStatement {
    pub name: String,
    pub raw_rest: String,
}

// ========== ALTER APP WORKLOAD GROUP MAPPING ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterAppWorkloadGroupMappingStatement {
    pub name: String,
    pub raw_rest: String,
}

// ========== Remaining stubs ==========

stub_struct!(
    DropDatabaseStatement,
    DropTablespaceStatement,
    DropRuleStatement,
    GetDiagStatement,
    ShowEventStatement,
    RemovePackageStatement,
    DropPolicyLabelStatement,
);

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDomainStatement {
    pub name: ObjectName,
    pub action: AlterDomainAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterDomainAction {
    SetDefault { expr: String },
    DropDefault,
    SetNotNull,
    DropNotNull,
    AddConstraint { name: Option<String>, check_expr: String },
    DropConstraint { name: String, cascade: bool },
    OwnerTo { new_owner: String },
    RenameTo { new_name: String },
    ValidateConstraint { name: String },
}

// ========== Real implementations for 10 utility statements ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShutdownStatement {
    pub mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BarrierStatement {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PurgeTarget {
    Table { name: ObjectName },
    Index { name: ObjectName },
    RecycleBin,
    Snapshot { name: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PurgeStatement {
    pub target: PurgeTarget,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotStatement {
    pub name: Option<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimeCapsuleStatement {
    pub table_name: ObjectName,
    pub action: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShrinkStatement {
    pub target: Option<String>,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerifyStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompileStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CleanConnStatement {
    pub force: bool,
    pub for_database: Option<String>,
    pub to_user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SecLabelStatement {
    pub object_type: String,
    pub name: ObjectName,
    pub provider: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreatePolicyLabelStatement {
    pub if_not_exists: bool,
    pub name: String,
    pub add: bool,
    pub label_type: String,
    pub targets: Vec<ObjectName>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterPolicyLabelStatement {
    pub name: String,
    pub add: bool,
    pub label_type: String,
    pub targets: Vec<ObjectName>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterMaskingPolicyAction {
    Comments(String),
    Add { function: String, labels: Vec<String> },
    Remove { function: String, labels: Vec<String> },
    Modify { function: String, labels: Vec<String> },
    ModifyFilter { filter_clauses: Vec<FilterClause> },
    DropFilter,
    Disable,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterMaskingPolicyStatement {
    pub name: String,
    pub action: AlterMaskingPolicyAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GrantRoleStatement {
    pub roles: Vec<String>,
    pub grantees: Vec<String>,
    pub with_admin_option: bool,
    pub granted_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RevokeRoleStatement {
    pub roles: Vec<String>,
    pub grantees: Vec<String>,
    pub granted_by: Option<String>,
    pub cascade: bool,
}

// ========== ALTER DATABASE LINK / DIRECTORY / LANGUAGE / LARGE OBJECT / PACKAGE / SESSION / SYSTEM KILL SESSION ==========

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDatabaseLinkStatement {
    pub name: String,
    pub action: AlterDatabaseLinkAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterDatabaseLinkAction {
    ConnectTo { user: String, password: String, connect_string: Option<String> },
    RenameTo { new_name: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterDirectoryStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterLanguageStatement {
    pub name: String,
    pub action: AlterLanguageAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterLanguageAction {
    RenameTo(String),
    OwnerTo(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterLargeObjectStatement {
    pub oid: String,
    pub new_owner: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterPackageStatement {
    pub name: String,
    pub action: AlterPackageAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterPackageAction {
    Compile { debug: bool, reuse_settings: bool },
    OwnerTo(String),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSessionStatement {
    pub action: AlterSessionAction,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AlterSessionAction {
    Set { parameter: String, value: String },
    CloseDatabaseLink { name: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterSystemKillSessionStatement {
    pub session_id: String,
    pub immediate: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateLanguageStatement {
    pub name: String,
    pub trusted: bool,
    pub handler: Option<String>,
    pub inline_func: Option<String>,
    pub validator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateWeakPasswordDictStatement {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PredictByStatement {
    pub model: String,
    pub features: Vec<String>,
    pub using_clause: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MoveStatement {
    pub direction: FetchDirection,
    pub cursor_name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LockBucketsStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MarkBucketsStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SetSessionAuthorizationStatement {
    pub user: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateAppWorkloadGroupMappingStatement {
    pub name: String,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DropAppWorkloadGroupMappingStatement {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTextSearchConfigStatement {
    pub name: ObjectName,
    pub parser_name: Option<ObjectName>,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateTextSearchDictStatement {
    pub name: ObjectName,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTextSearchConfigFullStatement {
    pub name: ObjectName,
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlterTextSearchDictFullStatement {
    pub name: ObjectName,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExpdpDatabaseStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExpdpTableStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImpdpDatabaseStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImpdpTableStatement {
    pub raw_rest: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReassignOwnedStatement {
    pub old_role: String,
    pub new_role: String,
}
