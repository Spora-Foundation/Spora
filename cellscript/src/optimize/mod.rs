//! 优化器
//!
//! 常量折叠、死代码消除、内联优化

use crate::ast::*;
use crate::error::Result;
use std::collections::HashMap;

/// 优化器
pub struct Optimizer {
    /// 常量表
    constants: HashMap<String, ConstValue>,
    /// 优化级别
    level: u8,
}

/// 常量值
#[derive(Debug, Clone)]
pub enum ConstValue {
    U64(u64),
    U128(u128),
    Bool(bool),
    String(String),
    Address([u8; 32]),
    Hash([u8; 32]),
}

impl Optimizer {
    /// 创建新的优化器
    pub fn new(level: u8) -> Self {
        Self { constants: HashMap::new(), level }
    }

    /// 优化模块
    pub fn optimize_module(&mut self, module: &mut Module) -> Result<()> {
        if self.level == 0 {
            return Ok(());
        }

        // 收集常量定义
        self.collect_constants(module);

        // 优化每个 item
        for item in &mut module.items {
            match item {
                Item::Action(action) => {
                    self.optimize_action(action)?;
                }
                Item::Lock(lock) => {
                    self.optimize_lock(lock)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 收集常量定义
    fn collect_constants(&mut self, module: &Module) {
        for item in &module.items {
            // 这里简化处理，实际应该解析 const 定义
            // 目前假设常量已经在解析阶段处理
        }
    }

    /// 优化 Action
    fn optimize_action(&mut self, action: &mut ActionDef) -> Result<()> {
        let mut optimized_body = Vec::new();

        for stmt in &action.body {
            if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                optimized_body.push(opt_stmt);
            }
        }

        action.body = optimized_body;

        Ok(())
    }

    /// 优化 Lock
    fn optimize_lock(&mut self, lock: &mut LockDef) -> Result<()> {
        lock.body = self.optimize_expr(&lock.body)?;
        Ok(())
    }

    /// 优化语句
    fn optimize_stmt(&mut self, stmt: &Stmt) -> Result<Option<Stmt>> {
        match stmt {
            Stmt::Let(let_stmt) => {
                let value = self.optimize_expr(&let_stmt.value)?;

                // 常量传播
                if let Some(const_val) = self.try_eval_const(&value) {
                    self.constants.insert(let_stmt.name.clone(), const_val);
                }

                Ok(Some(Stmt::Let(LetStmt { name: let_stmt.name.clone(), ty: let_stmt.ty.clone(), value, span: let_stmt.span })))
            }
            Stmt::Expr(expr) => {
                let opt_expr = self.optimize_expr(expr)?;

                // 死代码消除：纯表达式结果未被使用
                if self.is_pure(&opt_expr) && self.level >= 2 {
                    return Ok(None);
                }

                Ok(Some(Stmt::Expr(opt_expr)))
            }
            Stmt::If(if_stmt) => {
                let condition = self.optimize_expr(&if_stmt.condition)?;

                // 常量条件折叠
                if let Some(ConstValue::Bool(val)) = self.try_eval_const(&condition) {
                    if val {
                        // 条件为真，替换为 then 分支
                        return Ok(Some(Stmt::Block(if_stmt.then_branch.clone())));
                    } else if let Some(else_branch) = &if_stmt.else_branch {
                        // 条件为假，替换为 else 分支
                        return Ok(Some(Stmt::Block(else_branch.clone())));
                    } else {
                        // 条件为假，无 else，删除
                        return Ok(None);
                    }
                }

                let mut then_branch = Vec::new();
                for stmt in &if_stmt.then_branch {
                    if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                        then_branch.push(opt_stmt);
                    }
                }

                let else_branch = if let Some(else_stmts) = &if_stmt.else_branch {
                    let mut opt_else = Vec::new();
                    for stmt in else_stmts {
                        if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                            opt_else.push(opt_stmt);
                        }
                    }
                    if opt_else.is_empty() {
                        None
                    } else {
                        Some(opt_else)
                    }
                } else {
                    None
                };

                Ok(Some(Stmt::If(IfStmt { condition, then_branch, else_branch, span: if_stmt.span })))
            }
            Stmt::For(for_stmt) => {
                let iterable = self.optimize_expr(&for_stmt.iterable)?;
                let mut body = Vec::new();
                for stmt in &for_stmt.body {
                    if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                        body.push(opt_stmt);
                    }
                }

                Ok(Some(Stmt::For(ForStmt { var: for_stmt.var.clone(), iterable, body, span: for_stmt.span })))
            }
            Stmt::While(while_stmt) => {
                let condition = self.optimize_expr(&while_stmt.condition)?;

                // 常量条件折叠
                if let Some(ConstValue::Bool(val)) = self.try_eval_const(&condition) {
                    if !val {
                        // 条件永远为假，删除循环
                        return Ok(None);
                    }
                    // 条件永远为真，保留但警告可能是无限循环
                }

                let mut body = Vec::new();
                for stmt in &while_stmt.body {
                    if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                        body.push(opt_stmt);
                    }
                }

                Ok(Some(Stmt::While(WhileStmt { condition, body, span: while_stmt.span })))
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    Ok(Some(Stmt::Return(Some(self.optimize_expr(expr)?))))
                } else {
                    Ok(Some(Stmt::Return(None)))
                }
            }
            Stmt::Block(stmts) => {
                let mut opt_stmts = Vec::new();
                for stmt in stmts {
                    if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                        opt_stmts.push(opt_stmt);
                    }
                }
                Ok(Some(Stmt::Block(opt_stmts)))
            }
            _ => Ok(Some(stmt.clone())),
        }
    }

    /// 优化表达式
    fn optimize_expr(&mut self, expr: &Expr) -> Result<Expr> {
        match expr {
            Expr::Binary(bin) => {
                let left = self.optimize_expr(&bin.left)?;
                let right = self.optimize_expr(&bin.right)?;

                // 常量折叠
                if let (Some(l), Some(r)) = (self.try_eval_const(&left), self.try_eval_const(&right)) {
                    if let Some(result) = self.fold_binary(&bin.op, &l, &r) {
                        return Ok(self.const_to_expr(&result, expr.span()));
                    }
                }

                // 代数简化
                if let Some(simplified) = self.simplify_algebra(&bin.op, &left, &right) {
                    return Ok(simplified);
                }

                Ok(Expr::Binary(BinaryExpr { left: Box::new(left), op: bin.op.clone(), right: Box::new(right), span: bin.span }))
            }
            Expr::Unary(unary) => {
                let expr = self.optimize_expr(&unary.expr)?;

                // 常量折叠
                if let Some(val) = self.try_eval_const(&expr) {
                    if let Some(result) = self.fold_unary(&unary.op, &val) {
                        return Ok(self.const_to_expr(&result, unary.span));
                    }
                }

                // 双重否定消除
                if unary.op == UnaryOp::Not {
                    if let Expr::Unary(inner) = &expr {
                        if inner.op == UnaryOp::Not {
                            return Ok(inner.expr.as_ref().clone());
                        }
                    }
                }

                Ok(Expr::Unary(UnaryExpr { op: unary.op.clone(), expr: Box::new(expr), span: unary.span }))
            }
            Expr::Call(call) => {
                let mut opt_args = Vec::new();
                for arg in &call.args {
                    opt_args.push(self.optimize_expr(arg)?);
                }

                Ok(Expr::Call(CallExpr { func: call.func.clone(), args: opt_args, span: call.span }))
            }
            Expr::FieldAccess(field) => Ok(Expr::FieldAccess(FieldAccessExpr {
                expr: Box::new(self.optimize_expr(&field.expr)?),
                field: field.field.clone(),
                span: field.span,
            })),
            Expr::Index(index) => Ok(Expr::Index(IndexExpr {
                expr: Box::new(self.optimize_expr(&index.expr)?),
                index: Box::new(self.optimize_expr(&index.index)?),
                span: index.span,
            })),
            Expr::Tuple(elems) => {
                let mut opt_elems = Vec::new();
                for elem in elems {
                    opt_elems.push(self.optimize_expr(elem)?);
                }
                Ok(Expr::Tuple(opt_elems))
            }
            Expr::Array(elems) => {
                let mut opt_elems = Vec::new();
                for elem in elems {
                    opt_elems.push(self.optimize_expr(elem)?);
                }
                Ok(Expr::Array(opt_elems))
            }
            Expr::Block(stmts) => {
                let mut opt_stmts = Vec::new();
                for stmt in stmts {
                    if let Some(opt_stmt) = self.optimize_stmt(stmt)? {
                        opt_stmts.push(opt_stmt);
                    }
                }
                Ok(Expr::Block(opt_stmts))
            }
            Expr::If(if_expr) => {
                let condition = self.optimize_expr(&if_expr.condition)?;

                // 常量条件折叠
                if let Some(ConstValue::Bool(val)) = self.try_eval_const(&condition) {
                    if val {
                        return Ok(Expr::Block(if_expr.then_branch.clone()));
                    } else if let Some(else_branch) = &if_expr.else_branch {
                        return Ok(Expr::Block(else_branch.clone()));
                    }
                }

                Ok(Expr::If(IfExpr {
                    condition: Box::new(condition),
                    then_branch: if_expr.then_branch.clone(),
                    else_branch: if_expr.else_branch.clone(),
                    span: if_expr.span,
                }))
            }
            Expr::Path(path) => {
                // 常量传播
                if let Some(const_val) = self.constants.get(&path.name) {
                    return Ok(self.const_to_expr(const_val, path.span));
                }
                Ok(Expr::Path(path.clone()))
            }
            _ => Ok(expr.clone()),
        }
    }

    /// 尝试求值为常量
    fn try_eval_const(&self, expr: &Expr) -> Option<ConstValue> {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::U64(n) => Some(ConstValue::U64(*n)),
                Literal::U128(n) => Some(ConstValue::U128(*n)),
                Literal::Bool(b) => Some(ConstValue::Bool(*b)),
                Literal::String(s) => Some(ConstValue::String(s.clone())),
                _ => None,
            },
            Expr::Path(path) => self.constants.get(&path.name).cloned(),
            _ => None,
        }
    }

    /// 折叠二元运算
    fn fold_binary(&self, op: &BinaryOp, left: &ConstValue, right: &ConstValue) -> Option<ConstValue> {
        use ConstValue::*;

        match (op, left, right) {
            (BinaryOp::Add, U64(l), U64(r)) => Some(U64(l.wrapping_add(*r))),
            (BinaryOp::Sub, U64(l), U64(r)) => Some(U64(l.wrapping_sub(*r))),
            (BinaryOp::Mul, U64(l), U64(r)) => Some(U64(l.wrapping_mul(*r))),
            (BinaryOp::Div, U64(l), U64(r)) => {
                if *r == 0 {
                    None
                } else {
                    Some(U64(l / r))
                }
            }
            (BinaryOp::Mod, U64(l), U64(r)) => {
                if *r == 0 {
                    None
                } else {
                    Some(U64(l % r))
                }
            }
            (BinaryOp::And, U64(l), U64(r)) => Some(U64(l & r)),
            (BinaryOp::Or, U64(l), U64(r)) => Some(U64(l | r)),
            (BinaryOp::Xor, U64(l), U64(r)) => Some(U64(l ^ r)),
            (BinaryOp::Eq, U64(l), U64(r)) => Some(Bool(l == r)),
            (BinaryOp::Ne, U64(l), U64(r)) => Some(Bool(l != r)),
            (BinaryOp::Lt, U64(l), U64(r)) => Some(Bool(l < r)),
            (BinaryOp::Le, U64(l), U64(r)) => Some(Bool(l <= r)),
            (BinaryOp::Gt, U64(l), U64(r)) => Some(Bool(l > r)),
            (BinaryOp::Ge, U64(l), U64(r)) => Some(Bool(l >= r)),
            (BinaryOp::And, Bool(l), Bool(r)) => Some(Bool(*l && *r)),
            (BinaryOp::Or, Bool(l), Bool(r)) => Some(Bool(*l || *r)),
            (BinaryOp::Eq, Bool(l), Bool(r)) => Some(Bool(l == r)),
            (BinaryOp::Ne, Bool(l), Bool(r)) => Some(Bool(l != r)),
            _ => None,
        }
    }

    /// 折叠一元运算
    fn fold_unary(&self, op: &UnaryOp, expr: &ConstValue) -> Option<ConstValue> {
        use ConstValue::*;

        match (op, expr) {
            (UnaryOp::Not, Bool(b)) => Some(Bool(!b)),
            (UnaryOp::Neg, U64(n)) => Some(U64(n.wrapping_neg())),
            _ => None,
        }
    }

    /// 代数简化
    fn simplify_algebra(&self, op: &BinaryOp, left: &Expr, right: &Expr) -> Option<Expr> {
        use BinaryOp::*;

        match (op, left, right) {
            // x + 0 = x
            (Add, _, Expr::Literal(Literal::U64(0))) => Some(left.clone()),
            // 0 + x = x
            (Add, Expr::Literal(Literal::U64(0)), _) => Some(right.clone()),
            // x - 0 = x
            (Sub, _, Expr::Literal(Literal::U64(0))) => Some(left.clone()),
            // x * 0 = 0
            (Mul, _, Expr::Literal(Literal::U64(0))) => Some(Expr::Literal(Literal::U64(0))),
            // 0 * x = 0
            (Mul, Expr::Literal(Literal::U64(0)), _) => Some(Expr::Literal(Literal::U64(0))),
            // x * 1 = x
            (Mul, _, Expr::Literal(Literal::U64(1))) => Some(left.clone()),
            // 1 * x = x
            (Mul, Expr::Literal(Literal::U64(1)), _) => Some(right.clone()),
            // x / 1 = x
            (Div, _, Expr::Literal(Literal::U64(1))) => Some(left.clone()),
            // x & 0 = 0
            (And, _, Expr::Literal(Literal::U64(0))) => Some(Expr::Literal(Literal::U64(0))),
            // 0 & x = 0
            (And, Expr::Literal(Literal::U64(0)), _) => Some(Expr::Literal(Literal::U64(0))),
            // x | 0 = x
            (Or, _, Expr::Literal(Literal::U64(0))) => Some(left.clone()),
            // 0 | x = x
            (Or, Expr::Literal(Literal::U64(0)), _) => Some(right.clone()),
            // x ^ 0 = x
            (Xor, _, Expr::Literal(Literal::U64(0))) => Some(left.clone()),
            // 0 ^ x = x
            (Xor, Expr::Literal(Literal::U64(0)), _) => Some(right.clone()),
            // x ^ x = 0
            (Xor, l, r) if l == r => Some(Expr::Literal(Literal::U64(0))),
            _ => None,
        }
    }

    /// 检查表达式是否为纯表达式（无副作用）
    fn is_pure(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(_) | Expr::Path(_) => true,
            Expr::Binary(bin) => self.is_pure(&bin.left) && self.is_pure(&bin.right),
            Expr::Unary(unary) => self.is_pure(&unary.expr),
            Expr::Tuple(elems) | Expr::Array(elems) => elems.iter().all(|e| self.is_pure(e)),
            Expr::FieldAccess(field) => self.is_pure(&field.expr),
            Expr::Index(index) => self.is_pure(&index.expr) && self.is_pure(&index.index),
            _ => false, // create, destroy, transfer, call 等可能有副作用
        }
    }

    /// 常量转换为表达式
    fn const_to_expr(&self, val: &ConstValue, span: Span) -> Expr {
        match val {
            ConstValue::U64(n) => Expr::Literal(Literal::U64(*n)),
            ConstValue::U128(n) => Expr::Literal(Literal::U128(*n)),
            ConstValue::Bool(b) => Expr::Literal(Literal::Bool(*b)),
            ConstValue::String(s) => Expr::Literal(Literal::String(s.clone())),
            ConstValue::Address(a) => Expr::Literal(Literal::Address(*a)),
            ConstValue::Hash(h) => Expr::Literal(Literal::Hash(*h)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_folding() {
        let mut optimizer = Optimizer::new(1);

        // 测试 2 + 3 = 5
        let expr = Expr::Binary(BinaryExpr {
            left: Box::new(Expr::Literal(Literal::U64(2))),
            op: BinaryOp::Add,
            right: Box::new(Expr::Literal(Literal::U64(3))),
            span: Span::default(),
        });

        let result = optimizer.optimize_expr(&expr).unwrap();
        assert_eq!(result, Expr::Literal(Literal::U64(5)));
    }

    #[test]
    fn test_algebraic_simplification() {
        let mut optimizer = Optimizer::new(1);

        // 测试 x + 0 = x
        let expr = Expr::Binary(BinaryExpr {
            left: Box::new(Expr::Path(PathExpr { name: "x".to_string(), span: Span::default() })),
            op: BinaryOp::Add,
            right: Box::new(Expr::Literal(Literal::U64(0))),
            span: Span::default(),
        });

        let result = optimizer.optimize_expr(&expr).unwrap();
        assert_eq!(result, Expr::Path(PathExpr { name: "x".to_string(), span: Span::default() }));
    }

    #[test]
    fn test_boolean_folding() {
        let mut optimizer = Optimizer::new(1);

        // 测试 true && false = false
        let expr = Expr::Binary(BinaryExpr {
            left: Box::new(Expr::Literal(Literal::Bool(true))),
            op: BinaryOp::And,
            right: Box::new(Expr::Literal(Literal::Bool(false))),
            span: Span::default(),
        });

        let result = optimizer.optimize_expr(&expr).unwrap();
        assert_eq!(result, Expr::Literal(Literal::Bool(false)));
    }
}
