//! 生命周期验证
//!
//! 验证 #[lifecycle(...)] 属性的正确性

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

/// 生命周期验证器
pub struct LifecycleChecker {
    /// 已定义的生命周期状态
    states: HashMap<String, Vec<String>>,
    /// 状态转换图
    transitions: HashMap<String, HashMap<String, Vec<TransitionRule>>>,
}

/// 状态转换规则
#[derive(Debug, Clone)]
pub struct TransitionRule {
    pub from: String,
    pub to: String,
    pub condition: Option<String>, // 转换条件的表达式字符串
}

/// 资源生命周期信息
#[derive(Debug, Clone)]
pub struct LifecycleInfo {
    pub resource_name: String,
    pub states: Vec<String>,
    pub initial_state: String,
    pub final_states: Vec<String>,
}

impl LifecycleChecker {
    /// 创建新的生命周期验证器
    pub fn new() -> Self {
        Self { states: HashMap::new(), transitions: HashMap::new() }
    }

    /// 注册资源的生命周期
    pub fn register_lifecycle(&mut self, resource_name: &str, lifecycle: &Lifecycle) -> Result<()> {
        let states = lifecycle.states.clone();

        // 验证状态数量
        if states.len() < 2 {
            return Err(CompileError::new("lifecycle must have at least 2 states", lifecycle.span));
        }

        // 检查重复状态
        let mut seen = HashSet::new();
        for state in &states {
            if !seen.insert(state.clone()) {
                return Err(CompileError::new(format!("duplicate lifecycle state: {}", state), lifecycle.span));
            }
        }

        // 构建转换规则（只允许前向转换）
        let mut transitions = HashMap::new();
        for i in 0..states.len() - 1 {
            let from = states[i].clone();
            let to = states[i + 1].clone();

            transitions.entry(from.clone()).or_insert_with(Vec::new).push(TransitionRule {
                from: from.clone(),
                to: to.clone(),
                condition: None,
            });
        }

        self.states.insert(resource_name.to_string(), states);
        self.transitions.insert(resource_name.to_string(), transitions);

        Ok(())
    }

    /// 验证状态转换
    pub fn validate_transition(&self, resource_name: &str, from: &str, to: &str, span: Span) -> Result<()> {
        let states = self
            .states
            .get(resource_name)
            .ok_or_else(|| CompileError::new(format!("resource '{}' has no lifecycle defined", resource_name), span))?;

        // 检查状态是否存在
        if !states.contains(&from.to_string()) {
            return Err(CompileError::new(format!("invalid from state: {}", from), span));
        }

        if !states.contains(&to.to_string()) {
            return Err(CompileError::new(format!("invalid to state: {}", to), span));
        }

        // 获取允许的转换
        let transitions = self.transitions.get(resource_name).unwrap();

        if let Some(allowed) = transitions.get(from) {
            if allowed.iter().any(|t| t.to == to) {
                return Ok(());
            }
        }

        // 检查是否尝试反向转换
        let from_idx = states.iter().position(|s| s == from).unwrap();
        let to_idx = states.iter().position(|s| s == to).unwrap();

        if to_idx < from_idx {
            return Err(CompileError::new(format!("invalid lifecycle transition: cannot go from '{}' back to '{}'", from, to), span));
        }

        if to_idx == from_idx {
            return Err(CompileError::new(format!("invalid lifecycle transition: '{}' to itself", from), span));
        }

        // 跳过中间状态的转换
        Err(CompileError::new(format!("invalid lifecycle transition: cannot skip from '{}' to '{}'", from, to), span))
    }

    /// 获取生命周期信息
    pub fn get_lifecycle_info(&self, resource_name: &str) -> Option<LifecycleInfo> {
        let states = self.states.get(resource_name)?;

        Some(LifecycleInfo {
            resource_name: resource_name.to_string(),
            states: states.clone(),
            initial_state: states.first()?.clone(),
            final_states: vec![states.last()?.clone()],
        })
    }

    /// 检查资源是否已到达最终状态
    pub fn is_final_state(&self, resource_name: &str, state: &str) -> bool {
        if let Some(states) = self.states.get(resource_name) {
            if let Some(last) = states.last() {
                return last == state;
            }
        }
        false
    }

    /// 获取下一个可能的状态
    pub fn get_next_states(&self, resource_name: &str, from: &str) -> Vec<String> {
        let mut next_states = Vec::new();

        if let Some(transitions) = self.transitions.get(resource_name) {
            if let Some(rules) = transitions.get(from) {
                for rule in rules {
                    next_states.push(rule.to.clone());
                }
            }
        }

        next_states
    }

    /// 验证 Action 中的生命周期使用
    pub fn validate_action(&self, action: &ActionDef) -> Result<()> {
        // 简化实现：检查是否有状态字段的赋值
        for stmt in &action.body {
            self.validate_stmt(stmt)?;
        }

        Ok(())
    }

    /// 验证语句
    fn validate_stmt(&self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::Let(let_stmt) => {
                self.validate_expr(&let_stmt.value)?;
            }
            Stmt::Expr(expr) => {
                self.validate_expr(expr)?;
            }
            Stmt::If(if_stmt) => {
                self.validate_expr(&if_stmt.condition)?;
                for stmt in &if_stmt.then_branch {
                    self.validate_stmt(stmt)?;
                }
                if let Some(else_branch) = &if_stmt.else_branch {
                    for stmt in else_branch {
                        self.validate_stmt(stmt)?;
                    }
                }
            }
            Stmt::For(for_stmt) => {
                self.validate_expr(&for_stmt.iterable)?;
                for stmt in &for_stmt.body {
                    self.validate_stmt(stmt)?;
                }
            }
            Stmt::While(while_stmt) => {
                self.validate_expr(&while_stmt.condition)?;
                for stmt in &while_stmt.body {
                    self.validate_stmt(stmt)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// 验证表达式
    fn validate_expr(&self, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Create(create) => {
                // 检查创建的资源是否有生命周期
                // 如果有，验证初始状态
            }
            Expr::Binary(bin) => {
                self.validate_expr(&bin.left)?;
                self.validate_expr(&bin.right)?;
            }
            Expr::Unary(unary) => {
                self.validate_expr(&unary.expr)?;
            }
            Expr::Call(call) => {
                for arg in &call.args {
                    self.validate_expr(arg)?;
                }
            }
            Expr::FieldAccess(field) => {
                self.validate_expr(&field.expr)?;
            }
            Expr::Index(index) => {
                self.validate_expr(&index.expr)?;
                self.validate_expr(&index.index)?;
            }
            Expr::Block(stmts) => {
                for stmt in stmts {
                    self.validate_stmt(stmt)?;
                }
            }
            Expr::Tuple(elems) | Expr::Array(elems) => {
                for elem in elems {
                    self.validate_expr(elem)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// 生成生命周期验证代码
    pub fn generate_validation_code(&self, resource_name: &str) -> String {
        let mut code = String::new();

        code.push_str(&format!("// Lifecycle validation for {}\n", resource_name));

        if let Some(states) = self.states.get(resource_name) {
            code.push_str("// Valid states:\n");
            for (i, state) in states.iter().enumerate() {
                code.push_str(&format!("//   {}: {}\n", i, state));
            }

            code.push_str("\n// Valid transitions:\n");
            if let Some(transitions) = self.transitions.get(resource_name) {
                for (from, rules) in transitions {
                    for rule in rules {
                        code.push_str(&format!("//   {} -> {}\n", from, rule.to));
                    }
                }
            }
        }

        code
    }
}

/// 从 ReceiptDef 提取生命周期
pub fn extract_lifecycle(receipt: &ReceiptDef) -> Option<&Lifecycle> {
    receipt.lifecycle.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_registration() {
        let mut checker = LifecycleChecker::new();

        let lifecycle =
            Lifecycle { states: vec!["Created".to_string(), "Active".to_string(), "Settled".to_string()], span: Span::default() };

        checker.register_lifecycle("VestingGrant", &lifecycle).unwrap();

        // 验证有效转换
        assert!(checker.validate_transition("VestingGrant", "Created", "Active", Span::default()).is_ok());
        assert!(checker.validate_transition("VestingGrant", "Active", "Settled", Span::default()).is_ok());

        // 验证无效转换
        assert!(checker.validate_transition("VestingGrant", "Settled", "Active", Span::default()).is_err());
        assert!(checker.validate_transition("VestingGrant", "Created", "Settled", Span::default()).is_err());
    }

    #[test]
    fn test_lifecycle_info() {
        let mut checker = LifecycleChecker::new();

        let lifecycle = Lifecycle {
            states: vec!["Granted".to_string(), "Claimable".to_string(), "FullyClaimed".to_string()],
            span: Span::default(),
        };

        checker.register_lifecycle("Grant", &lifecycle).unwrap();

        let info = checker.get_lifecycle_info("Grant").unwrap();
        assert_eq!(info.initial_state, "Granted");
        assert_eq!(info.final_states, vec!["FullyClaimed"]);
        assert!(checker.is_final_state("Grant", "FullyClaimed"));
        assert!(!checker.is_final_state("Grant", "Granted"));
    }
}
