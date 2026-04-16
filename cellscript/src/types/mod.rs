//! CellScript 类型系统
//!
//! 包括类型检查、线性检查和生命周期验证

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::resolve::{FunctionDef, ModuleResolver, TypeDef};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallableKind {
    Action,
    Function,
    Lock,
}

#[derive(Debug, Clone)]
struct FunctionSignature {
    return_type: Option<Type>,
    kind: CallableKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellTypeKind {
    Resource,
    Shared,
    Receipt,
}

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

    fn update_type(&mut self, name: &str, ty: Type) -> bool {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), ty);
            true
        } else {
            self.parent.as_mut().map(|parent| parent.update_type(name, ty)).unwrap_or(false)
        }
    }

    fn merge_existing_type_refinements_from(&mut self, other: &TypeEnv) {
        let names = self.vars.keys().cloned().collect::<Vec<_>>();
        for name in names {
            if let Some(ty) = other.lookup(&name).cloned() {
                self.vars.insert(name, ty);
            }
        }
        if let Some(parent) = self.parent.as_mut() {
            parent.merge_existing_type_refinements_from(other);
        }
    }

    /// 标记资源为已消费
    pub fn consume(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Consumed)
    }

    pub fn transfer(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Transferred)
    }

    pub fn destroy(&mut self, name: &str) -> Result<()> {
        self.set_linear_state(name, LinearState::Destroyed)
    }

    fn set_linear_state(&mut self, name: &str, next: LinearState) -> Result<()> {
        match self.linear_states.get_mut(name) {
            Some(state) => {
                if *state != LinearState::Available {
                    return Err(CompileError::new(format!("resource '{}' already {:?}", name, state), Span::default()));
                }
                *state = next;
                Ok(())
            }
            None => {
                // 检查父环境
                if let Some(ref mut parent) = self.parent {
                    parent.set_linear_state(name, next)
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
    functions: HashMap<String, FunctionSignature>,
    linear_types: HashSet<String>,
    cell_type_kinds: HashMap<String, CellTypeKind>,
    type_capabilities: HashMap<String, HashSet<Capability>>,
    receipt_claim_outputs: HashMap<String, Option<Type>>,
    resolver: Option<&'a ModuleResolver>,
    current_module: Option<String>,
    current_callable: Option<CallableKind>,
    current_return_type: Option<Option<Type>>,
}

fn function_def_kind(function: &FunctionDef) -> CallableKind {
    match function {
        FunctionDef::Action(_) => CallableKind::Action,
        FunctionDef::Function(_) => CallableKind::Function,
        FunctionDef::Lock(_) => CallableKind::Lock,
    }
}

fn type_repr(ty: &Type) -> String {
    match ty {
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Unit => "()".to_string(),
        Type::Address => "Address".to_string(),
        Type::Hash => "Hash".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", type_repr(inner), size),
        Type::Tuple(items) => format!("({})", items.iter().map(type_repr).collect::<Vec<_>>().join(", ")),
        Type::Named(name) => name.clone(),
        Type::Ref(inner) => format!("&{}", type_repr(inner)),
        Type::MutRef(inner) => format!("&mut {}", type_repr(inner)),
    }
}

impl<'a> TypeChecker<'a> {
    /// 创建新的类型检查器
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            type_fields: HashMap::new(),
            functions: HashMap::new(),
            linear_types: HashSet::new(),
            cell_type_kinds: HashMap::new(),
            type_capabilities: HashMap::new(),
            receipt_claim_outputs: HashMap::new(),
            resolver: None,
            current_module: None,
            current_callable: None,
            current_return_type: None,
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
        if self.current_module.is_none() {
            self.current_module = Some(module.name.clone());
        }
        for item in &module.items {
            match item {
                Item::Const(const_def) => {
                    self.validate_type(&const_def.ty)?;
                    self.env.insert(const_def.name.clone(), const_def.ty.clone(), false, false);
                }
                Item::Resource(resource) => {
                    self.linear_types.insert(resource.name.clone());
                    self.cell_type_kinds.insert(resource.name.clone(), CellTypeKind::Resource);
                    self.type_capabilities.insert(resource.name.clone(), resource.capabilities.iter().copied().collect());
                    self.type_fields.insert(
                        resource.name.clone(),
                        resource.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Shared(shared) => {
                    self.linear_types.insert(shared.name.clone());
                    self.cell_type_kinds.insert(shared.name.clone(), CellTypeKind::Shared);
                    self.type_capabilities.insert(shared.name.clone(), shared.capabilities.iter().copied().collect());
                    self.type_fields.insert(
                        shared.name.clone(),
                        shared.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect(),
                    );
                }
                Item::Receipt(receipt) => {
                    self.linear_types.insert(receipt.name.clone());
                    self.cell_type_kinds.insert(receipt.name.clone(), CellTypeKind::Receipt);
                    self.type_capabilities.insert(receipt.name.clone(), receipt.capabilities.iter().copied().collect());
                    self.receipt_claim_outputs.insert(receipt.name.clone(), receipt.claim_output.clone());
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
                    self.functions.insert(
                        action.name.clone(),
                        FunctionSignature { return_type: action.return_type.clone(), kind: CallableKind::Action },
                    );
                }
                Item::Function(function) => {
                    self.functions.insert(
                        function.name.clone(),
                        FunctionSignature { return_type: function.return_type.clone(), kind: CallableKind::Function },
                    );
                }
                Item::Lock(lock) => {
                    self.functions
                        .insert(lock.name.clone(), FunctionSignature { return_type: Some(Type::Bool), kind: CallableKind::Lock });
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
            Item::Function(f) => self.check_function(f),
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
        if let Some(output) = &receipt.claim_output {
            self.validate_type(output)?;
            self.validate_receipt_claim_output(output, receipt.span)?;
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
        let previous_callable = self.current_callable.replace(CallableKind::Action);
        let previous_return_type = self.current_return_type.replace(action.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();

            for param in &action.params {
                let is_linear = self.is_linear_type(&param.ty);
                env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&action.body)?;

            for stmt in &action.body {
                self.check_stmt(&mut env, stmt)?;
            }

            if let Some(return_type) = &action.return_type {
                self.check_body_returns_or_tail_expr("action", &action.name, &action.body, return_type, action.span, &return_env)?;
            }

            if let Some(stmt) = action.body.last() {
                self.mark_stmt_as_returned(&mut env, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    /// 检查纯函数定义
    fn check_function(&mut self, function: &FnDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Function);
        let previous_return_type = self.current_return_type.replace(function.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();

            for param in &function.params {
                let is_linear = self.is_linear_type(&param.ty);
                env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&function.body)?;

            for stmt in &function.body {
                self.check_stmt(&mut env, stmt)?;
            }

            if let Some(return_type) = &function.return_type {
                self.check_body_returns_or_tail_expr(
                    "function",
                    &function.name,
                    &function.body,
                    return_type,
                    function.span,
                    &return_env,
                )?;
            }

            if let Some(stmt) = function.body.last() {
                self.mark_stmt_as_returned(&mut env, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    /// 检查 lock 定义
    fn check_lock(&mut self, lock: &LockDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Lock);
        let previous_return_type = self.current_return_type.replace(Some(Type::Bool));
        let result = (|| {
            if lock.return_type != Type::Bool {
                return Err(CompileError::new("lock definitions must return bool", lock.span));
            }

            let mut env = self.env.child();

            for param in &lock.params {
                let is_linear = self.is_linear_type(&param.ty);
                env.insert(param.name.clone(), param.ty.clone(), is_linear, param.is_mut);
            }
            self.check_no_unreachable_stmts(&lock.body)?;

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
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    /// 检查语句
    fn check_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let ty = self.infer_let_value_type(env, let_stmt)?;
                if let Some(ref declared_ty) = let_stmt.ty {
                    if !self.types_equal(&ty, declared_ty) {
                        return Err(CompileError::new(
                            format!("type mismatch: expected {:?}, found {:?}", declared_ty, ty),
                            let_stmt.span,
                        ));
                    }
                }
                if matches!(ty, Type::Unit) {
                    return Err(CompileError::new("cannot bind the result of a function without a return value", let_stmt.span));
                }
                self.bind_pattern(env, &let_stmt.pattern, &ty, let_stmt.is_mut, let_stmt.span)?;
                Ok(())
            }
            Stmt::Expr(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::Return(None) => {
                if let Some(Some(expected)) = &self.current_return_type {
                    return Err(CompileError::new(
                        format!("return without value in function returning {:?}", expected),
                        stmt_span(stmt),
                    ));
                }
                Ok(())
            }
            Stmt::Return(Some(expr)) => {
                let ty = self.infer_expr(env, expr)?;
                match &self.current_return_type {
                    Some(Some(expected)) if !self.types_equal(expected, &ty) => {
                        return Err(CompileError::new(
                            format!("return type mismatch: expected {:?}, found {:?}", expected, ty),
                            expr_span(expr),
                        ));
                    }
                    Some(None) => {
                        return Err(CompileError::new(
                            "return value is not allowed in a function without a return type",
                            expr_span(expr),
                        ));
                    }
                    _ => {}
                }
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
                env.merge_existing_type_refinements_from(&loop_env);
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
                env.merge_existing_type_refinements_from(&while_env);
                Ok(())
            }
        }
    }

    fn check_no_unreachable_stmts(&self, stmts: &[Stmt]) -> Result<()> {
        let mut previous_guaranteed_return = false;
        for stmt in stmts {
            if previous_guaranteed_return {
                return Err(CompileError::new("unreachable statement after guaranteed return", stmt_span(stmt)));
            }
            self.check_no_unreachable_nested(stmt)?;
            previous_guaranteed_return = self.stmt_always_returns(stmt);
        }
        Ok(())
    }

    fn check_no_unreachable_nested(&self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::If(if_stmt) => {
                self.check_no_unreachable_stmts(&if_stmt.then_branch)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.check_no_unreachable_stmts(else_branch)?;
                }
            }
            Stmt::For(for_stmt) => self.check_no_unreachable_stmts(&for_stmt.body)?,
            Stmt::While(while_stmt) => self.check_no_unreachable_stmts(&while_stmt.body)?,
            Stmt::Expr(Expr::Block(stmts)) => self.check_no_unreachable_stmts(stmts)?,
            _ => {}
        }
        Ok(())
    }

    fn infer_let_value_type(&mut self, env: &mut TypeEnv, let_stmt: &LetStmt) -> Result<Type> {
        if let Expr::Array(elems) = &let_stmt.value {
            if elems.is_empty() {
                return match &let_stmt.ty {
                    Some(declared @ Type::Array(_, 0)) => Ok(declared.clone()),
                    Some(Type::Array(_, size)) => Err(CompileError::new(
                        format!("empty array literal cannot initialize non-empty array of length {}", size),
                        let_stmt.span,
                    )),
                    Some(_) => Err(CompileError::new("empty array literal requires an array type annotation", let_stmt.span)),
                    None => Err(CompileError::new("empty array literal requires an explicit array type annotation", let_stmt.span)),
                };
            }
        }
        self.infer_expr(env, &let_stmt.value)
    }

    /// 推断表达式类型
    fn infer_expr(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        self.validate_expr_allowed_in_current_callable(expr)?;
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
                self.require_capability(&expr_ty, Capability::Transfer, "transfer", transfer.span)?;
                if let Expr::Identifier(name) = transfer.expr.as_ref() {
                    if self.is_linear_type(&expr_ty) {
                        env.transfer(name)?;
                    }
                }
                Ok(expr_ty)
            }
            Expr::Destroy(destroy) => {
                let destroy_ty = self.infer_expr(env, &destroy.expr)?;
                self.require_capability(&destroy_ty, Capability::Destroy, "destroy", destroy.span)?;
                if let Expr::Identifier(name) = destroy.expr.as_ref() {
                    match env.lookup(name).cloned() {
                        Some(ty) if self.is_linear_type(&ty) => env.destroy(name)?,
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
                if !self.is_receipt_type(&receipt_ty) {
                    return Err(CompileError::new("claim requires a receipt value", claim.span));
                }
                if let Expr::Identifier(name) = claim.receipt.as_ref() {
                    if self.is_linear_type(&receipt_ty) {
                        env.consume(name)?;
                    }
                }
                Ok(self.resolve_receipt_claim_output(&receipt_ty).unwrap_or(Type::U64))
            }
            Expr::Settle(settle) => {
                let settle_ty = self.infer_expr(env, &settle.expr)?;
                if !self.is_linear_type(&settle_ty) {
                    return Err(CompileError::new("settle requires a cell-backed linear value", settle.span));
                }
                if let Expr::Identifier(name) = settle.expr.as_ref() {
                    env.consume(name)?;
                }
                Ok(settle_ty)
            }
            Expr::Assert(assert_expr) => {
                let cond_ty = self.infer_expr(env, &assert_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("assert condition must be boolean", assert_expr.span));
                }
                if !matches!(assert_expr.message.as_ref(), Expr::String(_)) {
                    return Err(CompileError::new("assert message must be a string literal", expr_span(&assert_expr.message)));
                }
                Ok(Type::Unit)
            }
            Expr::Block(stmts) => {
                let mut block_env = env.child();
                let mut last_ty = Type::Unit;
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
                    return Err(CompileError::new("empty array literal requires an explicit array type annotation", expr_span(expr)));
                }
                let elem_ty = self.infer_expr(env, &elems[0])?;
                for elem in elems.iter().skip(1) {
                    let next_ty = self.infer_expr(env, elem)?;
                    if !self.types_equal(&elem_ty, &next_ty) {
                        return Err(CompileError::new("array elements must have matching types", expr_span(elem)));
                    }
                }
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

    fn validate_expr_allowed_in_current_callable(&self, expr: &Expr) -> Result<()> {
        if self.current_callable != Some(CallableKind::Function) {
            return Ok(());
        }

        let operation = match expr {
            Expr::Create(_) => Some("create"),
            Expr::Consume(_) => Some("consume"),
            Expr::Transfer(_) => Some("transfer"),
            Expr::Destroy(_) => Some("destroy"),
            Expr::ReadRef(_) => Some("read_ref"),
            Expr::Claim(_) => Some("claim"),
            Expr::Settle(_) => Some("settle"),
            _ => None,
        };

        if let Some(operation) = operation {
            return Err(CompileError::new(
                format!(
                    "pure function cannot contain '{}' Cell/runtime operation; move state transition logic into an action",
                    operation
                ),
                expr_span(expr),
            ));
        }

        Ok(())
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

    fn stmts_always_return(&self, stmts: &[Stmt]) -> bool {
        stmts.iter().any(|stmt| self.stmt_always_returns(stmt))
    }

    fn stmt_always_returns(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Return(_) => true,
            Stmt::If(if_stmt) => {
                let Some(else_branch) = &if_stmt.else_branch else {
                    return false;
                };
                self.stmts_always_return(&if_stmt.then_branch) && self.stmts_always_return(else_branch)
            }
            Stmt::Expr(Expr::Block(stmts)) => self.stmts_always_return(stmts),
            _ => false,
        }
    }

    fn check_body_returns_or_tail_expr(
        &mut self,
        kind: &str,
        name: &str,
        body: &[Stmt],
        return_type: &Type,
        span: Span,
        env: &TypeEnv,
    ) -> Result<()> {
        if self.body_returns_or_tail_expr(body, return_type, env)? {
            return Ok(());
        }

        Err(CompileError::new(format!("{} '{}' with a return type must return a value on all paths", kind, name), span))
    }

    fn body_returns_or_tail_expr(&mut self, body: &[Stmt], return_type: &Type, env: &TypeEnv) -> Result<bool> {
        if self.stmts_always_return(body) {
            return Ok(true);
        }

        let Some((last, prefix)) = body.split_last() else {
            return Ok(false);
        };
        let mut tail_env = env.clone();
        for stmt in prefix {
            self.check_stmt(&mut tail_env, stmt)?;
        }

        if let Stmt::Expr(expr) = last {
            let tail_ty = self.infer_expr(&mut tail_env, expr)?;
            if self.types_equal(&tail_ty, return_type) {
                return Ok(true);
            }
            return Err(CompileError::new(
                format!("tail expression type mismatch: expected {:?}, found {:?}", return_type, tail_ty),
                expr_span(expr),
            ));
        }

        if let Stmt::If(if_stmt) = last {
            let Some(else_branch) = &if_stmt.else_branch else {
                return Ok(false);
            };
            let then_ok = self.body_returns_or_tail_expr(&if_stmt.then_branch, return_type, &tail_env.child())?;
            let else_ok = self.body_returns_or_tail_expr(else_branch, return_type, &tail_env.child())?;
            return Ok(then_ok && else_ok);
        }

        Ok(false)
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
            Expr::Transfer(_) | Expr::Claim(_) | Expr::Settle(_) => Ok(()),
            Expr::Assert(assert_expr) => self.mark_expr_as_moved(env, &assert_expr.condition),
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
                if let Some(root) = assignment_root_name(assign.target.as_ref()) {
                    let root_is_mut_ref = matches!(env.lookup(root), Some(Type::MutRef(_)));
                    if !env.is_mutable(root) && !root_is_mut_ref {
                        return Err(CompileError::new(format!("assignment target rooted at '{}' is not mutable", root), assign.span));
                    }
                }
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
                if let Some(signature) = self.functions.get(name).cloned() {
                    self.validate_call_allowed(name, signature.kind, call.span)?;
                    return Ok(signature.return_type.unwrap_or(Type::Unit));
                }
                if let Some(function) = self.resolve_function(name) {
                    self.validate_call_allowed(name, function_def_kind(&function), call.span)?;
                    return Ok(self.function_return_type(&function).unwrap_or(Type::Unit));
                }
                if let Some((prefix, suffix)) = name.rsplit_once("::") {
                    if self.current_module.as_deref() == Some(prefix) {
                        if let Some(signature) = self.functions.get(suffix).cloned() {
                            self.validate_call_allowed(name, signature.kind, call.span)?;
                            return Ok(signature.return_type.unwrap_or(Type::Unit));
                        }
                    }
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
                    "push" => {
                        if call.args.len() != 1 {
                            return Err(CompileError::new("Vec.push expects exactly one argument", call.span));
                        }
                        let arg_ty = self.infer_expr(env, &call.args[0])?;
                        if let Type::Named(name) = &receiver_ty {
                            if name == "Vec" {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(&arg_ty))));
                                }
                                return Ok(Type::Unit);
                            }
                            if let Some(item_ty) = self.parse_named_collection_item_type(name) {
                                if !self.types_equal(&item_ty, &arg_ty) {
                                    return Err(CompileError::new(
                                        format!("Vec.push type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                return Ok(Type::Unit);
                            }
                        }
                        Err(CompileError::new("push is only supported on Vec values", call.span))
                    }
                    "extend_from_slice" => Ok(Type::Unit),
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
            FunctionDef::Action(action) => action.return_type.clone(),
            FunctionDef::Function(function) => function.return_type.clone(),
            FunctionDef::Lock(_) => Some(Type::Bool),
        }
    }

    fn validate_call_allowed(&self, callee_name: &str, callee_kind: CallableKind, span: Span) -> Result<()> {
        match (self.current_callable, callee_kind) {
            (Some(CallableKind::Function), CallableKind::Action) => Err(CompileError::new(
                format!("pure function cannot call action '{}'; move state transition logic into an action", callee_name),
                span,
            )),
            (Some(CallableKind::Function), CallableKind::Lock) => {
                Err(CompileError::new(format!("pure function cannot call lock '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Action) => {
                Err(CompileError::new(format!("lock cannot call action '{}'", callee_name), span))
            }
            (Some(CallableKind::Lock), CallableKind::Lock) => {
                Err(CompileError::new(format!("lock cannot call lock '{}'", callee_name), span))
            }
            _ => Ok(()),
        }
    }

    /// 验证类型
    fn validate_type(&self, ty: &Type) -> Result<()> {
        match ty {
            Type::Unit => Ok(()),
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
            (Type::Unit, Type::Unit) => true,
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

    fn base_type_name<'b>(&self, ty: &'b Type) -> Option<&'b str> {
        match ty {
            Type::Named(name) => Some(name.split('<').next().unwrap_or(name.as_str())),
            Type::Ref(inner) | Type::MutRef(inner) => self.base_type_name(inner),
            _ => None,
        }
    }

    fn resolve_cell_type_kind(&self, name: &str) -> Option<CellTypeKind> {
        if let Some(kind) = self.cell_type_kinds.get(name).copied() {
            return Some(kind);
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(_) => Some(CellTypeKind::Resource),
            TypeDef::Shared(_) => Some(CellTypeKind::Shared),
            TypeDef::Receipt(_) => Some(CellTypeKind::Receipt),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn resolve_receipt_claim_output(&self, ty: &Type) -> Option<Type> {
        let type_name = self.base_type_name(ty)?;
        if let Some(output) = self.receipt_claim_outputs.get(type_name) {
            return output.clone();
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, type_name)? {
            TypeDef::Receipt(receipt) => receipt.claim_output,
            TypeDef::Resource(_) | TypeDef::Shared(_) | TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn validate_receipt_claim_output(&self, output: &Type, span: Span) -> Result<()> {
        let Some(type_name) = self.base_type_name(output) else {
            return Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span));
        };
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared) => Ok(()),
            Some(CellTypeKind::Receipt) => Err(CompileError::new("receipt claim output must not be another receipt", span)),
            None => Err(CompileError::new("receipt claim output must be a cell-backed resource or shared type", span)),
        }
    }

    fn resolve_capabilities(&self, name: &str) -> Option<HashSet<Capability>> {
        if let Some(capabilities) = self.type_capabilities.get(name) {
            return Some(capabilities.clone());
        }
        let (resolver, module) = (self.resolver?, self.current_module.as_ref()?);
        match resolver.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.capabilities.into_iter().collect()),
            TypeDef::Shared(shared) => Some(shared.capabilities.into_iter().collect()),
            TypeDef::Receipt(receipt) => Some(receipt.capabilities.into_iter().collect()),
            TypeDef::Struct(_) | TypeDef::Enum(_) => None,
        }
    }

    fn require_capability(&self, ty: &Type, capability: Capability, operation: &str, span: Span) -> Result<()> {
        let Some(type_name) = self.base_type_name(ty) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation), span));
        };
        let Some(capabilities) = self.resolve_capabilities(type_name) else {
            return Err(CompileError::new(format!("{} requires a cell-backed value", operation), span));
        };
        if capabilities.contains(&capability) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "type '{}' does not declare '{}' capability required by {}",
                    type_name,
                    capability_name(capability),
                    operation
                ),
                span,
            ))
        }
    }

    fn is_receipt_type(&self, ty: &Type) -> bool {
        self.base_type_name(ty).and_then(|name| self.resolve_cell_type_kind(name)).is_some_and(|kind| kind == CellTypeKind::Receipt)
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
        Expr::Assert(assert_expr) => assert_expr.span,
        Expr::Block(stmts) => stmts.last().map(stmt_span).unwrap_or_default(),
        Expr::Tuple(_) | Expr::Array(_) => Span::default(),
        Expr::If(if_expr) => if_expr.span,
        Expr::Cast(cast) => cast.span,
        Expr::Range(range) => range.span,
        Expr::StructInit(init) => init.span,
        Expr::Match(match_expr) => match_expr.span,
    }
}

fn assignment_root_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Identifier(name) => Some(name.as_str()),
        Expr::FieldAccess(field) => assignment_root_name(&field.expr),
        Expr::Index(index) => assignment_root_name(&index.expr),
        _ => None,
    }
}

fn capability_name(capability: Capability) -> &'static str {
    match capability {
        Capability::Store => "store",
        Capability::Transfer => "transfer",
        Capability::Destroy => "destroy",
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
