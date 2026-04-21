//! AST 级别的模拟解释器
//!
//! 在没有 ckb-vm 的环境中，对纯计算 action/fn 进行符号化模拟执行。
//! 产生执行追踪 (trace) 而非实际字节码执行结果。
//! 对涉及 Cell 操作 (create/consume/transfer/destroy/read_ref) 的表达式，
//! 记录为 trace 事件并返回占位值。

use crate::ast::*;
use std::collections::HashMap;

/// 模拟值
#[derive(Debug, Clone, PartialEq)]
pub enum SimValue {
    /// 整数
    Integer(u64),
    /// 布尔
    Bool(bool),
    /// 字符串
    String(String),
    /// 单元
    Unit,
    /// 符号占位值 (Cell 操作结果)
    Symbolic { ty: String, description: String },
    /// 结构体值
    Struct { name: String, fields: Vec<(String, SimValue)> },
    /// 数组
    Array(Vec<SimValue>),
    /// 元组
    Tuple(Vec<SimValue>),
}

impl std::fmt::Display for SimValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimValue::Integer(n) => write!(f, "{}", n),
            SimValue::Bool(b) => write!(f, "{}", b),
            SimValue::String(s) => write!(f, "\"{}\"", s),
            SimValue::Unit => write!(f, "()"),
            SimValue::Symbolic { ty, description } => write!(f, "<simulated {} ({})>", ty, description),
            SimValue::Struct { name, fields } => {
                write!(f, "{} {{ ", name)?;
                let parts: Vec<String> = fields.iter().map(|(n, v)| format!("{}: {}", n, v)).collect();
                write!(f, "{}", parts.join(", "))?;
                write!(f, " }}")
            }
            SimValue::Array(items) => {
                write!(f, "[")?;
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", parts.join(", "))?;
                write!(f, "]")
            }
            SimValue::Tuple(items) => {
                write!(f, "(")?;
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                write!(f, "{}", parts.join(", "))?;
                write!(f, ")")
            }
        }
    }
}

/// 追踪事件
#[derive(Debug, Clone)]
pub enum TraceEvent {
    /// 变量绑定
    Bind { name: String, value: SimValue },
    /// Cell 创建
    Create { ty: String, fields: Vec<(String, String)> },
    /// Cell 消费
    Consume { description: String },
    /// Cell 转移
    Transfer { description: String, to: String },
    /// Cell 销毁
    Destroy { description: String },
    /// 读取共享引用
    ReadRef { ty: String },
    /// Claim 操作
    Claim { description: String },
    /// Settle 操作
    Settle { description: String },
    /// 函数调用
    Call { name: String, args: Vec<String> },
    /// 返回值
    Return { value: SimValue },
    /// 条件分支
    Branch { condition: SimValue, taken: bool },
    /// 断言
    Assert { condition: SimValue, message: String },
}

impl std::fmt::Display for TraceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceEvent::Bind { name, value } => write!(f, "  let {} = {}", name, value),
            TraceEvent::Create { ty, fields } => {
                write!(f, "  create {} {{ ", ty)?;
                let parts: Vec<String> = fields.iter().map(|(n, v)| format!("{}: {}", n, v)).collect();
                write!(f, "{}", parts.join(", "))?;
                write!(f, " }}")
            }
            TraceEvent::Consume { description } => write!(f, "  consume {}", description),
            TraceEvent::Transfer { description, to } => write!(f, "  transfer {} to {}", description, to),
            TraceEvent::Destroy { description } => write!(f, "  destroy {}", description),
            TraceEvent::ReadRef { ty } => write!(f, "  read_ref<{}>()", ty),
            TraceEvent::Claim { description } => write!(f, "  claim {}", description),
            TraceEvent::Settle { description } => write!(f, "  settle {}", description),
            TraceEvent::Call { name, args } => write!(f, "  call {}({})", name, args.join(", ")),
            TraceEvent::Return { value } => write!(f, "  return {}", value),
            TraceEvent::Branch { condition, taken } => write!(f, "  if {} -> {}", condition, if *taken { "then" } else { "else" }),
            TraceEvent::Assert { condition, message } => write!(f, "  assert!({}, \"{}\")", condition, message),
        }
    }
}

/// 模拟结果
#[derive(Debug, Clone)]
pub struct SimulateResult {
    /// 入口名称
    pub entry_name: String,
    /// 返回值
    pub return_value: SimValue,
    /// 追踪事件
    pub trace: Vec<TraceEvent>,
    /// 是否包含 Cell 操作 (非纯计算)
    pub has_cell_ops: bool,
    /// 模拟的步骤数
    pub steps: u64,
}

impl std::fmt::Display for SimulateResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Simulated entry: {}", self.entry_name)?;
        writeln!(f, "Steps: {}", self.steps)?;
        if self.has_cell_ops {
            writeln!(f, "Cell operations: yes (symbolic)")?;
        } else {
            writeln!(f, "Cell operations: none (pure computation)")?;
        }
        writeln!(f, "Trace:")?;
        for event in &self.trace {
            writeln!(f, "{}", event)?;
        }
        write!(f, "Result: {}", self.return_value)
    }
}

/// 模拟解释器
pub struct SimulateInterpreter {
    /// 环境 (变量绑定)
    env: HashMap<String, SimValue>,
    /// 追踪事件
    trace: Vec<TraceEvent>,
    /// 函数定义
    functions: HashMap<String, (Vec<Param>, Vec<Stmt>)>,
    /// 是否包含 Cell 操作
    has_cell_ops: bool,
    /// 步骤计数
    steps: u64,
    /// 最大步数限制
    max_steps: u64,
}

/// 模拟执行错误
#[derive(Debug, Clone)]
pub enum SimulateError {
    /// 超出步数限制
    StepLimitExceeded { max: u64 },
    /// 未定义的变量
    UndefinedVariable { name: String },
    /// 未定义的函数
    UndefinedFunction { name: String },
    /// 类型不匹配
    TypeError { expected: String, got: String },
    /// 不支持的表达式
    Unsupported { description: String },
}

impl std::fmt::Display for SimulateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimulateError::StepLimitExceeded { max } => write!(f, "simulation exceeded maximum step limit ({})", max),
            SimulateError::UndefinedVariable { name } => write!(f, "undefined variable '{}'", name),
            SimulateError::UndefinedFunction { name } => write!(f, "undefined function '{}'", name),
            SimulateError::TypeError { expected, got } => write!(f, "type error: expected {}, got {}", expected, got),
            SimulateError::Unsupported { description } => write!(f, "unsupported: {}", description),
        }
    }
}

impl SimulateInterpreter {
    /// 创建新的模拟解释器
    pub fn new(module: &Module, max_steps: u64) -> Self {
        let mut functions = HashMap::new();

        for item in &module.items {
            match item {
                Item::Function(f) => {
                    functions.insert(f.name.clone(), (f.params.clone(), f.body.clone()));
                }
                Item::Action(a) => {
                    functions.insert(format!("action::{}", a.name), (a.params.clone(), a.body.clone()));
                }
                Item::Lock(l) => {
                    functions.insert(format!("lock::{}", l.name), (l.params.clone(), l.body.clone()));
                }
                _ => {}
            }
        }

        Self { env: HashMap::new(), trace: Vec::new(), functions, has_cell_ops: false, steps: 0, max_steps }
    }

    /// 模拟执行 action
    pub fn simulate_action(&mut self, name: &str, args: &[SimValue]) -> Result<SimulateResult, SimulateError> {
        let key = format!("action::{}", name);
        let (params, body) = self.functions.get(&key).cloned().ok_or_else(|| SimulateError::UndefinedFunction { name: key })?;

        // 绑定参数
        for (param, arg) in params.iter().zip(args.iter()) {
            self.env.insert(param.name.clone(), arg.clone());
        }

        // 执行函数体
        let result = self.exec_stmts(&body)?;
        let return_value = result.unwrap_or(SimValue::Unit);

        Ok(SimulateResult {
            entry_name: name.to_string(),
            return_value: return_value.clone(),
            trace: self.trace.clone(),
            has_cell_ops: self.has_cell_ops,
            steps: self.steps,
        })
    }

    /// 模拟执行 fn
    pub fn simulate_function(&mut self, name: &str, args: &[SimValue]) -> Result<SimValue, SimulateError> {
        let (params, body) =
            self.functions.get(name).cloned().ok_or_else(|| SimulateError::UndefinedFunction { name: name.to_string() })?;

        // 保存当前环境
        let saved_env = self.env.clone();
        self.env.clear();

        // 绑定参数
        for (param, arg) in params.iter().zip(args.iter()) {
            self.env.insert(param.name.clone(), arg.clone());
        }

        let result = self.exec_stmts(&body)?;
        let value = result.unwrap_or(SimValue::Unit);

        // 恢复环境
        self.env = saved_env;

        Ok(value)
    }

    /// 执行语句列表
    fn exec_stmts(&mut self, stmts: &[Stmt]) -> Result<Option<SimValue>, SimulateError> {
        for stmt in stmts {
            if let Some(value) = self.exec_stmt(stmt)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    /// 执行单条语句
    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Option<SimValue>, SimulateError> {
        self.bump_steps()?;

        match stmt {
            Stmt::Let(let_stmt) => {
                let value = self.eval_expr(&let_stmt.value)?;
                self.bind_pattern(&let_stmt.pattern, value.clone());
                Ok(None)
            }
            Stmt::Return(expr) => {
                let value = match expr {
                    Some(e) => self.eval_expr(e)?,
                    None => SimValue::Unit,
                };
                self.trace.push(TraceEvent::Return { value: value.clone() });
                Ok(Some(value))
            }
            Stmt::Expr(expr) => {
                let value = self.eval_expr(expr)?;
                Ok(if matches!(value, SimValue::Unit) { None } else { Some(value) })
            }
            Stmt::If(if_stmt) => {
                let cond = self.eval_expr(&if_stmt.condition)?;
                let taken = self.is_truthy(&cond);
                self.trace.push(TraceEvent::Branch { condition: cond, taken });
                if taken {
                    self.exec_stmts(&if_stmt.then_branch)
                } else if let Some(else_branch) = &if_stmt.else_branch {
                    self.exec_stmts(else_branch)
                } else {
                    Ok(None)
                }
            }
            Stmt::For(for_stmt) => {
                // 简单模拟：执行最多 10 次迭代
                let iterable = self.eval_expr(&for_stmt.iterable)?;
                let items = match &iterable {
                    SimValue::Array(items) => items.clone(),
                    _ => vec![iterable],
                };
                for item in items.iter().take(10) {
                    self.bind_pattern(&for_stmt.pattern, item.clone());
                    if let Some(value) = self.exec_stmts(&for_stmt.body)? {
                        return Ok(Some(value));
                    }
                }
                Ok(None)
            }
            Stmt::While(while_stmt) => {
                // 限制最多 100 次迭代
                for _ in 0..100 {
                    self.bump_steps()?;
                    let cond = self.eval_expr(&while_stmt.condition)?;
                    if !self.is_truthy(&cond) {
                        break;
                    }
                    if let Some(value) = self.exec_stmts(&while_stmt.body)? {
                        return Ok(Some(value));
                    }
                }
                Ok(None)
            }
        }
    }

    /// 求值表达式
    fn eval_expr(&mut self, expr: &Expr) -> Result<SimValue, SimulateError> {
        self.bump_steps()?;

        match expr {
            Expr::Integer(n) => Ok(SimValue::Integer(*n)),
            Expr::Bool(b) => Ok(SimValue::Bool(*b)),
            Expr::String(s) => Ok(SimValue::String(s.clone())),
            Expr::ByteString(bytes) => Ok(SimValue::String(format!("0x{}", crate::hex_encode(bytes)))),
            Expr::Identifier(name) => {
                self.env.get(name).cloned().ok_or_else(|| SimulateError::UndefinedVariable { name: name.clone() })
            }
            Expr::Binary(bin) => {
                let left = self.eval_expr(&bin.left)?;
                let right = self.eval_expr(&bin.right)?;
                self.eval_binary(&bin.op, &left, &right)
            }
            Expr::Unary(unary) => {
                let value = self.eval_expr(&unary.expr)?;
                self.eval_unary(&unary.op, &value)
            }
            Expr::Call(call) => self.eval_call(call),
            Expr::FieldAccess(access) => {
                let obj = self.eval_expr(&access.expr)?;
                match &obj {
                    SimValue::Struct { fields, .. } => fields
                        .iter()
                        .find(|(n, _)| n == &access.field)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| SimulateError::UndefinedVariable { name: access.field.clone() }),
                    _ => Ok(SimValue::Symbolic { ty: "field".to_string(), description: format!("{}.{}", obj, access.field) }),
                }
            }
            Expr::Index(index) => {
                let obj = self.eval_expr(&index.expr)?;
                let idx = self.eval_expr(&index.index)?;
                match (&obj, &idx) {
                    (SimValue::Array(items), SimValue::Integer(i)) => {
                        items.get(*i as usize).cloned().ok_or_else(|| SimulateError::UndefinedVariable { name: format!("[{}]", i) })
                    }
                    _ => Ok(SimValue::Symbolic { ty: "index".to_string(), description: format!("{}[{}]", obj, idx) }),
                }
            }
            Expr::Create(create) => {
                self.has_cell_ops = true;
                let fields: Vec<(String, String)> = create
                    .fields
                    .iter()
                    .map(|(name, expr)| {
                        let value = self
                            .eval_expr(expr)
                            .unwrap_or_else(|_| SimValue::Symbolic { ty: "unknown".to_string(), description: "...".to_string() });
                        (name.clone(), value.to_string())
                    })
                    .collect();
                self.trace.push(TraceEvent::Create { ty: create.ty.clone(), fields: fields.clone() });
                Ok(SimValue::Symbolic { ty: create.ty.clone(), description: "created cell".to_string() })
            }
            Expr::Consume(consume) => {
                self.has_cell_ops = true;
                let value = self.eval_expr(&consume.expr)?;
                let desc = value.to_string();
                self.trace.push(TraceEvent::Consume { description: desc.clone() });
                Ok(SimValue::Symbolic { ty: "consumed".to_string(), description: desc })
            }
            Expr::Transfer(transfer) => {
                self.has_cell_ops = true;
                let value = self.eval_expr(&transfer.expr)?;
                let to = self.eval_expr(&transfer.to)?;
                self.trace.push(TraceEvent::Transfer { description: value.to_string(), to: to.to_string() });
                Ok(SimValue::Symbolic { ty: "transferred".to_string(), description: "transfer".to_string() })
            }
            Expr::Destroy(destroy) => {
                self.has_cell_ops = true;
                let value = self.eval_expr(&destroy.expr)?;
                self.trace.push(TraceEvent::Destroy { description: value.to_string() });
                Ok(SimValue::Unit)
            }
            Expr::ReadRef(read_ref) => {
                self.has_cell_ops = true;
                self.trace.push(TraceEvent::ReadRef { ty: read_ref.ty.clone() });
                Ok(SimValue::Symbolic { ty: read_ref.ty.clone(), description: "read_ref cell dep".to_string() })
            }
            Expr::Claim(claim) => {
                self.has_cell_ops = true;
                let value = self.eval_expr(&claim.receipt)?;
                self.trace.push(TraceEvent::Claim { description: value.to_string() });
                Ok(SimValue::Symbolic { ty: "claimed".to_string(), description: "claim".to_string() })
            }
            Expr::Settle(settle) => {
                self.has_cell_ops = true;
                let value = self.eval_expr(&settle.expr)?;
                self.trace.push(TraceEvent::Settle { description: value.to_string() });
                Ok(SimValue::Symbolic { ty: "settled".to_string(), description: "settle".to_string() })
            }
            Expr::Assert(assert) => {
                let cond = self.eval_expr(&assert.condition)?;
                let msg = self.eval_expr(&assert.message)?;
                self.trace.push(TraceEvent::Assert { condition: cond.clone(), message: msg.to_string() });
                if !self.is_truthy(&cond) {
                    Ok(SimValue::Symbolic { ty: "assert_failed".to_string(), description: msg.to_string() })
                } else {
                    Ok(SimValue::Unit)
                }
            }
            Expr::Block(stmts) => {
                let result = self.exec_stmts(stmts)?;
                Ok(result.unwrap_or(SimValue::Unit))
            }
            Expr::Tuple(exprs) => {
                let values: Vec<SimValue> = exprs.iter().map(|e| self.eval_expr(e)).collect::<Result<_, _>>()?;
                Ok(SimValue::Tuple(values))
            }
            Expr::Array(exprs) => {
                let values: Vec<SimValue> = exprs.iter().map(|e| self.eval_expr(e)).collect::<Result<_, _>>()?;
                Ok(SimValue::Array(values))
            }
            Expr::If(if_expr) => {
                let cond = self.eval_expr(&if_expr.condition)?;
                let taken = self.is_truthy(&cond);
                if taken {
                    self.eval_expr(&if_expr.then_branch)
                } else {
                    self.eval_expr(&if_expr.else_branch)
                }
            }
            Expr::Cast(cast) => {
                let value = self.eval_expr(&cast.expr)?;
                // 简单模拟：整型截断
                match &value {
                    SimValue::Integer(n) => Ok(SimValue::Integer(*n)),
                    _ => Ok(value),
                }
            }
            Expr::Range(_range) => Ok(SimValue::Symbolic { ty: "range".to_string(), description: "range".to_string() }),
            Expr::StructInit(init) => {
                let fields: Vec<(String, SimValue)> = init
                    .fields
                    .iter()
                    .map(|(name, expr)| {
                        let value = self
                            .eval_expr(expr)
                            .unwrap_or_else(|_| SimValue::Symbolic { ty: "unknown".to_string(), description: "...".to_string() });
                        (name.clone(), value)
                    })
                    .collect();
                Ok(SimValue::Struct { name: init.ty.clone(), fields })
            }
            Expr::Match(_match) => {
                // 简单模拟：取第一个分支
                Ok(SimValue::Symbolic { ty: "match".to_string(), description: "match expression".to_string() })
            }
            Expr::Assign(assign) => {
                let value = self.eval_expr(&assign.value)?;
                if let Expr::Identifier(name) = assign.target.as_ref() {
                    self.env.insert(name.clone(), value.clone());
                }
                Ok(value)
            }
        }
    }

    /// 求值二元运算
    fn eval_binary(&self, op: &BinaryOp, left: &SimValue, right: &SimValue) -> Result<SimValue, SimulateError> {
        match (left, right) {
            (SimValue::Integer(l), SimValue::Integer(r)) => Ok(match op {
                BinaryOp::Add => SimValue::Integer(l.wrapping_add(*r)),
                BinaryOp::Sub => SimValue::Integer(l.wrapping_sub(*r)),
                BinaryOp::Mul => SimValue::Integer(l.wrapping_mul(*r)),
                BinaryOp::Div => {
                    if *r == 0 {
                        SimValue::Symbolic { ty: "div_zero".to_string(), description: "division by zero".to_string() }
                    } else {
                        SimValue::Integer(l / r)
                    }
                }
                BinaryOp::Mod => {
                    if *r == 0 {
                        SimValue::Symbolic { ty: "mod_zero".to_string(), description: "modulo by zero".to_string() }
                    } else {
                        SimValue::Integer(l % r)
                    }
                }
                BinaryOp::Eq => SimValue::Bool(l == r),
                BinaryOp::Ne => SimValue::Bool(l != r),
                BinaryOp::Lt => SimValue::Bool(l < r),
                BinaryOp::Le => SimValue::Bool(l <= r),
                BinaryOp::Gt => SimValue::Bool(l > r),
                BinaryOp::Ge => SimValue::Bool(l >= r),
                BinaryOp::And => SimValue::Bool(*l != 0 && *r != 0),
                BinaryOp::Or => SimValue::Bool(*l != 0 || *r != 0),
            }),
            (SimValue::Bool(l), SimValue::Bool(r)) => Ok(match op {
                BinaryOp::Eq => SimValue::Bool(l == r),
                BinaryOp::Ne => SimValue::Bool(l != r),
                BinaryOp::And => SimValue::Bool(*l && *r),
                BinaryOp::Or => SimValue::Bool(*l || *r),
                _ => return Err(SimulateError::TypeError { expected: "integer".to_string(), got: "bool".to_string() }),
            }),
            _ => Ok(SimValue::Symbolic { ty: "binary".to_string(), description: format!("{:?} {:?} {:?}", left, op, right) }),
        }
    }

    /// 求值一元运算
    fn eval_unary(&self, op: &UnaryOp, value: &SimValue) -> Result<SimValue, SimulateError> {
        match (op, value) {
            (UnaryOp::Neg, SimValue::Integer(n)) => Ok(SimValue::Integer(n.wrapping_neg())),
            (UnaryOp::Not, SimValue::Bool(b)) => Ok(SimValue::Bool(!b)),
            (UnaryOp::Not, SimValue::Integer(n)) => Ok(SimValue::Bool(*n == 0)),
            _ => Ok(SimValue::Symbolic { ty: "unary".to_string(), description: format!("{:?} {:?}", op, value) }),
        }
    }

    /// 求值调用
    fn eval_call(&mut self, call: &CallExpr) -> Result<SimValue, SimulateError> {
        // 获取函数名
        let func_name = match call.func.as_ref() {
            Expr::Identifier(name) => name.clone(),
            _ => return Ok(SimValue::Symbolic { ty: "call".to_string(), description: "indirect call".to_string() }),
        };

        // 求值参数
        let args: Vec<SimValue> = call.args.iter().map(|e| self.eval_expr(e)).collect::<Result<_, _>>()?;
        let arg_strs: Vec<String> = args.iter().map(|v| v.to_string()).collect();

        // 检查内建函数
        match func_name.as_str() {
            "vec_new" | "Vec::new" => return Ok(SimValue::Array(Vec::new())),
            "push" => {
                // 简化：不实际修改数组
                return Ok(SimValue::Unit);
            }
            "len" | "length" => {
                if let Some(SimValue::Array(items)) = args.first() {
                    return Ok(SimValue::Integer(items.len() as u64));
                }
                return Ok(SimValue::Symbolic { ty: "len".to_string(), description: "len()".to_string() });
            }
            _ => {}
        }

        self.trace.push(TraceEvent::Call { name: func_name.clone(), args: arg_strs });

        // 尝试调用用户定义的 fn
        if self.functions.contains_key(&func_name) {
            return self.simulate_function(&func_name, &args);
        }

        // 未知函数 → 符号化
        Ok(SimValue::Symbolic { ty: "call_result".to_string(), description: format!("{}()", func_name) })
    }

    /// 绑定模式
    fn bind_pattern(&mut self, pattern: &BindingPattern, value: SimValue) {
        match pattern {
            BindingPattern::Name(name) => {
                self.trace.push(TraceEvent::Bind { name: name.clone(), value: value.clone() });
                self.env.insert(name.clone(), value);
            }
            BindingPattern::Tuple(patterns) => {
                if let SimValue::Tuple(values) = value {
                    for (p, v) in patterns.iter().zip(values.into_iter()) {
                        self.bind_pattern(p, v);
                    }
                }
            }
            BindingPattern::Wildcard => {}
        }
    }

    /// 判断值是否为真
    fn is_truthy(&self, value: &SimValue) -> bool {
        match value {
            SimValue::Bool(b) => *b,
            SimValue::Integer(n) => *n != 0,
            SimValue::Unit => false,
            SimValue::Symbolic { .. } => true, // 符号值默认为真
            _ => true,
        }
    }

    /// 递增步数
    fn bump_steps(&mut self) -> Result<(), SimulateError> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(SimulateError::StepLimitExceeded { max: self.max_steps });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn parse_module(source: &str) -> Module {
        let tokens = lexer::lex(source).expect("lex");
        parser::parse(&tokens).expect("parse")
    }

    #[test]
    fn simulate_pure_arithmetic_action() {
        let source = r#"
module sim_test

action add(a: u64, b: u64) -> u64 {
    let result = a + b
    return result
}
"#;
        let module = parse_module(source);
        let mut interp = SimulateInterpreter::new(&module, 1000);
        let result = interp.simulate_action("add", &[SimValue::Integer(3), SimValue::Integer(5)]).unwrap();
        assert_eq!(result.return_value, SimValue::Integer(8));
        assert!(!result.has_cell_ops);
    }

    #[test]
    fn simulate_cell_operation_traces() {
        let source = r#"
module cell_test

resource Token has store, transfer, destroy {
    amount: u64,
}

action mint(amount: u64) -> u64 {
    let token = create Token { amount: amount }
    consume token
    return amount
}
"#;
        let module = parse_module(source);
        let mut interp = SimulateInterpreter::new(&module, 1000);
        let result = interp.simulate_action("mint", &[SimValue::Integer(100)]).unwrap();
        assert!(result.has_cell_ops);
        assert!(result.trace.iter().any(|e| matches!(e, TraceEvent::Create { .. })));
        assert!(result.trace.iter().any(|e| matches!(e, TraceEvent::Consume { .. })));
    }

    #[test]
    fn simulate_read_ref_traces() {
        let source = r#"
module ref_test

shared Config {
    threshold: u64,
}

action check() -> u64 {
    let cfg = read_ref<Config>()
    cfg.threshold
}
"#;
        let module = parse_module(source);
        let mut interp = SimulateInterpreter::new(&module, 1000);
        let result = interp.simulate_action("check", &[]).unwrap();
        assert!(result.has_cell_ops);
        assert!(result.trace.iter().any(|e| matches!(e, TraceEvent::ReadRef { .. })));
    }

    #[test]
    fn simulate_if_branch() {
        let source = r#"
module branch_test

action classify(x: u64) -> u64 {
    let result = 0
    if x > 10 {
        return 1
    }
    return 0
}
"#;
        let module = parse_module(source);
        let mut interp = SimulateInterpreter::new(&module, 1000);
        let result = interp.simulate_action("classify", &[SimValue::Integer(15)]).unwrap();
        assert_eq!(result.return_value, SimValue::Integer(1));
    }

    #[test]
    fn simulate_step_limit() {
        let source = r#"
module limit_test

action infinite() -> u64 {
    let x = 0
    return x
}
"#;
        let module = parse_module(source);
        let mut interp = SimulateInterpreter::new(&module, 2); // 非常低的步数限制
                                                               // 解析 + 绑定 + 返回 可能超过 2 步
        let result = interp.simulate_action("infinite", &[]);
        // 应该成功（只有3步）或失败
        match result {
            Ok(r) => assert!(r.steps <= 3),
            Err(SimulateError::StepLimitExceeded { .. }) => {} // ok
            Err(e) => panic!("unexpected error: {}", e),
        }
    }
}
