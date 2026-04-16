//! CellScript 类型系统
//!
//! 包括类型检查、线性检查和生命周期验证

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver};
use std::collections::{HashMap, HashSet};

/// 类型环境
pub struct TypeEnv {
    /// 变量类型
    vars: HashMap<String, Type>,
    /// 变量可变性
    mutability: HashMap<String, bool>,
    /// 资源线性状态
    linear_states: HashMap<String, LinearState>,
    /// 父环境
    parent: Option<Box<TypeEnv>>,
}

/// 线性状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearState {
    /// 可用
    Available,
    /// 已消费
    Consumed,
    /// 已转移
    Transferred,
    /// 已销毁
    Destroyed,
}

impl TypeEnv {
    /// 创建新的类型环境
    pub fn new() -> Self {
        Self { vars: HashMap::new(), mutability: HashMap::new(), linear_states: HashMap::new(), parent: None }
    }

    /// 创建子环境
    pub fn child(&self) -> Self {
        Self { vars: HashMap::new(), mutability: HashMap::new(), linear_states: HashMap::new(), parent: Some(Box::new(self.clone())) }
    }

    /// 查找变量类型
    pub fn lookup(&self, name: &str) -> Option<&Type> {
        self.vars.get(name).or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }

    pub fn is_mutable(&self, name: &str) -> bool {
        self.mutability.get(name).copied().or_else(|| self.parent.as_ref().map(|p| p.is_mutable(name))).unwrap_or(false)
    }

    /// 插入变量
    pub fn insert(&mut self, name: String, ty: Type, is_linear: bool, is_mut: bool) {
        self.vars.insert(name.clone(), ty);
        self.mutability.insert(name.clone(), is_mut);
        if is_linear {
            self.linear_states.insert(name, LinearState::Available);
        }
    }

    /// 标记资源为已消费
    pub fn consume(&mut self, name: &str) -> Result<()> {
        match self.linear_states.get_mut(name) {
            Some(state) => {
                if *state != LinearState::Available {
                    return Err(CompileError::new(format!("resource '{}' already {:?}", name, state), Span::default()));
                }
                *state = LinearState::Consumed;
                Ok(())
            }
            None => {
                // 检查父环境
                if let Some(ref mut parent) = self.parent {
                    parent.consume(name)
                } else {
                    Err(CompileError::new(format!("unknown resource '{}'", name), Span::default()))
                }
            }
        }
    }

    /// 检查所有线性资源是否已正确处理
    pub fn check_linear_complete(&self) -> Result<()> {
        for (name, state) in &self.linear_states {
            if *state == LinearState::Available {
                return Err(CompileError::new(
                    format!("linear resource '{}' was not consumed, transferred, or destroyed", name),
                    Span::default(),
                ));
            }
        }
        Ok(())
    }
}

impl Clone for TypeEnv {
    fn clone(&self) -> Self {
        Self {
            vars: self.vars.clone(),
            mutability: self.mutability.clone(),
            linear_states: self.linear_states.clone(),
            parent: self.parent.as_ref().map(|p| Box::new((**p).clone())),
        }
    }
}

/// 类型检查器
pub struct TypeChecker<'a> {
    env: TypeEnv,
    type_fields: HashMap<String, HashMap<String, Type>>,
    functions: HashMap<String, Option<Type>>,
    linear_types: HashSet<String>,
    resolver: Option<&'a ModuleResolver>,
    current_module: Option<String>,
}

impl<'a> TypeChecker<'a> {
    /// 创建新的类型检查器
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            type_fields: HashMap::new(),
            functions: HashMap::new(),
            linear_types: HashSet::new(),
            resolver: None,
            current_module: None,
        }
    }

    pub fn with_resolver(resolver: &'a ModuleResolver, current_module: impl Into<String>) -> Self {
        let mut checker = Self::new();
        checker.resolver = Some(resolver);
        checker.current_module = Some(current_module.into());
        checker
    }

    /// 检查模块
    pub fn check_module(&mut self, module: &Module) -> Result<()> {
        for item in &module.items {
            match item {
                Item::Const(const_def) => {
                    self.validate_type(&const_def.ty)?;
                    self.env.insert(const_def.name.clone(), const_def.ty.clone(), false, false);
                }
                Item::Resource(resource) => {
                    self.linear_types.insert(resource.name.clone());
                    self.type_fields.insert(
                        resource.name.clone(),
                        resource.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Shared(shared) => {
                    self.linear_types.insert(shared.name.clone());
                    self.type_fields.insert(
                        shared.name.clone(),
                        shared.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Receipt(receipt) => {
                    self.linear_types.insert(receipt.name.clone());
                    self.type_fields.insert(
                        receipt.name.clone(),
                        receipt.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Struct(struct_def) => {
                    self.type_fields.insert(
                        struct_def.name.clone(),
                        struct_def.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Action(action) => {
                    self.functions.insert(action.name.clone(), action.return_type.clone());
                }
                Item::Function(function) => {
                    self.functions.insert(function.name.clone(), function.return_type.clone());
                }
                Item::Lock(lock) => {
                    self.functions.insert(lock.name.clone(), Some(Type::Bool));
                }
                Item::Enum(_) | Item::Use(_) => {}
            }
        }

        for item in &module.items {
            self.check_item(item)?;
        }
        Ok(())
    }

    /// 检查模块项
    fn check_item(&mut self, item: &Item) -> Result<()> {
        match item {
            Item::Resource(r) => self.check_resource(r),
            Item::Shared(s) => self.check_shared(s),
            Item::Receipt(r) => self.check_receipt(r),
            Item::Struct(s) => self.check_struct(s),
            Item::Const(c) => self.check_const(c),
            Item::Enum(_) => Ok(()),
            Item::Action(a) => self.check_action(a),
            Item::Function(f) => self.check_action(f),
            Item::Lock(l) => self.check_lock(l),
            Item::Use(_) => Ok(()), // use 语句不需要类型检查
        }
    }

    /// 检查 resource 定义
    fn check_resource(&mut self, resource: &ResourceDef) -> Result<()> {
        // 检查字段类型
        for field in &resource.fields {
            self.validate_type(&field.ty)?;
        }
        Ok(())
    }

    /// 检查 shared 定义
    fn check_shared(&mut self, shared: &SharedDef) -> Result<()> {
        for field in &shared.fields {
            self.validate_type(&field.ty)?;
        }
        Ok(())
    }

    /// 检查 receipt 定义
    fn check_receipt(&mut self, receipt: &ReceiptDef) -> Result<()> {
        for field in &receipt.fields {
            self.validate_type(&field.ty)?;
        }
        Ok(())
    }

    /// 检查 struct 定义
    fn check_struct(&mut self, struct_def: &StructDef) -> Result<()> {
        for field in &struct_def.fields {
            self.validate_type(&field.ty)?;
        }
        Ok(())
    }

    fn check_const(&mut self, const_def: &ConstDef) -> Result<()> {
        let mut env = self.env.clone();
        let value_ty = self.infer_expr(&mut env, &const_def.value)?;
        if !self.types_equal(&value_ty, &const_def.ty) {
            return Err(CompileError::new(
                format!("const '{}' has type mismatch: expected {:?}, found {:?}", const_def.name, const_def.ty, value_ty),
                const_def.span,
            ));
        }
        Ok(())
    }

    /// 检查 action 定义
    fn check_action(&mut self, action: &ActionDef) -> Result<()> {
        // 创建新的环境
        let mut env = self.env.child();

        // 添加参数到环境
        for param in &action.params {
            let is_linear = self.is_linear_type(&param.ty);
            env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
        }

        // 检查函数体
        for stmt in &action.body {
            self.check_stmt(&mut env, stmt)?;
        }

        if let Some(stmt) = action.body.last() {
            self.mark_stmt_as_returned(&mut env, stmt)?;
        }

        // 检查所有线性资源是否已处理
        env.check_linear_complete()
    }

    /// 检查 lock 定义
    fn check_lock(&mut self, lock: &LockDef) -> Result<()> {
        if lock.return_type != Type::Bool {
            return Err(CompileError::new("lock definitions must return bool", lock.span));
        }

        let mut env = self.env.child();

        for param in &lock.params {
            let is_linear = self.is_linear_type(&param.ty);
            env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
        }

        for stmt in &lock.body {
            self.check_stmt(&mut env, stmt)?;
        }

        let Some(stmt) = lock.body.last() else {
            return Err(CompileError::new("lock body must return a bool value", lock.span));
        };
        let return_ty = self.infer_lock_terminal_stmt(&mut env, stmt)?;
        if !self.is_bool_type(&return_ty) {
            return Err(CompileError::new("lock body must evaluate to bool", lock.span));
        }
        self.mark_stmt_as_returned(&mut env, stmt)?;

        env.check_linear_complete()
    }

    /// 检查语句
    fn check_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let ty = self.infer_expr(env, &let_stmt.value)?;
                if let Some(ref declared_ty) = let_stmt.ty {
                    if !self.types_equal(&ty, declared_ty) {
                        return Err(CompileError::new(
                            format!("type mismatch: expected {:?}, found {:?}", declared_ty, ty),
                            let_stmt.span,
                        ));
                    }
                }
                self.bind_pattern(env, &let_stmt.pattern, &ty, let_stmt.is_mut, let_stmt.span)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::Return(None) => Ok(()),
            Stmt::Return(Some(expr)) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::If(if_stmt) => {
                let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if condition must be boolean", if_stmt.span));
                }
                let mut then_env = env.child();
                for stmt in &if_stmt.then_branch {
                    self.check_stmt(&mut then_env, stmt)?;
                }
                if let Some(ref else_branch) = if_stmt.else_branch {
                    let mut else_env = env.child();
                    for stmt in else_branch {
                        self.check_stmt(&mut else_env, stmt)?;
                    }
                }
                Ok(())
            }
            Stmt::For(for_stmt) => {
                let iter_ty = self.infer_expr(env, &for_stmt.iterable)?;
                let mut loop_env = env.child();
                let item_ty = self.iter_item_type(&iter_ty, for_stmt.span)?;
                self.bind_pattern(&mut loop_env, &for_stmt.pattern, &item_ty, false, for_stmt.span)?;
                for stmt in &for_stmt.body {
                    self.check_stmt(&mut loop_env, stmt)?;
                }
                Ok(())
            }
            Stmt::While(while_stmt) => {
                let cond_ty = self.infer_expr(env, &while_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("while condition must be boolean", while_stmt.span));
                }
                let mut while_env = env.child();
                for stmt in &while_stmt.body {
                    self.check_stmt(&mut while_env, stmt)?;
                }
                Ok(())
            }
        }
    }

    /// 推断表达式类型
    fn infer_expr(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        match expr {
            Expr::Integer(_) => Ok(Type::U64),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::Named("String".to_string())),
            Expr::ByteString(_) => Ok(Type::Array(Box::new(Type::U8), 0)),
            Expr::Identifier(name) => {
                if let Some(ty) = env.lookup(name).cloned() {
                    Ok(ty)
                } else if let Some(constant) = self.resolve_constant(name) {
                    Ok(constant.ty)
                } else if let Some((prefix, _)) = name.split_once("::") {
                    Ok(Type::Named(prefix.to_string()))
                } else {
                    Err(CompileError::new(format!("undefined variable '{}'", name), Span::default()))
                }
            }
            Expr::Assign(assign) => self.infer_assign_expr(env, assign),
            Expr::Binary(bin) => {
                let left_ty = self.infer_expr(env, &bin.left)?;
                let right_ty = self.infer_expr(env, &bin.right)?;

                match bin.op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("arithmetic operations require numeric types", bin.span));
                        }
                        Ok(left_ty)
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        if !self.types_equal(&left_ty, &right_ty) {
                            return Err(CompileError::new("comparison requires matching types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("ordering comparison requires numeric types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        if !self.is_bool_type(&left_ty) || !self.is_bool_type(&right_ty) {
                            return Err(CompileError::new("logical operations require boolean types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                }
            }
            Expr::Unary(unary) => {
                let expr_ty = self.infer_expr(env, &unary.expr)?;
                match unary.op {
                    UnaryOp::Neg => {
                        if !self.is_numeric_type(&expr_ty) {
                            return Err(CompileError::new("negation requires numeric type", unary.span));
                        }
                        Ok(expr_ty)
                    }
                    UnaryOp::Not => {
                        if !self.is_bool_type(&expr_ty) {
                            return Err(CompileError::new("logical not requires boolean type", unary.span));
                        }
                        Ok(Type::Bool)
                    }
                    UnaryOp::Ref => Ok(Type::Ref(Box::new(expr_ty))),
                    UnaryOp::Deref => match expr_ty {
                        Type::Ref(inner) | Type::MutRef(inner) => Ok((*inner).clone()),
                        _ => Err(CompileError::new("cannot dereference a non-reference value", unary.span)),
                    },
                }
            }
            Expr::Call(call) => {
                for arg in &call.args {
                    self.infer_expr(env, arg)?;
                }
                for arg in &call.args {
                    self.mark_expr_as_moved(env, arg)?;
                }
                self.infer_call_type(env, call)
            }
            Expr::FieldAccess(field) => {
                let expr_ty = self.infer_expr(env, &field.expr)?;
                self.lookup_field_type(&expr_ty, &field.field, field.span)
            }
            Expr::Index(index) => {
                let expr_ty = self.infer_expr(env, &index.expr)?;
                let index_ty = self.infer_expr(env, &index.index)?;
                if !self.is_numeric_type(&index_ty) {
                    return Err(CompileError::new("index expression requires a numeric index", index.span));
                }
                self.index_result_type(&expr_ty, index.span)
            }
            Expr::Create(create) => {
                // 检查字段
                for (_, value) in &create.fields {
                    self.infer_expr(env, value)?;
                }
                Ok(Type::Named(create.ty.clone()))
            }
            Expr::Consume(consume) => {
                if let Expr::Identifier(name) = consume.expr.as_ref() {
                    match env.lookup(name).cloned() {
                        Some(ty) if self.is_linear_type(&ty) => env.consume(name)?,
                        Some(Type::Named(_)) => {}
                        Some(_) => {}
                        None => return Err(CompileError::new(format!("undefined variable '{}'", name), Span::default())),
                    }
                }
                Ok(Type::U64)
            }
            Expr::Transfer(transfer) => {
                let expr_ty = self.infer_expr(env, &transfer.expr)?;
                let to_ty = self.infer_expr(env, &transfer.to)?;
                if !self.is_address_like_type(&to_ty) {
                    return Err(CompileError::new("transfer destination must be address-like", transfer.span));
                }
                if let Expr::Identifier(name) = transfer.expr.as_ref() {
                    if self.is_linear_type(&expr_ty) {
                        env.consume(name)?;
                    }
                }
                Ok(expr_ty)
            }
            Expr::Destroy(destroy) => {
                if let Expr::Identifier(name) = destroy.expr.as_ref() {
                    match env.lookup(name).cloned() {
                        Some(ty) if self.is_linear_type(&ty) => env.consume(name)?,
                        Some(Type::Named(_)) => {}
                        Some(_) => {}
                        None => return Err(CompileError::new(format!("undefined variable '{}'", name), Span::default())),
                    }
                }
                Ok(Type::U64)
            }
            Expr::ReadRef(read_ref) => Ok(Type::Ref(Box::new(Type::Named(read_ref.ty.clone())))),
            Expr::Claim(claim) => {
                let receipt_ty = self.infer_expr(env, &claim.receipt)?;
                if let Expr::Identifier(name) = claim.receipt.as_ref() {
                    if self.is_linear_type(&receipt_ty) {
                        env.consume(name)?;
                    }
                }
                Ok(Type::U64)
            }
            Expr::Settle(settle) => self.infer_expr(env, &settle.expr),
            Expr::Block(stmts) => {
                let mut block_env = env.child();
                let mut last_ty = Type::U64;
                for stmt in stmts {
                    match stmt {
                        Stmt::Expr(e) => {
                            last_ty = self.infer_expr(&mut block_env, e)?;
                        }
                        _ => {
                            self.check_stmt(&mut block_env, stmt)?;
                        }
                    }
                }
                Ok(last_ty)
            }
            Expr::Tuple(elems) => {
                let mut types = Vec::new();
                for elem in elems {
                    types.push(self.infer_expr(env, elem)?);
                }
                Ok(Type::Tuple(types))
            }
            Expr::Array(elems) => {
                if elems.is_empty() {
                    return Ok(Type::Array(Box::new(Type::U64), 0));
                }
                let elem_ty = self.infer_expr(env, &elems[0])?;
                Ok(Type::Array(Box::new(elem_ty), elems.len()))
            }
            Expr::If(if_expr) => {
                let cond_ty = self.infer_expr(env, &if_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if expression condition must be boolean", if_expr.span));
                }
                let then_ty = self.infer_expr(env, &if_expr.then_branch)?;
                let else_ty = self.infer_expr(env, &if_expr.else_branch)?;
                if self.types_equal(&then_ty, &else_ty) {
                    Ok(then_ty)
                } else {
                    Err(CompileError::new(
                        format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                        if_expr.span,
                    ))
                }
            }
            Expr::Cast(cast) => {
                self.infer_expr(env, &cast.expr)?;
                Ok(cast.ty.clone())
            }
            Expr::Range(range) => {
                self.infer_expr(env, &range.start)?;
                self.infer_expr(env, &range.end)?;
                Ok(Type::Named("Range".to_string()))
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.infer_expr(env, value)?;
                }
                Ok(Type::Named(init.ty.clone()))
            }
            Expr::Match(match_expr) => {
                self.infer_expr(env, &match_expr.expr)?;
                let mut arm_ty = None;
                for arm in &match_expr.arms {
                    let ty = self.infer_expr(env, &arm.value)?;
                    if arm_ty.as_ref().is_none_or(|existing| self.types_equal(existing, &ty)) {
                        arm_ty = Some(ty);
                    } else {
                        return Err(CompileError::new("match arms must have matching types", arm.span));
                    }
                }
                arm_ty.ok_or_else(|| CompileError::new("match expression must contain at least one arm", match_expr.span))
            }
        }
    }

    fn bind_pattern(&self, env: &mut TypeEnv, pattern: &BindingPattern, ty: &Type, is_mut: bool, span: Span) -> Result<()> {
        match pattern {
            BindingPattern::Name(name) => {
                let is_linear = self.is_linear_type(ty);
                env.insert(name.clone(), ty.clone(), is_linear, is_mut);
                Ok(())
            }
            BindingPattern::Wildcard => Ok(()),
            BindingPattern::Tuple(items) => {
                let Type::Tuple(types) = ty else {
                    return Err(CompileError::new("tuple binding requires a tuple value", span));
                };
                if items.len() != types.len() {
                    return Err(CompileError::new(
                        format!("tuple binding arity mismatch: pattern has {}, value has {}", items.len(), types.len()),
                        span,
                    ));
                }
                for (item, item_ty) in items.iter().zip(types.iter()) {
                    self.bind_pattern(env, item, item_ty, is_mut, span)?;
                }
                Ok(())
            }
        }
    }

    fn mark_stmt_as_returned(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Expr(expr) => self.mark_expr_as_moved(env, expr),
            Stmt::Return(Some(expr)) => self.mark_expr_as_moved(env, expr),
            _ => Ok(()),
        }
    }

    fn infer_lock_terminal_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<Type> {
        match stmt {
            Stmt::Expr(expr) => self.infer_expr(env, expr),
            Stmt::Return(Some(expr)) => self.infer_expr(env, expr),
            Stmt::If(if_stmt) => {
                let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if condition must be boolean", if_stmt.span));
                }
                let mut then_env = env.child();
                let then_ty = if let Some(stmt) = if_stmt.then_branch.last() {
                    for stmt in &if_stmt.then_branch[..if_stmt.then_branch.len().saturating_sub(1)] {
                        self.check_stmt(&mut then_env, stmt)?;
                    }
                    self.infer_lock_terminal_stmt(&mut then_env, stmt)?
                } else {
                    return Err(CompileError::new("lock if branch must end with a bool expression", if_stmt.span));
                };
                let else_branch = if_stmt
                    .else_branch
                    .as_ref()
                    .ok_or_else(|| CompileError::new("lock if statement must have an else branch", if_stmt.span))?;
                let mut else_env = env.child();
                let else_ty = if let Some(stmt) = else_branch.last() {
                    for stmt in &else_branch[..else_branch.len().saturating_sub(1)] {
                        self.check_stmt(&mut else_env, stmt)?;
                    }
                    self.infer_lock_terminal_stmt(&mut else_env, stmt)?
                } else {
                    return Err(CompileError::new("lock else branch must end with a bool expression", if_stmt.span));
                };
                if !self.types_equal(&then_ty, &else_ty) {
                    return Err(CompileError::new("lock branches must return matching types", if_stmt.span));
                }
                Ok(then_ty)
            }
            _ => Err(CompileError::new("lock body must end with an expression or explicit return", stmt_span(stmt))),
        }
    }

    fn mark_expr_as_moved(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Identifier(name) => {
                if let Some(ty) = env.lookup(name).cloned() {
                    if self.is_linear_type(&ty) {
                        env.consume(name)?;
                    }
                }
                Ok(())
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.mark_expr_as_moved(env, item)?;
                }
                Ok(())
            }
            Expr::Cast(cast) => self.mark_expr_as_moved(env, &cast.expr),
            Expr::Assign(assign) => self.mark_expr_as_moved(env, &assign.value),
            Expr::Transfer(_) | Expr::Claim(_) => Ok(()),
            Expr::Settle(settle) => self.mark_expr_as_moved(env, &settle.expr),
            Expr::If(if_expr) => {
                self.mark_expr_as_moved(env, &if_expr.then_branch)?;
                self.mark_expr_as_moved(env, &if_expr.else_branch)
            }
            Expr::Match(match_expr) => {
                for arm in &match_expr.arms {
                    self.mark_expr_as_moved(env, &arm.value)?;
                }
                Ok(())
            }
            Expr::Block(stmts) => {
                if let Some(stmt) = stmts.last() {
                    self.mark_stmt_as_returned(env, stmt)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn infer_assign_expr(&mut self, env: &mut TypeEnv, assign: &AssignExpr) -> Result<Type> {
        let value_ty = self.infer_expr(env, &assign.value)?;

        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                let Some(target_ty) = env.lookup(name).cloned() else {
                    return Err(CompileError::new(format!("undefined variable '{}'", name), assign.span));
                };
                if self.is_linear_type(&target_ty) {
                    return Err(CompileError::new("assignment to linear/resource variables is not supported yet", assign.span));
                }
                if !env.is_mutable(name) {
                    return Err(CompileError::new(format!("variable '{}' is not mutable", name), assign.span));
                }
                match assign.op {
                    AssignOp::Assign => {
                        if !self.types_equal(&target_ty, &value_ty) {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            Expr::FieldAccess(_) | Expr::Index(_) => {
                let target_ty = self.infer_expr(env, &assign.target)?;
                match assign.op {
                    AssignOp::Assign => {
                        if !self.types_equal(&target_ty, &value_ty) {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            _ => Err(CompileError::new("invalid assignment target", assign.span)),
        }
    }

    fn index_result_type(&self, ty: &Type, span: Span) -> Result<Type> {
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) | Type::MutRef(inner) => self.index_result_type(inner, span),
            Type::Named(name) => self
                .parse_named_collection_item_type(name)
                .ok_or_else(|| CompileError::new(format!("indexing is not supported for type '{}'", name), span)),
            _ => Err(CompileError::new("indexing requires an array-like value", span)),
        }
    }

    fn iter_item_type(&self, ty: &Type, span: Span) -> Result<Type> {
        match ty {
            Type::Array(elem, _) => Ok((**elem).clone()),
            Type::Ref(inner) => Ok(Type::Ref(Box::new(self.iter_item_type(inner, span)?))),
            Type::MutRef(inner) => Ok(Type::MutRef(Box::new(self.iter_item_type(inner, span)?))),
            Type::Named(name) if name == "Range" => Ok(Type::U64),
            Type::Named(name) => self
                .parse_named_collection_item_type(name)
                .ok_or_else(|| CompileError::new(format!("cannot iterate over type '{}'", name), span)),
            _ => Err(CompileError::new("for-loop iterable must be a range or collection type", span)),
        }
    }

    fn parse_named_collection_item_type(&self, name: &str) -> Option<Type> {
        if let Some(inner) = name.strip_prefix("Vec<").and_then(|rest| rest.strip_suffix('>')) {
            return Some(self.parse_named_type_repr(inner));
        }
        None
    }

    fn parse_named_type_repr(&self, repr: &str) -> Type {
        match repr.trim() {
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "u128" => Type::U128,
            "bool" => Type::Bool,
            "Address" => Type::Address,
            "Hash" => Type::Hash,
            other => Type::Named(other.to_string()),
        }
    }

    fn lookup_field_type(&self, ty: &Type, field: &str, span: Span) -> Result<Type> {
        match ty {
            Type::Address | Type::Hash => {
                if field == "0" {
                    return Ok(Type::Array(Box::new(Type::U8), 32));
                }
                Err(CompileError::new(format!("builtin value '{:?}' only exposes tuple field '0'", ty), span))
            }
            Type::Tuple(items) => {
                if let Ok(index) = field.parse::<usize>() {
                    if let Some(item_ty) = items.get(index) {
                        return Ok(item_ty.clone());
                    }
                    return Err(CompileError::new(format!("tuple field '{}' is out of bounds", field), span));
                }
                Err(CompileError::new(format!("tuple field '{}' must be a numeric index", field), span))
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.lookup_field_type(inner, field, span),
            Type::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                if let Some(fields) = self.type_fields.get(base_name) {
                    if let Some(field_ty) = fields.get(field) {
                        return Ok(field_ty.clone());
                    }
                }
                if let Some(module) = &self.current_module {
                    if let Some(resolver) = self.resolver {
                        if let Some(fields) = resolver.type_fields(module, base_name) {
                            if let Some((_, field_ty)) = fields.into_iter().find(|(field_name, _)| field_name == field) {
                                return Ok(field_ty);
                            }
                        }
                    }
                }
                Err(CompileError::new(format!("unknown field '{}' on type '{}'", field, base_name), span))
            }
            _ => Err(CompileError::new(format!("type '{:?}' does not support field access", ty), span)),
        }
    }

    fn infer_call_type(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Type> {
        match call.func.as_ref() {
            Expr::Identifier(name) => {
                if let Some(ret_ty) = self.functions.get(name).cloned().flatten() {
                    return Ok(ret_ty);
                }
                if let Some(function) = self.resolve_function(name) {
                    return Ok(self.function_return_type(&function).unwrap_or(Type::U64));
                }
                if let Some((prefix, suffix)) = name.rsplit_once("::") {
                    return Ok(match (prefix, suffix) {
                        ("env", "current_daa_score") => Type::U64,
                        ("Address", "zero") => Type::Address,
                        ("Hash", "zero") => Type::Hash,
                        (_, "new") => Type::Named(prefix.to_string()),
                        (_, "zero") => Type::Named(prefix.to_string()),
                        _ => return Err(CompileError::new(format!("unknown namespaced function '{}'", name), call.span)),
                    });
                }
                if name == "min" || name == "max" || name == "isqrt" {
                    return Ok(Type::U64);
                }
                Err(CompileError::new(format!("unknown function '{}'", name), call.span))
            }
            Expr::FieldAccess(field) => {
                let receiver_ty = self.infer_expr(env, &field.expr)?;
                match field.field.as_str() {
                    "type_hash" => Ok(Type::Hash),
                    "len" => Ok(Type::U64),
                    "push" => Ok(Type::U64),
                    "extend_from_slice" => Ok(Type::U64),
                    _ => self.lookup_field_type(&receiver_ty, &field.field, field.span),
                }
            }
            _ => Err(CompileError::new("unsupported call target", call.span)),
        }
    }

    fn resolve_function(&self, name: &str) -> Option<FunctionDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_function(module, name))
    }

    fn resolve_constant(&self, name: &str) -> Option<crate::resolve::ConstantDef> {
        self.resolver.zip(self.current_module.as_deref()).and_then(|(resolver, module)| resolver.resolve_constant(module, name))
    }

    fn function_return_type(&self, function: &FunctionDef) -> Option<Type> {
        match function {
            FunctionDef::Action(action) | FunctionDef::Function(action) => action.return_type.clone(),
            FunctionDef::Lock(_) => Some(Type::Bool),
        }
    }

    /// 验证类型
    fn validate_type(&self, ty: &Type) -> Result<()> {
        match ty {
            Type::Array(elem_ty, _) => self.validate_type(elem_ty),
            Type::Tuple(types) => {
                for t in types {
                    self.validate_type(t)?;
                }
                Ok(())
            }
            Type::Ref(inner) | Type::MutRef(inner) => self.validate_type(inner),
            _ => Ok(()),
        }
    }

    /// 检查类型是否相等
    fn types_equal(&self, a: &Type, b: &Type) -> bool {
        if self.is_numeric_type(a) && self.is_numeric_type(b) {
            return true;
        }
        match (a, b) {
            (Type::U8, Type::U8) => true,
            (Type::U16, Type::U16) => true,
            (Type::U32, Type::U32) => true,
            (Type::U64, Type::U64) => true,
            (Type::U128, Type::U128) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Address, Type::Address) => true,
            (Type::Hash, Type::Hash) => true,
            (Type::Array(a1, n1), Type::Array(b1, n2)) => n1 == n2 && self.types_equal(a1, b1),
            (Type::Tuple(a1), Type::Tuple(b1)) => {
                a1.len() == b1.len() && a1.iter().zip(b1.iter()).all(|(x, y)| self.types_equal(x, y))
            }
            (Type::Named(a1), Type::Named(b1)) => a1 == b1,
            (Type::Ref(a1), Type::Ref(b1)) => self.types_equal(a1, b1),
            (Type::MutRef(a1), Type::MutRef(b1)) => self.types_equal(a1, b1),
            _ => false,
        }
    }

    /// 检查是否为线性类型
    fn is_linear_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Named(name) => {
                let base_name = name.split('<').next().unwrap_or(name.as_str());
                self.linear_types.contains(base_name)
                    || self
                        .resolver
                        .zip(self.current_module.as_ref())
                        .is_some_and(|(resolver, module)| resolver.type_is_linear(module, base_name))
            }
            _ => false,
        }
    }

    /// 检查是否为数值类型
    fn is_numeric_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128)
            || matches!(ty, Type::Named(name) if name == "usize" || name == "isize")
    }

    /// 检查是否为布尔类型
    fn is_bool_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Bool)
    }

    fn is_address_like_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Address => true,
            Type::Ref(inner) | Type::MutRef(inner) => self.is_address_like_type(inner),
            _ => false,
        }
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(let_stmt) => let_stmt.span,
        Stmt::Expr(expr) => expr_span(expr),
        Stmt::Return(Some(expr)) => expr_span(expr),
        Stmt::Return(None) => Span::default(),
        Stmt::If(if_stmt) => if_stmt.span,
        Stmt::For(for_stmt) => for_stmt.span,
        Stmt::While(while_stmt) => while_stmt.span,
    }
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) => Span::default(),
        Expr::Assign(assign) => assign.span,
        Expr::Binary(binary) => binary.span,
        Expr::Unary(unary) => unary.span,
        Expr::Call(call) => call.span,
        Expr::FieldAccess(field) => field.span,
        Expr::Index(index) => index.span,
        Expr::Create(create) => create.span,
        Expr::Consume(consume) => consume.span,
        Expr::Transfer(transfer) => transfer.span,
        Expr::Destroy(destroy) => destroy.span,
        Expr::ReadRef(read_ref) => read_ref.span,
        Expr::Claim(claim) => claim.span,
        Expr::Settle(settle) => settle.span,
        Expr::Block(stmts) => stmts.last().map(stmt_span).unwrap_or_default(),
        Expr::Tuple(_) | Expr::Array(_) => Span::default(),
        Expr::If(if_expr) => if_expr.span,
        Expr::Cast(cast) => cast.span,
        Expr::Range(range) => range.span,
        Expr::StructInit(init) => init.span,
        Expr::Match(match_expr) => match_expr.span,
    }
}

/// 类型检查入口函数
pub fn check(module: &Module) -> Result<()> {
    let mut checker = TypeChecker::new();
    checker.check_module(module)
}

pub fn check_with_resolver(module: &Module, resolver: &ModuleResolver, current_module: &str) -> Result<()> {
    let mut checker = TypeChecker::with_resolver(resolver, current_module);
    checker.check_module(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser, resolve::ModuleResolver};
    use camino::Utf8PathBuf;

    fn example_module(name: &str) -> Module {
        let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
        let source = std::fs::read_to_string(path).unwrap();
        let tokens = lexer::lex(&source).unwrap();
        parser::parse(&tokens).unwrap()
    }

    #[test]
    fn imported_token_type_is_treated_as_linear() {
        let token = example_module("token.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        let checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
        assert!(checker.is_linear_type(&Type::Named("Token".to_string())));
    }

    #[test]
    fn launch_module_type_checks_with_registered_imports() {
        let token = example_module("token.cell");
        let amm = example_module("amm_pool.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(amm).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        check_with_resolver(&launch, &resolver, &launch.name).unwrap();
    }

    #[test]
    fn imported_linear_argument_is_marked_consumed_after_call() {
        let token = example_module("token.cell");
        let amm = example_module("amm_pool.cell");
        let launch = example_module("launch.cell");

        let mut resolver = ModuleResolver::new();
        resolver.register_module(token).unwrap();
        resolver.register_module(amm).unwrap();
        resolver.register_module(launch.clone()).unwrap();

        let action = launch
            .items
            .iter()
            .find_map(|item| match item {
                Item::Action(action) if action.name == "launch_token" => Some(action.clone()),
                _ => None,
            })
            .unwrap();

        let mut checker = TypeChecker::with_resolver(&resolver, launch.name.clone());
        let mut env = checker.env.child();
        for param in &action.params {
            let is_linear = checker.is_linear_type(&param.ty);
            env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
        }

        for stmt in &action.body {
            checker.check_stmt(&mut env, stmt).unwrap();
            if let Stmt::Let(let_stmt) = stmt {
                if matches!(&let_stmt.pattern, BindingPattern::Tuple(_)) {
                    break;
                }
            }
        }

        assert_eq!(env.linear_states.get("pool_paired_token"), Some(&LinearState::Consumed));
    }
}
