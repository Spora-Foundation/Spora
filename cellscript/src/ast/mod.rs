//! CellScript 抽象语法树 (AST)

use crate::error::Span;

/// 模块声明
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub items: Vec<Item>,
    pub span: Span,
}

/// 模块项
#[derive(Debug, Clone)]
pub enum Item {
    Resource(ResourceDef),
    Shared(SharedDef),
    Receipt(ReceiptDef),
    Struct(StructDef),
    Const(ConstDef),
    Enum(EnumDef),
    Action(ActionDef),
    Function(FnDef),
    Lock(LockDef),
    Use(UseStmt),
}

/// Resource 定义
#[derive(Debug, Clone)]
pub struct ResourceDef {
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// Shared 定义
#[derive(Debug, Clone)]
pub struct SharedDef {
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// Receipt 定义
#[derive(Debug, Clone)]
pub struct ReceiptDef {
    pub name: String,
    pub claim_output: Option<Type>,
    pub lifecycle: Option<Lifecycle>,
    pub capabilities: Vec<Capability>,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// Struct 定义
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

/// 常量定义
#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
    pub span: Span,
}

/// 枚举定义
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// 枚举变体
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
    pub span: Span,
}

/// 生命周期定义
#[derive(Debug, Clone)]
pub struct Lifecycle {
    pub states: Vec<String>,
    pub span: Span,
}

/// 能力
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Store,
    Transfer,
    Destroy,
}

/// 字段
#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// Action 定义
#[derive(Debug, Clone)]
pub struct ActionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub effect: EffectClass,
    pub effect_declared: bool,
    pub scheduler_hint: Option<SchedulerHint>,
    pub doc_comment: Option<String>,
    pub span: Span,
}

/// Function 定义。`fn` 是纯计算 helper，不是状态转换入口。
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub doc_comment: Option<String>,
    pub span: Span,
}

/// Lock 定义
#[derive(Debug, Clone)]
pub struct LockDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Use 语句
#[derive(Debug, Clone)]
pub struct UseStmt {
    pub module_path: Vec<String>,
    pub imports: Vec<UseImport>,
    pub span: Span,
}

/// Use 导入项
#[derive(Debug, Clone)]
pub struct UseImport {
    pub name: String,
    pub alias: Option<String>,
}

/// 参数
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub is_mut: bool,
    pub is_ref: bool,
    pub span: Span,
}

/// 类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    U8,
    U16,
    U32,
    U64,
    U128,
    Bool,
    Unit,
    Address,
    Hash,
    Array(Box<Type>, usize),
    Tuple(Vec<Type>),
    Named(String),
    Ref(Box<Type>),
    MutRef(Box<Type>),
}

/// 语句
#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetStmt),
    Expr(Expr),
    Return(Option<Expr>),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
}

/// 绑定模式
#[derive(Debug, Clone)]
pub enum BindingPattern {
    Name(String),
    Tuple(Vec<BindingPattern>),
    Wildcard,
}

/// Let 语句
#[derive(Debug, Clone)]
pub struct LetStmt {
    pub pattern: BindingPattern,
    pub ty: Option<Type>,
    pub value: Expr,
    pub is_mut: bool,
    pub is_ephemeral: bool,
    pub span: Span,
}

/// If 语句
#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: Vec<Stmt>,
    pub else_branch: Option<Vec<Stmt>>,
    pub span: Span,
}

/// For 语句
#[derive(Debug, Clone)]
pub struct ForStmt {
    pub pattern: BindingPattern,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// While 语句
#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// 表达式
#[derive(Debug, Clone)]
pub enum Expr {
    Integer(u64),
    Bool(bool),
    String(String),
    ByteString(Vec<u8>),
    Identifier(String),
    Assign(AssignExpr),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    FieldAccess(FieldAccessExpr),
    Index(IndexExpr),
    Create(CreateExpr),
    Consume(ConsumeExpr),
    Transfer(TransferExpr),
    Destroy(DestroyExpr),
    ReadRef(ReadRefExpr),
    Claim(ClaimExpr),
    Settle(SettleExpr),
    Assert(AssertExpr),
    Block(Vec<Stmt>),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    If(IfExpr),
    Cast(CastExpr),
    Range(RangeExpr),
    StructInit(StructInitExpr),
    Match(MatchExpr),
}

/// 赋值表达式
#[derive(Debug, Clone)]
pub struct AssignExpr {
    pub target: Box<Expr>,
    pub op: AssignOp,
    pub value: Box<Expr>,
    pub span: Span,
}

/// 赋值运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
}

/// 二元表达式
#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
    pub span: Span,
}

/// 二元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// 一元表达式
#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
    pub span: Span,
}

/// 一元运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Ref,
    Deref,
}

/// 调用表达式
#[derive(Debug, Clone)]
pub struct CallExpr {
    pub func: Box<Expr>,
    pub args: Vec<Expr>,
    pub span: Span,
}

/// 字段访问表达式
#[derive(Debug, Clone)]
pub struct FieldAccessExpr {
    pub expr: Box<Expr>,
    pub field: String,
    pub span: Span,
}

/// 索引表达式
#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub expr: Box<Expr>,
    pub index: Box<Expr>,
    pub span: Span,
}

/// Create 表达式
#[derive(Debug, Clone)]
pub struct CreateExpr {
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub lock: Option<Box<Expr>>,
    pub span: Span,
}

/// Consume 表达式
#[derive(Debug, Clone)]
pub struct ConsumeExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Transfer 表达式
#[derive(Debug, Clone)]
pub struct TransferExpr {
    pub expr: Box<Expr>,
    pub to: Box<Expr>,
    pub span: Span,
}

/// Destroy 表达式
#[derive(Debug, Clone)]
pub struct DestroyExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

/// ReadRef 表达式
#[derive(Debug, Clone)]
pub struct ReadRefExpr {
    pub ty: String,
    pub span: Span,
}

/// Claim 表达式
#[derive(Debug, Clone)]
pub struct ClaimExpr {
    pub receipt: Box<Expr>,
    pub span: Span,
}

/// Settle 表达式
#[derive(Debug, Clone)]
pub struct SettleExpr {
    pub expr: Box<Expr>,
    pub span: Span,
}

/// Assert / assert_invariant expression
#[derive(Debug, Clone)]
pub struct AssertExpr {
    pub condition: Box<Expr>,
    pub message: Box<Expr>,
    pub span: Span,
}

/// If 表达式
#[derive(Debug, Clone)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_branch: Box<Expr>,
    pub else_branch: Box<Expr>,
    pub span: Span,
}

/// 类型转换表达式
#[derive(Debug, Clone)]
pub struct CastExpr {
    pub expr: Box<Expr>,
    pub ty: Type,
    pub span: Span,
}

/// 区间表达式
#[derive(Debug, Clone)]
pub struct RangeExpr {
    pub start: Box<Expr>,
    pub end: Box<Expr>,
    pub span: Span,
}

/// Struct 初始化表达式
#[derive(Debug, Clone)]
pub struct StructInitExpr {
    pub ty: String,
    pub fields: Vec<(String, Expr)>,
    pub span: Span,
}

/// Match 表达式
#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub expr: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

/// Match 分支
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub value: Expr,
    pub span: Span,
}

/// 效果类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectClass {
    Pure,
    ReadOnly,
    Mutating,
    Creating,
    Destroying,
}

/// 调度器提示
#[derive(Debug, Clone)]
pub struct SchedulerHint {
    pub parallelizable: bool,
    pub estimated_cycles: u64,
}
