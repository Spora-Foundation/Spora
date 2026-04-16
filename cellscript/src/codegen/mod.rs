//! RISC-V 代码生成器
//!
//! 将 Spora IR 转换为 RISC-V 汇编或 ELF 产物

use crate::ast::{BinaryOp, UnaryOp};
use crate::error::{CompileError, Result};
use crate::ir::*;
use crate::ArtifactFormat;
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const CKB_LOAD_CELL_SYSCALL_NUMBER: u64 = 2071;
const CKB_SOURCE_INPUT: u64 = 0x01;
const CKB_SOURCE_OUTPUT: u64 = 0x02;
const CKB_SOURCE_CELL_DEP: u64 = 0x03;
const RUNTIME_SCRATCH_BUFFER_SIZE: usize = 256;
const RUNTIME_SCRATCH_SIZE: usize = 8 + RUNTIME_SCRATCH_BUFFER_SIZE;
const RUNTIME_CELL_BUFFER_SIZE: usize = 256;
const RUNTIME_CELL_SLOT_SIZE: usize = 8 + RUNTIME_CELL_BUFFER_SIZE;

#[derive(Debug, Clone)]
struct SchemaFieldLayout {
    offset: usize,
    ty: IrType,
    fixed_size: Option<usize>,
}

#[derive(Debug, Clone)]
struct SchemaFieldValueSource {
    obj_var_id: usize,
    type_name: String,
    field: String,
    layout: SchemaFieldLayout,
}

fn fixed_scalar_width(ty: &IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (IrType::Bool | IrType::U8, Some(1)) => Some(1),
        (IrType::U16, Some(2)) => Some(2),
        (IrType::U32, Some(4)) => Some(4),
        (IrType::U64, Some(8)) => Some(8),
        _ => None,
    }
}

fn fixed_scalar_const_value(value: &IrConst) -> Option<u64> {
    match value {
        IrConst::Bool(value) => Some(u64::from(*value)),
        IrConst::U8(value) => Some((*value).into()),
        IrConst::U16(value) => Some((*value).into()),
        IrConst::U32(value) => Some((*value).into()),
        IrConst::U64(value) => Some(*value),
        _ => None,
    }
}

fn abi_arg_label(index: usize) -> String {
    if index < 8 {
        format!("a{}", index)
    } else {
        format!("stack+{}", (index - 8) * 8)
    }
}

#[derive(Debug, Clone)]
enum PreludeU64OperandSource {
    Const(u64),
    ParamVar(usize),
    Field(SchemaFieldValueSource),
}

#[derive(Debug, Clone)]
enum PreludeU64ValueSource {
    Const(u64),
    ParamVar(usize),
    Field(SchemaFieldValueSource),
    Binary { op: BinaryOp, left: Box<PreludeU64ValueSource>, right: PreludeU64OperandSource },
}

/// 代码生成选项
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// 优化级别
    pub opt_level: u8,
    /// 是否生成调试信息
    pub debug: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self { opt_level: 0, debug: false }
    }
}

/// 代码生成器
pub struct CodeGenerator {
    options: CodegenOptions,
    /// 生成的汇编代码
    assembly: Vec<String>,
    /// 当前函数名
    current_function: Option<String>,
    /// 当前栈帧大小
    frame_size: usize,
    /// 逻辑输出句柄计数器
    next_virtual_output: usize,
    /// 是否依赖尚未具有真实可执行语义的符号化 cell/runtime 降级路径
    requires_symbolic_runtime: bool,
    /// Named schema field layouts, keyed by type name then field name.
    type_layouts: HashMap<String, HashMap<String, SchemaFieldLayout>>,
    /// Fixed encoded size of named schemas when all fields have fixed-width layouts.
    type_fixed_sizes: HashMap<String, usize>,
    /// Lifecycle state names for receipt schemas that declared #[lifecycle(...)].
    lifecycle_states: HashMap<String, Vec<String>>,
    /// Function parameters whose slot contains a pointer to Borsh-encoded schema bytes.
    schema_pointer_vars: BTreeSet<usize>,
    /// Function parameter slots available before the prelude summaries run.
    param_vars: BTreeSet<usize>,
    /// Schema pointer slots backed by a VM-loaded cell buffer size word.
    schema_pointer_size_offsets: HashMap<usize, usize>,
    /// Fixed scalar temporaries that are aliases for schema-backed field loads.
    schema_field_value_sources: HashMap<usize, SchemaFieldValueSource>,
    /// U64 temporaries that can be recomputed in the CKB-runtime prelude.
    prelude_u64_value_sources: HashMap<usize, PreludeU64ValueSource>,
    /// Fixed scalar temporaries that can be recomputed as immediates in the CKB-runtime prelude.
    prelude_scalar_immediates: HashMap<usize, u64>,
    /// Per-CKB-runtime cell data buffers keyed by IR variable id.
    cell_buffer_offsets: HashMap<usize, usize>,
    /// Per-CKB-runtime cell size words keyed by IR variable id.
    cell_buffer_size_offsets: HashMap<usize, usize>,
    /// Consumed IR operand variable ids in source lowering order.
    consume_order: Vec<usize>,
    /// Consumed Input index keyed by IR operand variable id.
    consume_indices: HashMap<usize, usize>,
    /// Consumed named schema type keyed by IR operand variable id.
    consume_type_names: HashMap<usize, String>,
    /// Read-ref IR destination variable ids in source lowering order.
    read_ref_order: Vec<usize>,
    /// Read-ref CellDep index keyed by IR destination variable id.
    read_ref_indices: HashMap<usize, usize>,
    /// Unique label counter for runtime checks.
    next_runtime_label: usize,
}

impl CodeGenerator {
    /// 创建新的代码生成器
    pub fn new(options: CodegenOptions) -> Self {
        Self {
            options,
            assembly: Vec::new(),
            current_function: None,
            frame_size: 16,
            next_virtual_output: 0,
            requires_symbolic_runtime: false,
            type_layouts: HashMap::new(),
            type_fixed_sizes: HashMap::new(),
            lifecycle_states: HashMap::new(),
            schema_pointer_vars: BTreeSet::new(),
            param_vars: BTreeSet::new(),
            schema_pointer_size_offsets: HashMap::new(),
            schema_field_value_sources: HashMap::new(),
            prelude_u64_value_sources: HashMap::new(),
            prelude_scalar_immediates: HashMap::new(),
            cell_buffer_offsets: HashMap::new(),
            cell_buffer_size_offsets: HashMap::new(),
            consume_order: Vec::new(),
            consume_indices: HashMap::new(),
            consume_type_names: HashMap::new(),
            read_ref_order: Vec::new(),
            read_ref_indices: HashMap::new(),
            next_runtime_label: 0,
        }
    }

    /// 生成代码
    pub fn generate(mut self, ir: &IrModule, format: ArtifactFormat) -> Result<Vec<u8>> {
        let has_entrypoint = ir.items.iter().any(|item| matches!(item, IrItem::Action(_) | IrItem::Lock(_)));
        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.register_type_def(type_def);
            }
        }

        // 生成文件头
        self.emit_header();

        // 生成类型定义（数据段）
        for item in &ir.items {
            if let IrItem::TypeDef(type_def) = item {
                self.generate_type_def(type_def)?;
            }
        }

        // 生成代码段
        self.emit_section(".text");

        for item in &ir.items {
            match item {
                IrItem::Action(action) => {
                    self.generate_action(action)?;
                }
                _ => {}
            }
        }
        for item in &ir.items {
            if let IrItem::Lock(lock) = item {
                self.generate_lock(lock)?;
            }
        }
        if has_entrypoint {
            for item in &ir.items {
                if let IrItem::PureFn(function) = item {
                    self.generate_pure_fn(function)?;
                }
            }
        }

        // 生成运行时支持函数
        self.generate_runtime_support();

        // 组装为汇编产物
        self.assemble(format)
    }

    /// 生成文件头
    fn emit_header(&mut self) {
        self.assembly.push("# CellScript Generated Assembly".to_string());
        self.assembly.push(format!("# opt_level={}, debug={}", self.options.opt_level, self.options.debug));
        self.assembly.push(".option arch, +rv64imac".to_string());
        self.assembly.push("".to_string());
    }

    /// 生成段声明
    fn emit_section(&mut self, section: &str) {
        self.assembly.push(format!(".section {}", section));
    }

    /// 生成全局符号
    fn emit_global(&mut self, name: &str) {
        self.assembly.push(format!(".global {}", name));
        self.assembly.push(format!(".type {}, @function", name));
    }

    /// 生成标签
    fn emit_label(&mut self, name: &str) {
        self.assembly.push(format!("{}:", name));
    }

    /// 生成指令
    fn emit(&mut self, instruction: impl Into<String>) {
        self.assembly.push(format!("    {}", instruction.into()));
    }

    /// 生成类型定义
    fn generate_type_def(&mut self, type_def: &IrTypeDef) -> Result<()> {
        // 生成类型的 Borsh 序列化/反序列化代码
        self.emit_section(".rodata");
        self.emit_label(&format!("__type_desc_{}", type_def.name));

        // 类型描述符：字段数量 + 字段信息
        self.emit(format!(".word {}", type_def.fields.len()));

        for field in &type_def.fields {
            // 字段名长度
            self.emit(format!(".byte {}", field.name.len()));
            // 字段名
            self.emit(format!(".ascii \"{}\"", field.name));
            // 对齐
            self.emit(".align 3");
            // 字段类型标识
            self.emit(format!(".word {}", self.type_id(&field.ty)));
        }

        Ok(())
    }

    fn register_type_def(&mut self, type_def: &IrTypeDef) {
        if let Some(fixed_size) = type_def.fields.iter().try_fold(0usize, |acc, field| field.fixed_size.map(|size| acc + size)) {
            self.type_fixed_sizes.insert(type_def.name.clone(), fixed_size);
        }
        if let Some(states) = &type_def.lifecycle_states {
            self.lifecycle_states.insert(type_def.name.clone(), states.clone());
        }
        let fields = type_def
            .fields
            .iter()
            .map(|field| {
                (field.name.clone(), SchemaFieldLayout { offset: field.offset, ty: field.ty.clone(), fixed_size: field.fixed_size })
            })
            .collect();
        self.type_layouts.insert(type_def.name.clone(), fields);
    }

    /// 获取类型 ID
    fn type_id(&self, ty: &IrType) -> u32 {
        match ty {
            IrType::U8 => 1,
            IrType::U16 => 2,
            IrType::U32 => 3,
            IrType::U64 => 4,
            IrType::U128 => 5,
            IrType::Bool => 6,
            IrType::Address => 7,
            IrType::Hash => 8,
            IrType::Array(_, _) => 9,
            IrType::Tuple(_) => 10,
            IrType::Named(_) => 11,
            IrType::Ref(_) => 12,
            IrType::MutRef(_) => 13,
            IrType::Unit => 14,
        }
    }

    /// 生成 action
    fn generate_action(&mut self, action: &IrAction) -> Result<()> {
        self.current_function = Some(action.name.clone());
        self.prepare_function_layout(&action.body, &action.params);
        self.next_virtual_output = 0;
        self.next_runtime_label = 0;
        self.set_schema_pointer_params(&action.params);
        self.set_consumed_schema_pointers(&action.body);
        self.set_read_ref_schema_pointers(&action.body);
        self.set_schema_field_value_sources(&action.body);

        self.emit_global(&action.name);
        self.emit_label(&action.name);

        // 函数序言
        self.emit_prologue();
        self.emit_param_spills(&action.params)?;

        // 生成函数体
        self.generate_body(&action.body)?;

        self.current_function = None;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.param_vars.clear();
        Ok(())
    }

    /// 生成 pure helper function
    fn generate_pure_fn(&mut self, function: &IrPureFn) -> Result<()> {
        self.current_function = Some(function.name.clone());
        self.prepare_function_layout(&function.body, &function.params);
        self.next_virtual_output = 0;
        self.next_runtime_label = 0;
        self.set_schema_pointer_params(&function.params);
        self.set_consumed_schema_pointers(&function.body);
        self.set_read_ref_schema_pointers(&function.body);
        self.set_schema_field_value_sources(&function.body);

        self.emit_global(&function.name);
        self.emit_label(&function.name);

        self.emit_prologue();
        self.emit_param_spills(&function.params)?;
        self.generate_body(&function.body)?;

        self.current_function = None;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.param_vars.clear();
        Ok(())
    }

    /// 生成 lock
    fn generate_lock(&mut self, lock: &IrLock) -> Result<()> {
        self.current_function = Some(lock.name.clone());
        self.prepare_function_layout(&lock.body, &lock.params);
        self.next_virtual_output = 0;
        self.next_runtime_label = 0;
        self.set_schema_pointer_params(&lock.params);
        self.set_consumed_schema_pointers(&lock.body);
        self.set_read_ref_schema_pointers(&lock.body);
        self.set_schema_field_value_sources(&lock.body);

        self.emit_global(&lock.name);
        self.emit_label(&lock.name);

        // 函数序言
        self.emit_prologue();
        self.emit_param_spills(&lock.params)?;

        // 生成函数体
        self.generate_body(&lock.body)?;

        self.current_function = None;
        self.schema_pointer_vars.clear();
        self.schema_pointer_size_offsets.clear();
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        self.param_vars.clear();
        Ok(())
    }

    fn set_schema_pointer_params(&mut self, params: &[IrParam]) {
        self.schema_pointer_vars.clear();
        self.param_vars.clear();
        for param in params {
            self.param_vars.insert(param.binding.id);
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_vars.insert(param.binding.id);
            }
        }
    }

    fn set_read_ref_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.schema_pointer_vars.insert(dest.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&dest.id).copied() {
                        self.schema_pointer_size_offsets.insert(dest.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_consumed_schema_pointers(&mut self, body: &IrBody) {
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    self.schema_pointer_vars.insert(var.id);
                    if let Some(size_offset) = self.cell_buffer_size_offsets.get(&var.id).copied() {
                        self.schema_pointer_size_offsets.insert(var.id, size_offset);
                    }
                }
            }
        }
    }

    fn set_schema_field_value_sources(&mut self, body: &IrBody) {
        self.schema_field_value_sources.clear();
        self.prelude_u64_value_sources.clear();
        self.prelude_scalar_immediates.clear();
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    IrInstruction::LoadConst { dest, value } => {
                        if let Some(value) = fixed_scalar_const_value(value) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Const(value));
                            }
                        }
                    }
                    IrInstruction::FieldAccess { dest, obj: IrOperand::Var(obj), field } => {
                        if !self.schema_pointer_vars.contains(&obj.id) {
                            continue;
                        }
                        let Some(type_name) = named_type_name(&obj.ty) else {
                            continue;
                        };
                        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
                            continue;
                        };
                        if fixed_scalar_width(&layout.ty, layout.fixed_size).is_some() && layout.ty == dest.ty {
                            let source = SchemaFieldValueSource {
                                obj_var_id: obj.id,
                                type_name: type_name.to_string(),
                                field: field.clone(),
                                layout,
                            };
                            self.schema_field_value_sources.insert(dest.id, source.clone());
                            if dest.ty == IrType::U64 {
                                self.prelude_u64_value_sources.insert(dest.id, PreludeU64ValueSource::Field(source));
                            }
                        }
                    }
                    IrInstruction::Binary { dest, op, left, right }
                        if dest.ty == IrType::U64 && matches!(op, BinaryOp::Add | BinaryOp::Sub) =>
                    {
                        let Some(left) = self.prelude_u64_value_source(left) else {
                            continue;
                        };
                        let Some(right) = self.prelude_u64_operand_source(right) else {
                            continue;
                        };
                        self.prelude_u64_value_sources
                            .insert(dest.id, PreludeU64ValueSource::Binary { op: *op, left: Box::new(left), right });
                    }
                    IrInstruction::Move { dest, src } if dest.ty == IrType::U64 => {
                        let Some(source) = self.prelude_u64_value_source(src) else {
                            continue;
                        };
                        self.prelude_u64_value_sources.insert(dest.id, source);
                    }
                    IrInstruction::Move { dest, src } if matches!(dest.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32) => {
                        if let Some(value) = self.prelude_scalar_immediate(src) {
                            self.prelude_scalar_immediates.insert(dest.id, value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn prelude_scalar_immediate(&self, operand: &IrOperand) -> Option<u64> {
        match operand {
            IrOperand::Const(value) => fixed_scalar_const_value(value),
            IrOperand::Var(var) => self.prelude_scalar_immediates.get(&var.id).copied(),
        }
    }

    fn prelude_u64_value_source(&self, operand: &IrOperand) -> Option<PreludeU64ValueSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64ValueSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64ValueSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => self.prelude_u64_value_sources.get(&var.id).cloned(),
            _ => None,
        }
    }

    fn prelude_u64_operand_source(&self, operand: &IrOperand) -> Option<PreludeU64OperandSource> {
        match operand {
            IrOperand::Const(IrConst::U64(n)) => Some(PreludeU64OperandSource::Const(*n)),
            IrOperand::Var(var) if var.ty == IrType::U64 && self.param_vars.contains(&var.id) => {
                Some(PreludeU64OperandSource::ParamVar(var.id))
            }
            IrOperand::Var(var) => match self.prelude_u64_value_sources.get(&var.id)? {
                PreludeU64ValueSource::Const(n) => Some(PreludeU64OperandSource::Const(*n)),
                PreludeU64ValueSource::ParamVar(var_id) => Some(PreludeU64OperandSource::ParamVar(*var_id)),
                PreludeU64ValueSource::Field(source) => Some(PreludeU64OperandSource::Field(source.clone())),
                PreludeU64ValueSource::Binary { .. } => None,
            },
            _ => None,
        }
    }

    /// 生成函数体
    fn generate_body(&mut self, body: &IrBody) -> Result<()> {
        // 处理 consume_set
        for (index, pattern) in body.consume_set.iter().enumerate() {
            self.generate_consume(pattern, index)?;
        }

        // 处理 read_refs
        for (index, pattern) in body.read_refs.iter().enumerate() {
            self.generate_read_ref(pattern, index)?;
        }

        // 处理 create_set
        for (index, pattern) in body.create_set.iter().enumerate() {
            self.generate_create(pattern, index)?;
        }

        // 生成基本块
        for block in &body.blocks {
            self.generate_block(block)?;
        }

        Ok(())
    }

    /// 生成 consume 代码
    fn generate_consume(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# consume {}", pattern.binding));
        if let Some(var_id) = self.consume_order.get(index).copied() {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let input_index = self.consume_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_syscall_to_offsets(
                    "consume",
                    CKB_SOURCE_INPUT,
                    input_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(1);
                self.emit(format!("addi t0, sp, {}", buffer_offset));
                self.emit(format!("sd t0, {}(sp)", var_id * 8));
                return Ok(());
            }
        }

        self.emit_load_cell_syscall("consume", CKB_SOURCE_INPUT, index);
        Ok(())
    }

    /// 生成 read_ref 代码
    fn generate_read_ref(&mut self, pattern: &CellPattern, index: usize) -> Result<()> {
        self.emit(format!("# read_ref {}", pattern.binding));
        if let Some(var_id) = self.read_ref_order.get(index).copied() {
            if let (Some(size_offset), Some(buffer_offset)) =
                (self.cell_buffer_size_offsets.get(&var_id).copied(), self.cell_buffer_offsets.get(&var_id).copied())
            {
                let dep_index = self.read_ref_indices.get(&var_id).copied().unwrap_or(index);
                self.emit_load_cell_syscall_to_offsets(
                    "read_ref",
                    CKB_SOURCE_CELL_DEP,
                    dep_index,
                    size_offset,
                    buffer_offset,
                    RUNTIME_CELL_BUFFER_SIZE,
                );
                self.emit_return_on_syscall_error(1);
                self.emit(format!("addi t0, sp, {}", buffer_offset));
                self.emit(format!("sd t0, {}(sp)", var_id * 8));
                return Ok(());
            }
        }

        self.emit_load_cell_syscall("read_ref", CKB_SOURCE_CELL_DEP, index);
        Ok(())
    }

    /// 生成 create 代码
    fn generate_create(&mut self, pattern: &CreatePattern, index: usize) -> Result<()> {
        // The verifier cannot create cells inside CKB-VM; it can only verify the
        // transaction output selected by the lowering metadata.
        self.emit(format!("# create {}", pattern.ty));
        self.emit_load_cell_syscall("create", CKB_SOURCE_OUTPUT, index);
        self.emit_return_on_syscall_error(1);

        // 如果有 lock 脚本，设置 lock
        if pattern.lock.is_some() {
            self.emit("# set lock script");
        }

        if self.can_verify_create_output(pattern) {
            self.emit_create_output_checks(pattern);
        } else {
            self.emit("# cellscript abi: output field verification incomplete for this create pattern");
            self.emit("# cellscript abi: fail closed because the output state is not fully verified");
            self.emit("li a0, 5");
            self.emit_epilogue();
        }

        Ok(())
    }

    /// 生成基本块
    fn generate_block(&mut self, block: &IrBlock) -> Result<()> {
        self.emit_label(&format!(".Lblock_{}", block.id.0));

        for instruction in &block.instructions {
            self.generate_instruction(instruction)?;
        }

        self.generate_terminator(&block.terminator)?;

        Ok(())
    }

    /// 生成指令
    fn generate_instruction(&mut self, instruction: &IrInstruction) -> Result<()> {
        match instruction {
            IrInstruction::LoadConst { dest, value } => {
                self.emit_load_const(dest, value)?;
            }
            IrInstruction::LoadVar { dest, name } => {
                self.emit_load_var(dest, name)?;
            }
            IrInstruction::StoreVar { name, src } => {
                self.emit_store_var(name, src)?;
            }
            IrInstruction::Binary { dest, op, left, right } => {
                self.emit_binary(dest, *op, left, right)?;
            }
            IrInstruction::Unary { dest, op, operand } => {
                self.emit_unary(dest, *op, operand)?;
            }
            IrInstruction::FieldAccess { dest, obj, field } => {
                self.emit_field_access(dest, obj, field)?;
            }
            IrInstruction::Index { dest, arr, idx } => {
                self.emit_index(dest, arr, idx)?;
            }
            IrInstruction::Length { dest, operand } => {
                self.emit_length(dest, operand)?;
            }
            IrInstruction::TypeHash { dest, operand } => {
                self.emit_type_hash(dest, operand)?;
            }
            IrInstruction::CollectionNew { dest, ty } => {
                self.emit_collection_new(dest, ty)?;
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.emit_collection_push(collection, value)?;
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.emit_collection_extend(collection, slice)?;
            }
            IrInstruction::Call { dest, func, args } => {
                self.emit_call(dest.as_ref(), func, args)?;
            }
            IrInstruction::ReadRef { dest, ty } => {
                self.emit_read_ref(dest, ty)?;
            }
            IrInstruction::Move { dest, src } => {
                self.emit_move(dest, src)?;
            }
            IrInstruction::Consume { operand } => {
                self.emit_consume(operand)?;
            }
            IrInstruction::Create { dest, pattern } => {
                self.emit_create(dest, pattern)?;
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.emit_transfer(dest, operand, to)?;
            }
            IrInstruction::Destroy { operand } => {
                self.emit_destroy(operand)?;
            }
            IrInstruction::Claim { dest, receipt } => {
                self.emit_claim(dest, receipt)?;
            }
            IrInstruction::Settle { operand } => {
                self.emit_settle(operand)?;
            }
        }
        Ok(())
    }

    /// 生成终止指令
    fn generate_terminator(&mut self, terminator: &IrTerminator) -> Result<()> {
        match terminator {
            IrTerminator::Return(None) => {
                self.emit_epilogue();
            }
            IrTerminator::Return(Some(operand)) => {
                // 将返回值放入 a0
                match operand {
                    IrOperand::Const(c) => match c {
                        IrConst::U64(n) => self.emit(format!("li a0, {}", n)),
                        _ => self.emit("li a0, 0"),
                    },
                    IrOperand::Var(v) => {
                        self.emit(format!("ld a0, {}(sp)", v.id * 8));
                    }
                }
                self.emit_epilogue();
            }
            IrTerminator::Jump(block_id) => {
                self.emit(format!("j .Lblock_{}", block_id.0));
            }
            IrTerminator::Branch { cond, then_block, else_block } => match cond {
                IrOperand::Const(IrConst::Bool(b)) => {
                    if *b {
                        self.emit(format!("j .Lblock_{}", then_block.0));
                    } else {
                        self.emit(format!("j .Lblock_{}", else_block.0));
                    }
                }
                IrOperand::Const(IrConst::U64(n)) => {
                    if *n != 0 {
                        self.emit(format!("j .Lblock_{}", then_block.0));
                    } else {
                        self.emit(format!("j .Lblock_{}", else_block.0));
                    }
                }
                IrOperand::Var(v) => {
                    self.emit(format!("ld t0, {}(sp)", v.id * 8));
                    self.emit(format!("beqz t0, .Lblock_{}", else_block.0));
                    self.emit(format!("j .Lblock_{}", then_block.0));
                }
                _ => {
                    self.emit(format!("j .Lblock_{}", else_block.0));
                }
            },
        }
        Ok(())
    }

    /// 函数序言
    fn emit_prologue(&mut self) {
        // 保存返回地址和帧指针
        self.emit(format!("addi sp, sp, -{}", self.frame_size));
        self.emit(format!("sd ra, {}(sp)", self.frame_size - 8));
        self.emit(format!("sd fp, {}(sp)", self.frame_size - 16));
        self.emit(format!("addi fp, sp, {}", self.frame_size));
    }

    /// 函数尾声
    fn emit_epilogue(&mut self) {
        // 恢复返回地址和帧指针
        self.emit(format!("ld ra, {}(sp)", self.frame_size - 8));
        self.emit(format!("ld fp, {}(sp)", self.frame_size - 16));
        self.emit(format!("addi sp, sp, {}", self.frame_size));
        self.emit("ret");
    }

    fn prepare_function_layout(&mut self, body: &IrBody, params: &[IrParam]) {
        let mut max_var_id = None;
        for param in params {
            self.record_var(&param.binding, &mut max_var_id);
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                self.record_instruction_var(instruction, &mut max_var_id);
            }
            self.record_terminator_var(&block.terminator, &mut max_var_id);
        }

        let locals_size = max_var_id.map(|id| (id + 1) * 8).unwrap_or(0);
        self.cell_buffer_offsets.clear();
        self.cell_buffer_size_offsets.clear();
        self.consume_order.clear();
        self.consume_indices.clear();
        self.consume_type_names.clear();
        self.read_ref_order.clear();
        self.read_ref_indices.clear();
        self.schema_pointer_size_offsets.clear();

        let mut next_cell_slot = locals_size;
        for param in params {
            if named_type_name(&param.ty).is_some() {
                self.schema_pointer_size_offsets.insert(param.binding.id, next_cell_slot);
                next_cell_slot += 8;
            }
        }

        let mut consume_index = 0usize;
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let Some(var) = consumed_operand_var(instruction) {
                    if let Some(type_name) = named_type_name(&var.ty) {
                        self.consume_type_names.insert(var.id, type_name.to_string());
                    }
                    self.cell_buffer_size_offsets.insert(var.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(var.id, next_cell_slot + 8);
                    self.consume_order.push(var.id);
                    self.consume_indices.insert(var.id, consume_index);
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    consume_index += 1;
                }
            }
        }

        let mut read_ref_index = 0usize;
        for block in &body.blocks {
            for instruction in &block.instructions {
                if let IrInstruction::ReadRef { dest, .. } = instruction {
                    self.cell_buffer_size_offsets.insert(dest.id, next_cell_slot);
                    self.cell_buffer_offsets.insert(dest.id, next_cell_slot + 8);
                    self.read_ref_order.push(dest.id);
                    self.read_ref_indices.insert(dest.id, read_ref_index);
                    next_cell_slot += RUNTIME_CELL_SLOT_SIZE;
                    read_ref_index += 1;
                }
            }
        }

        self.frame_size = align_frame(next_cell_slot + RUNTIME_SCRATCH_SIZE + 16);
    }

    fn runtime_scratch_size_offset(&self) -> usize {
        self.frame_size - 16 - RUNTIME_SCRATCH_SIZE
    }

    fn runtime_scratch_buffer_offset(&self) -> usize {
        self.runtime_scratch_size_offset() + 8
    }

    fn emit_store_data_args_at(&mut self, max_bytes: usize, size_offset: usize, buffer_offset: usize) {
        self.emit(format!("li t0, {}", max_bytes));
        self.emit(format!("sd t0, {}(sp)", size_offset));
        self.emit(format!("addi a0, sp, {}", buffer_offset));
        self.emit(format!("addi a1, sp, {}", size_offset));
        self.emit("li a2, 0");
    }

    fn emit_load_cell_syscall(&mut self, reason: &str, source: u64, index: usize) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        self.emit_load_cell_syscall_to_offsets(reason, source, index, size_offset, buffer_offset, RUNTIME_SCRATCH_BUFFER_SIZE);
    }

    fn emit_load_cell_syscall_to_offsets(
        &mut self,
        reason: &str,
        source: u64,
        index: usize,
        size_offset: usize,
        buffer_offset: usize,
        max_bytes: usize,
    ) {
        self.emit(format!("# cellscript abi: LOAD_CELL reason={} source={} index={}", reason, ckb_source_name(source), index));
        self.emit_store_data_args_at(max_bytes, size_offset, buffer_offset);
        self.emit(format!("li a3, {}", index));
        self.emit(format!("li a4, {}", source));
        self.emit(format!("li a7, {}", CKB_LOAD_CELL_SYSCALL_NUMBER));
        self.emit("ecall");
        self.emit("# a0 = CKB syscall return code");
    }

    fn emit_return_on_syscall_error(&mut self, code: u64) {
        let ok_label = self.fresh_label("ckb_syscall_ok");
        self.emit(format!("beqz a0, {}", ok_label));
        self.emit(format!("li a0, {}", code));
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_bounds_check(&mut self, size_offset: usize, required_size: usize, context: &str) {
        let ok_label = self.fresh_label("schema_bounds_ok");
        self.emit(format!("# cellscript abi: bounds check {} required={}", context, required_size));
        self.emit(format!("ld t1, {}(sp)", size_offset));
        self.emit(format!("li t2, {}", required_size));
        self.emit("slt t1, t1, t2");
        self.emit(format!("beqz t1, {}", ok_label));
        self.emit("li a0, 2");
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_loaded_schema_exact_size_check(&mut self, size_offset: usize, expected_size: usize, context: &str) {
        let ok_label = self.fresh_label("schema_size_ok");
        self.emit(format!("# cellscript abi: exact size check {} expected={}", context, expected_size));
        self.emit(format!("ld t1, {}(sp)", size_offset));
        self.emit(format!("li t2, {}", expected_size));
        self.emit("sub t1, t1, t2");
        self.emit(format!("beqz t1, {}", ok_label));
        self.emit("li a0, 4");
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_symbolic_runtime_fail_closed(&mut self, feature: &str) {
        self.emit(format!("# cellscript abi: {} symbolic runtime is not executable", feature));
        self.emit("# cellscript abi: fail closed because the source operation has no complete verifier lowering");
        self.emit("li a0, 6");
        self.emit_epilogue();
    }

    fn emit_loaded_field_equals_expected(
        &mut self,
        size_offset: usize,
        buffer_offset: usize,
        layout: &SchemaFieldLayout,
        expected: &IrOperand,
        context: &str,
    ) {
        let Some(width) = fixed_scalar_width(&layout.ty, layout.fixed_size) else {
            return;
        };
        self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, context);
        self.emit(format!("# cellscript abi: verify output field {} offset={} size={}", context, layout.offset, width));
        self.emit(format!("addi t4, sp, {}", buffer_offset));
        self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        self.emit_expected_operand_to_t1(expected);
        self.emit("sub t2, t0, t1");
        let ok_label = self.fresh_label("output_field_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit("li a0, 3");
        self.emit_epilogue();
        self.emit_label(&ok_label);
    }

    fn emit_expected_operand_to_t1(&mut self, operand: &IrOperand) {
        match operand {
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t1, {}", if *b { 1 } else { 0 })),
            IrOperand::Const(IrConst::U8(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U16(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U32(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Var(var) => {
                if let Some(value) = self.prelude_scalar_immediates.get(&var.id).copied() {
                    self.emit(format!("li t1, {}", value));
                } else if let Some(source) = self.schema_field_value_sources.get(&var.id).cloned() {
                    self.emit_schema_field_source_to_t1(&source);
                } else if let Some(source) = self.prelude_u64_value_sources.get(&var.id).cloned() {
                    self.emit_prelude_u64_value_source_to_t1(&source);
                } else {
                    self.emit(format!("ld t1, {}(sp)", var.id * 8));
                }
            }
            _ => self.emit("li t1, 0"),
        }
    }

    fn emit_prelude_u64_value_source_to_t1(&mut self, source: &PreludeU64ValueSource) {
        match source {
            PreludeU64ValueSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64ValueSource::ParamVar(var_id) => self.emit(format!("ld t1, {}(sp)", var_id * 8)),
            PreludeU64ValueSource::Field(source) => self.emit_schema_field_source_to_t1(source),
            PreludeU64ValueSource::Binary { op, left, right } => {
                self.emit("# cellscript abi: expected expression u64 add/sub chain");
                self.emit_prelude_u64_value_source_to_t1(left);
                self.emit("add t3, t1, zero");
                self.emit_prelude_u64_operand_source_to_t1(right);
                match op {
                    BinaryOp::Add => self.emit("add t1, t3, t1"),
                    BinaryOp::Sub => self.emit("sub t1, t3, t1"),
                    _ => unreachable!("prelude u64 binary source only supports add/sub"),
                }
            }
        }
    }

    fn emit_prelude_u64_operand_source_to_t1(&mut self, source: &PreludeU64OperandSource) {
        match source {
            PreludeU64OperandSource::Const(n) => self.emit(format!("li t1, {}", n)),
            PreludeU64OperandSource::ParamVar(var_id) => self.emit(format!("ld t1, {}(sp)", var_id * 8)),
            PreludeU64OperandSource::Field(source) => self.emit_schema_field_source_to_t1(source),
        }
    }

    fn emit_schema_field_source_to_t1(&mut self, source: &SchemaFieldValueSource) {
        let context = format!("{}.{}", source.type_name, source.field);
        let Some(width) = fixed_scalar_width(&source.layout.ty, source.layout.fixed_size) else {
            self.emit("li t1, 0");
            return;
        };
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&source.obj_var_id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(&source.type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &source.type_name);
            }
            self.emit_loaded_schema_bounds_check(size_offset, source.layout.offset + width, &context);
        }
        self.emit(format!("# cellscript abi: expected field {} offset={} size={}", context, source.layout.offset, width));
        self.emit(format!("ld t4, {}(sp)", source.obj_var_id * 8));
        self.emit_unaligned_scalar_load("t4", "t1", "t2", source.layout.offset, width);
    }

    fn can_verify_create_output(&self, pattern: &CreatePattern) -> bool {
        if pattern.lock.is_some() || pattern.fields.is_empty() {
            return false;
        }
        let Some(layouts) = self.type_layouts.get(&pattern.ty) else {
            return false;
        };
        let covered_fields = pattern.fields.iter().map(|(field, _)| field.as_str()).collect::<BTreeSet<_>>();
        if !layouts.keys().all(|field| covered_fields.contains(field.as_str())) {
            return false;
        }
        pattern.fields.iter().all(|(field, value)| {
            layouts.get(field).is_some_and(|layout| {
                fixed_scalar_width(&layout.ty, layout.fixed_size).is_some() && self.is_prelude_available_scalar(value)
            })
        })
    }

    fn emit_create_output_checks(&mut self, pattern: &CreatePattern) {
        let size_offset = self.runtime_scratch_size_offset();
        let buffer_offset = self.runtime_scratch_buffer_offset();
        if let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() {
            self.emit_loaded_schema_exact_size_check(size_offset, expected_size, &pattern.ty);
        }
        for (field, value) in &pattern.fields {
            let Some(layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get(field)).cloned() else {
                continue;
            };
            self.emit_loaded_field_equals_expected(size_offset, buffer_offset, &layout, value, &format!("{}.{}", pattern.ty, field));
        }
        self.emit_lifecycle_transition_check(pattern, size_offset, buffer_offset);
    }

    fn emit_lifecycle_transition_check(&mut self, pattern: &CreatePattern, output_size_offset: usize, output_buffer_offset: usize) {
        let Some(states) = self.lifecycle_states.get(&pattern.ty) else {
            return;
        };
        let state_count = states.len();
        let Some(consumed_var_id) = self.consumed_var_for_type(&pattern.ty) else {
            return;
        };
        let Some(input_size_offset) = self.cell_buffer_size_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let Some(input_buffer_offset) = self.cell_buffer_offsets.get(&consumed_var_id).copied() else {
            return;
        };
        let Some(state_layout) = self.type_layouts.get(&pattern.ty).and_then(|fields| fields.get("state")).cloned() else {
            return;
        };
        let Some(width) = fixed_scalar_width(&state_layout.ty, state_layout.fixed_size) else {
            return;
        };
        let Some(expected_size) = self.type_fixed_sizes.get(&pattern.ty).copied() else {
            return;
        };

        self.emit(format!("# cellscript abi: lifecycle transition {}.state old+1 state_count={}", pattern.ty, state_count));
        self.emit_loaded_schema_exact_size_check(input_size_offset, expected_size, &format!("{} input", pattern.ty));
        self.emit_loaded_schema_bounds_check(input_size_offset, state_layout.offset + width, &format!("{} input.state", pattern.ty));
        self.emit_loaded_schema_bounds_check(output_size_offset, state_layout.offset + width, &format!("{} output.state", pattern.ty));
        self.emit(format!("addi t4, sp, {}", input_buffer_offset));
        self.emit_unaligned_scalar_load("t4", "t0", "t2", state_layout.offset, width);
        let old_range_ok_label = self.fresh_label("lifecycle_old_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t0, t3");
        self.emit(format!("bnez t2, {}", old_range_ok_label));
        self.emit("li a0, 9");
        self.emit_epilogue();
        self.emit_label(&old_range_ok_label);

        self.emit(format!("addi t4, sp, {}", output_buffer_offset));
        self.emit_unaligned_scalar_load("t4", "t1", "t2", state_layout.offset, width);
        self.emit("addi t0, t0, 1");
        self.emit("sub t2, t1, t0");
        let ok_label = self.fresh_label("lifecycle_transition_ok");
        self.emit(format!("beqz t2, {}", ok_label));
        self.emit("li a0, 7");
        self.emit_epilogue();
        self.emit_label(&ok_label);

        let range_ok_label = self.fresh_label("lifecycle_state_range_ok");
        self.emit(format!("li t3, {}", state_count));
        self.emit("sltu t2, t1, t3");
        self.emit(format!("bnez t2, {}", range_ok_label));
        self.emit("li a0, 8");
        self.emit_epilogue();
        self.emit_label(&range_ok_label);
    }

    fn consumed_var_for_type(&self, type_name: &str) -> Option<usize> {
        self.consume_order
            .iter()
            .copied()
            .find(|var_id| self.consume_type_names.get(var_id).is_some_and(|consumed_type| consumed_type == type_name))
    }

    fn is_prelude_available_scalar(&self, operand: &IrOperand) -> bool {
        match operand {
            IrOperand::Const(IrConst::Bool(_) | IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_)) => true,
            IrOperand::Var(var) => {
                matches!(var.ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64)
                    && (self.param_vars.contains(&var.id)
                        || self.prelude_scalar_immediates.contains_key(&var.id)
                        || self.schema_field_value_sources.contains_key(&var.id)
                        || (var.ty == IrType::U64 && self.prelude_u64_value_sources.contains_key(&var.id)))
            }
            _ => false,
        }
    }

    fn emit_unaligned_scalar_load(&mut self, base_reg: &str, dest_reg: &str, scratch_reg: &str, offset: usize, width: usize) {
        self.emit(format!("li {}, 0", dest_reg));
        for byte_index in 0..width {
            self.emit(format!("lbu {}, {}({})", scratch_reg, offset + byte_index, base_reg));
            if byte_index != 0 {
                self.emit(format!("slli {}, {}, {}", scratch_reg, scratch_reg, byte_index * 8));
            }
            self.emit(format!("or {}, {}, {}", dest_reg, dest_reg, scratch_reg));
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!(".L{}_{}", prefix, self.next_runtime_label);
        self.next_runtime_label += 1;
        label
    }

    fn emit_param_spills(&mut self, params: &[IrParam]) -> Result<()> {
        let mut abi_index = 0usize;
        for param in params {
            if named_type_name(&param.ty).is_some() {
                self.emit(format!(
                    "# cellscript abi: schema param {} pointer={} length={}",
                    param.name,
                    abi_arg_label(abi_index),
                    abi_arg_label(abi_index + 1)
                ));
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                if let Some(size_offset) = self.schema_pointer_size_offsets.get(&param.binding.id).copied() {
                    self.emit_spill_abi_arg(abi_index + 1, size_offset);
                }
                abi_index += 2;
            } else {
                self.emit_spill_abi_arg(abi_index, param.binding.id * 8);
                abi_index += 1;
            }
        }

        Ok(())
    }

    fn emit_spill_abi_arg(&mut self, abi_index: usize, stack_offset: usize) {
        if abi_index < 8 {
            self.emit(format!("sd a{}, {}(sp)", abi_index, stack_offset));
        } else {
            let caller_stack_offset = (abi_index - 8) * 8;
            self.emit(format!("# cellscript abi: arg{} loaded from caller stack +{}", abi_index, caller_stack_offset));
            self.emit(format!("ld t0, {}(fp)", caller_stack_offset));
            self.emit(format!("sd t0, {}(sp)", stack_offset));
        }
    }

    fn record_instruction_var(&self, instruction: &IrInstruction, max_var_id: &mut Option<usize>) {
        match instruction {
            IrInstruction::LoadConst { dest, .. }
            | IrInstruction::LoadVar { dest, .. }
            | IrInstruction::Unary { dest, .. }
            | IrInstruction::FieldAccess { dest, .. }
            | IrInstruction::Index { dest, .. }
            | IrInstruction::Length { dest, .. }
            | IrInstruction::TypeHash { dest, .. }
            | IrInstruction::CollectionNew { dest, .. }
            | IrInstruction::Create { dest, .. }
            | IrInstruction::Claim { dest, .. }
            | IrInstruction::ReadRef { dest, .. } => self.record_var(dest, max_var_id),
            IrInstruction::Move { dest, src } => {
                self.record_var(dest, max_var_id);
                self.record_operand(src, max_var_id);
            }
            IrInstruction::Binary { dest, left, right, .. } => {
                self.record_var(dest, max_var_id);
                self.record_operand(left, max_var_id);
                self.record_operand(right, max_var_id);
            }
            IrInstruction::StoreVar { src, .. } => self.record_operand(src, max_var_id),
            IrInstruction::Call { dest, args, .. } => {
                if let Some(dest) = dest {
                    self.record_var(dest, max_var_id);
                }
                for arg in args {
                    self.record_operand(arg, max_var_id);
                }
            }
            IrInstruction::Consume { operand } | IrInstruction::Destroy { operand } | IrInstruction::Settle { operand } => {
                self.record_operand(operand, max_var_id)
            }
            IrInstruction::Transfer { dest, operand, to } => {
                self.record_var(dest, max_var_id);
                self.record_operand(operand, max_var_id);
                self.record_operand(to, max_var_id);
            }
            IrInstruction::CollectionPush { collection, value } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(value, max_var_id);
            }
            IrInstruction::CollectionExtend { collection, slice } => {
                self.record_operand(collection, max_var_id);
                self.record_operand(slice, max_var_id);
            }
        }
    }

    fn record_terminator_var(&self, terminator: &IrTerminator, max_var_id: &mut Option<usize>) {
        match terminator {
            IrTerminator::Return(Some(operand)) | IrTerminator::Branch { cond: operand, .. } => {
                self.record_operand(operand, max_var_id)
            }
            IrTerminator::Return(None) | IrTerminator::Jump(_) => {}
        }
    }

    fn record_operand(&self, operand: &IrOperand, max_var_id: &mut Option<usize>) {
        if let IrOperand::Var(var) = operand {
            self.record_var(var, max_var_id);
        }
    }

    fn record_var(&self, var: &IrVar, max_var_id: &mut Option<usize>) {
        *max_var_id = Some(max_var_id.map(|current| current.max(var.id)).unwrap_or(var.id));
    }

    /// 加载常量
    fn emit_load_const(&mut self, dest: &IrVar, value: &IrConst) -> Result<()> {
        match value {
            IrConst::U8(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U16(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U32(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U64(n) => self.emit(format!("li t0, {}", n)),
            IrConst::U128(_) => {
                // u128 需要两个寄存器
                self.emit("li t0, 0");
                self.emit("li t1, 0");
            }
            IrConst::Bool(b) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            IrConst::Address(_) | IrConst::Hash(_) => {
                // 加载地址/哈希（32字节）
                self.emit("la t0, __const_data");
            }
            IrConst::Array(_) => {
                self.emit("la t0, __const_data");
            }
        }
        // 存储到栈
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    /// 加载变量
    fn emit_load_var(&mut self, dest: &IrVar, name: &str) -> Result<()> {
        // 简化处理：假设变量在栈上
        self.emit(format!("# load var {}", name));
        self.emit(format!("ld t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    /// 存储变量
    fn emit_store_var(&mut self, name: &str, src: &IrOperand) -> Result<()> {
        self.emit(format!("# store var {}", name));
        match src {
            IrOperand::Const(c) => match c {
                IrConst::U64(n) => self.emit(format!("li t0, {}", n)),
                _ => self.emit("li t0, 0"),
            },
            IrOperand::Var(v) => {
                self.emit(format!("ld t0, {}(sp)", v.id * 8));
            }
        }
        Ok(())
    }

    /// 二元运算
    fn emit_binary(&mut self, dest: &IrVar, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> Result<()> {
        // 加载左操作数
        match left {
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            IrOperand::Var(v) => self.emit(format!("ld t0, {}(sp)", v.id * 8)),
            _ => self.emit("li t0, 0"),
        }

        // 加载右操作数
        match right {
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            IrOperand::Var(v) => self.emit(format!("ld t1, {}(sp)", v.id * 8)),
            _ => self.emit("li t1, 0"),
        }

        // 执行运算
        match op {
            BinaryOp::Add => self.emit("add t0, t0, t1"),
            BinaryOp::Sub => self.emit("sub t0, t0, t1"),
            BinaryOp::Mul => self.emit("mul t0, t0, t1"),
            BinaryOp::Div => self.emit("div t0, t0, t1"),
            BinaryOp::Mod => self.emit("rem t0, t0, t1"),
            BinaryOp::Eq => {
                self.emit("sub t0, t0, t1");
                self.emit("seqz t0, t0");
            }
            BinaryOp::Ne => {
                self.emit("sub t0, t0, t1");
                self.emit("snez t0, t0");
            }
            BinaryOp::Lt => self.emit("slt t0, t0, t1"),
            BinaryOp::Le => {
                self.emit("sgt t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::Gt => self.emit("sgt t0, t0, t1"),
            BinaryOp::Ge => {
                self.emit("slt t0, t0, t1");
                self.emit("xori t0, t0, 1");
            }
            BinaryOp::And => self.emit("and t0, t0, t1"),
            BinaryOp::Or => self.emit("or t0, t0, t1"),
        }

        // 存储结果
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    /// 一元运算
    fn emit_unary(&mut self, dest: &IrVar, op: UnaryOp, operand: &IrOperand) -> Result<()> {
        // 加载操作数
        match operand {
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            IrOperand::Var(v) => self.emit(format!("ld t0, {}(sp)", v.id * 8)),
            _ => self.emit("li t0, 0"),
        }

        match op {
            UnaryOp::Neg => self.emit("neg t0, t0"),
            UnaryOp::Not => self.emit("xori t0, t0, 1"),
            UnaryOp::Ref | UnaryOp::Deref => self.emit("# reference conversion (no-op in asm backend)"),
        }

        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    /// 字段访问
    fn emit_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> Result<()> {
        if self.emit_schema_field_access(dest, obj, field) {
            return Ok(());
        }

        self.requires_symbolic_runtime = true;
        self.emit(format!("# field access .{}", field));
        match obj {
            IrOperand::Var(v) => self.emit(format!("ld t0, {}(sp)", v.id * 8)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            _ => self.emit("li t0, 0"),
        }
        self.emit(format!("addi t0, t0, {}", self.symbolic_field_tag(field)));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.emit_symbolic_runtime_fail_closed("field access");
        Ok(())
    }

    fn emit_schema_field_access(&mut self, dest: &IrVar, obj: &IrOperand, field: &str) -> bool {
        let IrOperand::Var(var) = obj else {
            return false;
        };
        if !self.schema_pointer_vars.contains(&var.id) {
            return false;
        }
        let Some(type_name) = named_type_name(&var.ty) else {
            return false;
        };
        let Some(layout) = self.type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
            return false;
        };
        let Some(width) = fixed_scalar_width(&layout.ty, layout.fixed_size) else {
            return false;
        };

        self.emit(format!("# field access .{}", field));
        self.emit(format!("# cellscript abi: schema field {}.{} offset={} size={}", type_name, field, layout.offset, width));
        if let Some(size_offset) = self.schema_pointer_size_offsets.get(&var.id).copied() {
            if let Some(expected_size) = self.type_fixed_sizes.get(type_name).copied() {
                self.emit_loaded_schema_exact_size_check(size_offset, expected_size, type_name);
            }
            self.emit_loaded_schema_bounds_check(size_offset, layout.offset + width, &format!("{}.{}", type_name, field));
        }
        self.emit(format!("ld t4, {}(sp)", var.id * 8));
        self.emit_unaligned_scalar_load("t4", "t0", "t2", layout.offset, width);
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        true
    }

    /// 数组索引
    fn emit_index(&mut self, dest: &IrVar, arr: &IrOperand, idx: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# index access");
        match arr {
            IrOperand::Var(v) => self.emit(format!("ld t0, {}(sp)", v.id * 8)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            _ => self.emit("li t0, 0"),
        }
        match idx {
            IrOperand::Var(v) => self.emit(format!("ld t1, {}(sp)", v.id * 8)),
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t1, {}", n)),
            _ => self.emit("li t1, 0"),
        }
        self.emit("slli t1, t1, 4");
        self.emit("add t0, t0, t1");
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.emit_symbolic_runtime_fail_closed("index access");
        Ok(())
    }

    fn emit_length(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.emit("# length");
        if let Some(static_len) = self.static_length(operand) {
            self.emit(format!("li t0, {}", static_len));
        } else {
            self.requires_symbolic_runtime = true;
            self.emit("li t0, 4");
            self.emit_symbolic_runtime_fail_closed("dynamic length");
        }
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    fn emit_type_hash(&mut self, dest: &IrVar, operand: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# type_hash");
        self.emit(format!("li t0, {}", self.symbolic_type_tag(operand)));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.emit_symbolic_runtime_fail_closed("type_hash");
        Ok(())
    }

    fn emit_collection_new(&mut self, dest: &IrVar, ty: &str) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit(format!("# collection new {}", ty));
        self.emit(format!("li t0, {}", 0x2000usize + self.next_virtual_output * 0x40 + self.symbolic_collection_tag(ty)));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.next_virtual_output += 1;
        self.emit_symbolic_runtime_fail_closed("collection new");
        Ok(())
    }

    fn emit_collection_push(&mut self, collection: &IrOperand, value: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# collection push");
        self.emit_symbolic_operand_comment("collection", collection);
        self.emit_symbolic_operand_comment("value", value);
        self.emit_symbolic_runtime_fail_closed("collection push");
        Ok(())
    }

    fn emit_collection_extend(&mut self, collection: &IrOperand, slice: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# collection extend_from_slice");
        self.emit_symbolic_operand_comment("collection", collection);
        self.emit_symbolic_operand_comment("slice", slice);
        self.emit_symbolic_runtime_fail_closed("collection extend");
        Ok(())
    }

    /// 函数调用
    fn emit_call(&mut self, dest: Option<&IrVar>, func: &str, args: &[IrOperand]) -> Result<()> {
        if func.contains("::") {
            return Err(CompileError::new(
                format!("external function call '{}' is not linkable yet; importable function summaries are only used for type/effect checking", func),
                crate::error::Span::default(),
            ));
        }
        self.emit(format!("# call {}", func));

        // 设置参数
        for (i, arg) in args.iter().enumerate() {
            match arg {
                IrOperand::Const(IrConst::U64(n)) => {
                    self.emit(format!("li a{}, {}", i, n));
                }
                IrOperand::Var(v) => {
                    self.emit(format!("ld a{}, {}(sp)", i, v.id * 8));
                }
                _ => {}
            }
        }

        // 调用
        self.emit(format!("call {}", func));

        // 保存返回值
        if let Some(d) = dest {
            self.emit(format!("sd a0, {}(sp)", d.id * 8));
        }

        Ok(())
    }

    fn emit_read_ref(&mut self, dest: &IrVar, ty: &str) -> Result<()> {
        if self.cell_buffer_offsets.contains_key(&dest.id) {
            self.emit(format!("# read_ref {} (preloaded from CellDep)", ty));
            return Ok(());
        }

        self.requires_symbolic_runtime = true;
        self.emit(format!("# read_ref {} (symbolic fallback)", ty));
        self.emit(format!("li t0, {}", 0x1000usize + self.next_virtual_output * 0x40));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.next_virtual_output += 1;
        self.emit_symbolic_runtime_fail_closed("read_ref");
        Ok(())
    }

    fn emit_move(&mut self, dest: &IrVar, src: &IrOperand) -> Result<()> {
        match src {
            IrOperand::Const(IrConst::U64(n)) => self.emit(format!("li t0, {}", n)),
            IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("li t0, {}", if *b { 1 } else { 0 })),
            IrOperand::Var(v) => self.emit(format!("ld t0, {}(sp)", v.id * 8)),
            _ => self.emit("li t0, 0"),
        }
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        Ok(())
    }

    /// consume
    fn emit_consume(&mut self, operand: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# consume");
        if let IrOperand::Var(var) = operand {
            if self.consume_indices.contains_key(&var.id) {
                self.emit("# cellscript abi: consumed input pointer retained for verifier field checks");
                return Ok(());
            }
            self.emit(format!("sd zero, {}(sp)", var.id * 8));
        }
        self.emit_symbolic_runtime_fail_closed("consume");
        Ok(())
    }

    /// create
    fn emit_create(&mut self, dest: &IrVar, pattern: &CreatePattern) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit(format!("# create {}", pattern.ty));
        for (field, value) in &pattern.fields {
            match value {
                IrOperand::Const(IrConst::U64(n)) => self.emit(format!("#   field {} = {}", field, n)),
                IrOperand::Const(IrConst::Bool(b)) => self.emit(format!("#   field {} = {}", field, b)),
                IrOperand::Var(var) => self.emit(format!("#   field {} <- {}", field, var.name)),
                _ => self.emit(format!("#   field {} <- <value>", field)),
            }
        }
        if pattern.lock.is_some() {
            self.emit("#   with_lock <expr>");
        }
        self.emit(format!("li t0, {}", self.next_virtual_output));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.next_virtual_output += 1;
        Ok(())
    }

    /// transfer
    fn emit_transfer(&mut self, dest: &IrVar, operand: &IrOperand, to: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# transfer");
        self.emit_symbolic_operand_comment("asset", operand);
        self.emit_symbolic_operand_comment("to", to);
        self.emit(format!("li t0, {}", 0x2000usize + self.next_virtual_output * 0x40));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.next_virtual_output += 1;
        self.emit_symbolic_runtime_fail_closed("transfer");
        Ok(())
    }

    /// destroy
    fn emit_destroy(&mut self, _operand: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# destroy");
        if let IrOperand::Var(var) = _operand {
            self.emit(format!("sd zero, {}(sp)", var.id * 8));
        }
        self.emit_symbolic_runtime_fail_closed("destroy");
        Ok(())
    }

    fn symbolic_field_tag(&self, field: &str) -> usize {
        field.bytes().fold(0usize, |acc, byte| acc.wrapping_mul(131).wrapping_add(byte as usize)) % 0x400
    }

    fn symbolic_collection_tag(&self, ty: &str) -> usize {
        ty.bytes().fold(0usize, |acc, byte| acc.wrapping_mul(167).wrapping_add(byte as usize)) % 0x400
    }

    fn symbolic_type_tag(&self, operand: &IrOperand) -> usize {
        let repr = match operand {
            IrOperand::Var(var) => format!("{:?}", var.ty),
            IrOperand::Const(IrConst::Address(_)) => "Address".to_string(),
            IrOperand::Const(IrConst::Hash(_)) => "Hash".to_string(),
            IrOperand::Const(IrConst::Bool(_)) => "Bool".to_string(),
            IrOperand::Const(IrConst::U8(_)) => "U8".to_string(),
            IrOperand::Const(IrConst::U16(_)) => "U16".to_string(),
            IrOperand::Const(IrConst::U32(_)) => "U32".to_string(),
            IrOperand::Const(IrConst::U64(_)) => "U64".to_string(),
            IrOperand::Const(IrConst::U128(_)) => "U128".to_string(),
            IrOperand::Const(IrConst::Array(items)) => format!("Array({})", items.len()),
        };
        repr.bytes().fold(0usize, |acc, byte| acc.wrapping_mul(181).wrapping_add(byte as usize)) % 0x1000
    }

    fn emit_symbolic_operand_comment(&mut self, label: &str, operand: &IrOperand) {
        let rendered = match operand {
            IrOperand::Var(var) => format!("{}: {}", label, var.name),
            IrOperand::Const(IrConst::U64(n)) => format!("{}: {}", label, n),
            IrOperand::Const(IrConst::Bool(b)) => format!("{}: {}", label, b),
            IrOperand::Const(IrConst::Address(_)) => format!("{}: <address>", label),
            IrOperand::Const(IrConst::Hash(_)) => format!("{}: <hash>", label),
            IrOperand::Const(IrConst::Array(items)) => format!("{}: <array:{}>", label, items.len()),
            IrOperand::Const(_) => format!("{}: <const>", label),
        };
        self.emit(format!("#   {}", rendered));
    }

    fn static_length(&self, operand: &IrOperand) -> Option<usize> {
        match operand {
            IrOperand::Var(var) => self.static_length_from_type(&var.ty),
            IrOperand::Const(IrConst::Array(items)) => Some(items.len()),
            _ => None,
        }
    }

    fn static_length_from_type(&self, ty: &IrType) -> Option<usize> {
        match ty {
            IrType::Array(_, size) => Some(*size),
            IrType::Ref(inner) | IrType::MutRef(inner) => self.static_length_from_type(inner),
            _ => None,
        }
    }

    /// claim
    fn emit_claim(&mut self, dest: &IrVar, receipt: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# claim");
        self.emit_symbolic_operand_comment("receipt", receipt);
        self.emit(format!("li t0, {}", 0x3000usize + self.next_virtual_output * 0x40));
        self.emit(format!("sd t0, {}(sp)", dest.id * 8));
        self.next_virtual_output += 1;
        self.emit_symbolic_runtime_fail_closed("claim");
        Ok(())
    }

    /// settle
    fn emit_settle(&mut self, operand: &IrOperand) -> Result<()> {
        self.requires_symbolic_runtime = true;
        self.emit("# settle");
        self.emit_symbolic_operand_comment("value", operand);
        self.emit_symbolic_runtime_fail_closed("settle");
        Ok(())
    }

    /// 生成运行时支持函数
    fn generate_runtime_support(&mut self) {
        self.emit_section(".text");

        self.emit_global("__env_current_daa_score");
        self.emit_label("__env_current_daa_score");
        self.emit("addi sp, sp, -32");
        self.emit("sd ra, 24(sp)");
        self.emit("# cellscript abi: LOAD_HEADER_BY_FIELD field=daa_score source=HeaderDep index=0");
        self.emit("li t0, 8");
        self.emit("sd t0, 8(sp)");
        self.emit("addi a0, sp, 16");
        self.emit("addi a1, sp, 8");
        self.emit("li a2, 0");
        self.emit("li a3, 0");
        self.emit("li a4, 4");
        self.emit("li a5, 0");
        self.emit("li a7, 2082");
        self.emit("ecall");
        self.emit("ld a0, 16(sp)");
        self.emit("ld ra, 24(sp)");
        self.emit("addi sp, sp, 32");
        self.emit("ret");
    }

    /// 汇编为 ELF
    fn assemble(&self, format: ArtifactFormat) -> Result<Vec<u8>> {
        let assembly_text = self.assembly.join("\n");
        match format {
            ArtifactFormat::RiscvAssembly => Ok(assembly_text.into_bytes()),
            ArtifactFormat::RiscvElf => {
                if self.requires_symbolic_runtime {
                    Err(CompileError::new(
                        "riscv64-elf emission is not yet supported for symbolic cell/runtime operations like read_ref, field access, index, type_hash, collection operations, create, consume, transfer, claim, settle, or destroy; use riscv64-asm for those programs today",
                        crate::error::Span::default(),
                    ))
                } else {
                    assemble_elf(&self.assembly)
                }
            }
        }
    }
}

/// 代码生成入口函数
pub fn generate(ir: &IrModule, options: &CodegenOptions, format: ArtifactFormat) -> Result<Vec<u8>> {
    let generator = CodeGenerator::new(options.clone());
    generator.generate(ir, format)
}

fn named_type_name(ty: &IrType) -> Option<&str> {
    match ty {
        IrType::Named(name) => Some(name.as_str()),
        IrType::Ref(inner) | IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

fn consumed_operand_var(instruction: &IrInstruction) -> Option<&IrVar> {
    let operand = match instruction {
        IrInstruction::Consume { operand } | IrInstruction::Transfer { operand, .. } | IrInstruction::Settle { operand } => operand,
        IrInstruction::Claim { receipt, .. } => receipt,
        _ => return None,
    };
    match operand {
        IrOperand::Var(var) if named_type_name(&var.ty).is_some() => Some(var),
        _ => None,
    }
}

const ELF_HEADER_SIZE: usize = 64;
const ELF_PROGRAM_HEADER_SIZE: usize = 56;
const ELF_SEGMENT_ALIGN: usize = 0x1000;
const ELF_BASE_ADDR: u64 = 0x10000;
const START_TRAMPOLINE_SIZE: usize = 20;
const EXIT_SYSCALL_NUMBER: i64 = 93;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SectionKind {
    Text,
    Rodata,
}

#[derive(Debug, Clone)]
enum AsmOp {
    Label,
    Instruction(Instruction),
    Word(u32),
    Byte(u8),
    Ascii(Vec<u8>),
    Align(usize),
}

#[derive(Debug, Clone, Copy)]
struct SymbolDef {
    section: SectionKind,
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct SectionLayout {
    text_base: u64,
    text_user_base: u64,
    rodata_base: u64,
}

#[derive(Debug, Clone)]
enum Instruction {
    Addi { rd: u8, rs1: u8, imm: i64 },
    Add { rd: u8, rs1: u8, rs2: u8 },
    Sub { rd: u8, rs1: u8, rs2: u8 },
    And { rd: u8, rs1: u8, rs2: u8 },
    Or { rd: u8, rs1: u8, rs2: u8 },
    Mul { rd: u8, rs1: u8, rs2: u8 },
    Div { rd: u8, rs1: u8, rs2: u8 },
    Rem { rd: u8, rs1: u8, rs2: u8 },
    Slt { rd: u8, rs1: u8, rs2: u8 },
    Sgt { rd: u8, rs1: u8, rs2: u8 },
    Xori { rd: u8, rs1: u8, imm: i64 },
    Seqz { rd: u8, rs: u8 },
    Snez { rd: u8, rs: u8 },
    Neg { rd: u8, rs: u8 },
    Ld { rd: u8, rs1: u8, imm: i64 },
    Lbu { rd: u8, rs1: u8, imm: i64 },
    Sd { rs2: u8, rs1: u8, imm: i64 },
    Slli { rd: u8, rs1: u8, shamt: i64 },
    Li { rd: u8, imm: i64 },
    La { rd: u8, label: String },
    Call { label: String },
    Jump { label: String },
    Beqz { rs: u8, label: String },
    Ret,
    Ecall,
}

fn assemble_elf(lines: &[String]) -> Result<Vec<u8>> {
    if let Some(external) = try_external_elf_toolchain(lines)? {
        return Ok(external);
    }
    assemble_elf_internal(lines)
}

fn assemble_elf_internal(lines: &[String]) -> Result<Vec<u8>> {
    let parsed = ParsedAssembly::from_lines(lines)?;
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;

    let text_user_size = parsed.section_size(SectionKind::Text);
    let rodata_size = parsed.section_size(SectionKind::Rodata);
    let rodata_offset = align_up(START_TRAMPOLINE_SIZE + text_user_size, 8);
    let layout = SectionLayout {
        text_base: ELF_BASE_ADDR,
        text_user_base: ELF_BASE_ADDR + START_TRAMPOLINE_SIZE as u64,
        rodata_base: ELF_BASE_ADDR + rodata_offset as u64,
    };

    let mut text_bytes = Vec::with_capacity(START_TRAMPOLINE_SIZE + text_user_size);
    let entry_addr = parsed.symbol_address(entry_label, &layout)?;
    encode_call_sequence(&mut text_bytes, layout.text_base, entry_addr)?;
    encode_li_sequence(&mut text_bytes, 17, EXIT_SYSCALL_NUMBER)?;
    text_bytes.extend_from_slice(&encode_ecall().to_le_bytes());
    parsed.encode_section(SectionKind::Text, &mut text_bytes, &layout, START_TRAMPOLINE_SIZE)?;

    let mut rodata_bytes = Vec::with_capacity(rodata_size);
    parsed.encode_section(SectionKind::Rodata, &mut rodata_bytes, &layout, 0)?;

    let segment_size = rodata_offset + rodata_bytes.len();
    let segment_file_offset = align_up(ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE, ELF_SEGMENT_ALIGN);
    let mut elf = vec![0u8; segment_file_offset + segment_size];
    write_elf_header(&mut elf[..ELF_HEADER_SIZE], layout.text_base)?;
    write_program_header(
        &mut elf[ELF_HEADER_SIZE..ELF_HEADER_SIZE + ELF_PROGRAM_HEADER_SIZE],
        segment_file_offset as u64,
        layout.text_base,
        segment_size as u64,
    )?;

    let segment = &mut elf[segment_file_offset..segment_file_offset + segment_size];
    segment[..text_bytes.len()].copy_from_slice(&text_bytes);
    segment[rodata_offset..rodata_offset + rodata_bytes.len()].copy_from_slice(&rodata_bytes);
    Ok(elf)
}

fn try_external_elf_toolchain(lines: &[String]) -> Result<Option<Vec<u8>>> {
    let Some(toolchain) = discover_external_toolchain()? else {
        return Ok(None);
    };
    let parsed = ParsedAssembly::from_lines(lines)?;
    let entry_label = parsed.entry_label.as_deref().ok_or_else(|| {
        CompileError::new("ELF target requires at least one action or lock entry point", crate::error::Span::default())
    })?;

    let temp_dir = make_external_toolchain_temp_dir()?;
    let asm_path = temp_dir.join("module.s");
    let elf_path = temp_dir.join("module.elf");
    let obj_path = temp_dir.join("module.o");
    fs::write(&asm_path, render_external_assembly(lines, entry_label)).map_err(|err| {
        CompileError::new(
            format!("failed to write temporary assembly file '{}': {}", asm_path.display(), err),
            crate::error::Span::default(),
        )
    })?;

    let external_result = match &toolchain.mode {
        ExternalToolchainMode::Compiler(compiler) => run_external_command(
            Command::new(compiler)
                .arg("-nostdlib")
                .arg("-march=rv64imac")
                .arg("-mabi=lp64")
                .arg("-Wl,-e,_start")
                .arg("-Wl,-Ttext=0x10000")
                .arg("-o")
                .arg(&elf_path)
                .arg(&asm_path),
            &toolchain,
            "RISC-V compiler",
        ),
        ExternalToolchainMode::AssemblerLinker { assembler, linker } => run_external_command(
            Command::new(assembler).arg("-march=rv64imac").arg("-mabi=lp64").arg(&asm_path).arg("-o").arg(&obj_path),
            &toolchain,
            "RISC-V assembler",
        )
        .and_then(|_| {
            run_external_command(
                Command::new(linker)
                    .arg("-m")
                    .arg("elf64lriscv")
                    .arg("-e")
                    .arg("_start")
                    .arg("-Ttext")
                    .arg("0x10000")
                    .arg("-o")
                    .arg(&elf_path)
                    .arg(&obj_path),
                &toolchain,
                "RISC-V linker",
            )
        }),
    };

    let elf = match external_result {
        Ok(()) => fs::read(&elf_path).map_err(|err| {
            CompileError::new(
                format!("failed to read external ELF output '{}': {}", elf_path.display(), err),
                crate::error::Span::default(),
            )
        }),
        Err(err) if toolchain.explicit => Err(err),
        Err(_) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Ok(None);
        }
    };

    let elf = elf
        .and_then(|bytes| {
            if bytes.starts_with(b"\x7fELF") {
                Ok(bytes)
            } else {
                Err(CompileError::new(
                    format!("external toolchain output '{}' is not an ELF file", elf_path.display()),
                    crate::error::Span::default(),
                ))
            }
        })
        .or_else(|err| if toolchain.explicit { Err(err) } else { Ok(Vec::new()) })?;

    if elf.is_empty() && !toolchain.explicit {
        let _ = fs::remove_dir_all(&temp_dir);
        return Ok(None);
    }

    let _ = fs::remove_dir_all(&temp_dir);
    Ok(Some(elf))
}

fn render_external_assembly(lines: &[String], entry_label: &str) -> String {
    let mut rendered = vec![
        ".section .text".to_string(),
        ".global _start".to_string(),
        ".type _start, @function".to_string(),
        "_start:".to_string(),
        format!("    call {}", entry_label),
        format!("    li a7, {}", EXIT_SYSCALL_NUMBER),
        "    ecall".to_string(),
    ];
    rendered.extend(lines.iter().filter(|line| !line.trim_start().starts_with(".option arch,")).cloned());
    let mut rendered = rendered.join("\n");
    rendered.push('\n');
    rendered
}

#[derive(Debug, Clone)]
struct ExternalToolchain {
    mode: ExternalToolchainMode,
    explicit: bool,
}

#[derive(Debug, Clone)]
enum ExternalToolchainMode {
    Compiler(PathBuf),
    AssemblerLinker { assembler: PathBuf, linker: PathBuf },
}

fn discover_external_toolchain() -> Result<Option<ExternalToolchain>> {
    let explicit_compiler = env::var_os("CELLSCRIPT_RISCV_CC").map(PathBuf::from);
    let explicit_assembler = env::var_os("CELLSCRIPT_RISCV_AS").map(PathBuf::from);
    let explicit_linker = env::var_os("CELLSCRIPT_RISCV_LD").map(PathBuf::from);

    if let Some(compiler) = explicit_compiler {
        if explicit_assembler.is_some() || explicit_linker.is_some() {
            return Err(CompileError::new(
                "set either CELLSCRIPT_RISCV_CC or CELLSCRIPT_RISCV_AS/CELLSCRIPT_RISCV_LD, not both",
                crate::error::Span::default(),
            ));
        }
        return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::Compiler(compiler), explicit: true }));
    }

    match (explicit_assembler, explicit_linker) {
        (Some(assembler), Some(linker)) => {
            return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::AssemblerLinker { assembler, linker }, explicit: true }))
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CompileError::new(
                "CELLSCRIPT_RISCV_AS and CELLSCRIPT_RISCV_LD must be set together",
                crate::error::Span::default(),
            ))
        }
        (None, None) => {}
    }

    if let Some(compiler) = find_first_tool(&[
        "riscv64-unknown-elf-gcc",
        "riscv64-elf-gcc",
        "riscv64-none-elf-gcc",
        "riscv64-linux-gnu-gcc",
        "riscv64-unknown-elf-clang",
    ]) {
        return Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::Compiler(compiler), explicit: false }));
    }

    match (
        find_first_tool(&["riscv64-unknown-elf-as", "riscv64-elf-as", "riscv64-none-elf-as", "riscv64-linux-gnu-as"]),
        find_first_tool(&["riscv64-unknown-elf-ld", "riscv64-elf-ld", "riscv64-none-elf-ld", "riscv64-linux-gnu-ld"]),
    ) {
        (Some(assembler), Some(linker)) => {
            Ok(Some(ExternalToolchain { mode: ExternalToolchainMode::AssemblerLinker { assembler, linker }, explicit: false }))
        }
        _ => Ok(None),
    }
}

fn run_external_command(command: &mut Command, toolchain: &ExternalToolchain, label: &str) -> Result<()> {
    let rendered = render_command(command);
    let output = command.output().map_err(|err| {
        CompileError::new(format!("failed to launch {} ({}): {}", label, rendered, err), crate::error::Span::default())
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!("{} failed ({}): {}", label, rendered, stderr.trim());
    if toolchain.explicit {
        return Err(CompileError::new(message, crate::error::Span::default()));
    }

    Err(CompileError::new(
        format!("autodetected external toolchain failed, falling back to built-in ELF assembler: {}", message),
        crate::error::Span::default(),
    ))
}

fn render_command(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command.get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{} {}", program, args)
    }
}

fn make_external_toolchain_temp_dir() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    let dir = env::temp_dir().join(format!("cellscript-riscv-{}-{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).map_err(|err| {
        CompileError::new(
            format!("failed to create temporary toolchain directory '{}': {}", dir.display(), err),
            crate::error::Span::default(),
        )
    })?;
    Ok(dir)
}

fn find_first_tool(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().find_map(|candidate| find_in_path(candidate))
}

fn find_in_path(tool: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).map(|dir| dir.join(tool)).find(|candidate| candidate.is_file() && is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map(|meta| meta.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[derive(Debug, Default)]
struct ParsedAssembly {
    text_ops: Vec<AsmOp>,
    rodata_ops: Vec<AsmOp>,
    text_size: usize,
    rodata_size: usize,
    symbols: HashMap<String, SymbolDef>,
    entry_label: Option<String>,
}

impl ParsedAssembly {
    fn from_lines(lines: &[String]) -> Result<Self> {
        let mut current_section = SectionKind::Text;
        let mut text_size = 0usize;
        let mut rodata_size = 0usize;
        let mut text_ops = Vec::new();
        let mut rodata_ops = Vec::new();
        let mut symbols = HashMap::new();
        let mut globals = BTreeSet::new();
        let mut entry_label = None;
        let mut fallback_entry = None;

        for line in lines {
            let Some(clean) = strip_comment(line) else {
                continue;
            };
            if clean.is_empty() {
                continue;
            }

            if let Some(section) = parse_section_directive(clean)? {
                current_section = section;
                continue;
            }
            if clean.starts_with(".option ") || clean.starts_with(".type ") {
                continue;
            }
            if let Some(symbol) = clean.strip_prefix(".global ") {
                globals.insert(symbol.trim().to_string());
                continue;
            }

            let (ops, offset) = match current_section {
                SectionKind::Text => (&mut text_ops, &mut text_size),
                SectionKind::Rodata => (&mut rodata_ops, &mut rodata_size),
            };

            if let Some(label) = clean.strip_suffix(':') {
                let label = label.trim().to_string();
                let symbol = SymbolDef { section: current_section, offset: *offset };
                if symbols.insert(label.clone(), symbol).is_some() {
                    return Err(CompileError::new(format!("duplicate assembly label '{}'", label), crate::error::Span::default()));
                }
                if current_section == SectionKind::Text && globals.contains(&label) {
                    if fallback_entry.is_none() {
                        fallback_entry = Some(label.clone());
                    }
                    if !label.starts_with("__") && entry_label.is_none() {
                        entry_label = Some(label.clone());
                    }
                }
                ops.push(AsmOp::Label);
                continue;
            }

            let op = parse_asm_op(clean)?;
            *offset += op_size(&op, *offset);
            ops.push(op);
        }

        Ok(Self { text_ops, rodata_ops, text_size, rodata_size, symbols, entry_label: entry_label.or(fallback_entry) })
    }

    fn section_size(&self, section: SectionKind) -> usize {
        match section {
            SectionKind::Text => self.text_size,
            SectionKind::Rodata => self.rodata_size,
        }
    }

    fn symbol_address(&self, label: &str, layout: &SectionLayout) -> Result<u64> {
        let symbol = self
            .symbols
            .get(label)
            .ok_or_else(|| CompileError::new(format!("unknown assembly label '{}'", label), crate::error::Span::default()))?;
        Ok(match symbol.section {
            SectionKind::Text => layout.text_user_base + symbol.offset as u64,
            SectionKind::Rodata => layout.rodata_base + symbol.offset as u64,
        })
    }

    fn encode_section(&self, section: SectionKind, out: &mut Vec<u8>, layout: &SectionLayout, base_bias: usize) -> Result<()> {
        let ops = match section {
            SectionKind::Text => &self.text_ops,
            SectionKind::Rodata => &self.rodata_ops,
        };
        let section_base = match section {
            SectionKind::Text => layout.text_user_base,
            SectionKind::Rodata => layout.rodata_base,
        };

        for op in ops {
            match op {
                AsmOp::Label => {}
                AsmOp::Word(word) => out.extend_from_slice(&word.to_le_bytes()),
                AsmOp::Byte(byte) => out.push(*byte),
                AsmOp::Ascii(bytes) => out.extend_from_slice(bytes),
                AsmOp::Align(bytes) => pad_to_alignment(out, *bytes),
                AsmOp::Instruction(inst) => {
                    let pc = section_base + (out.len() + base_bias) as u64;
                    encode_instruction(out, inst, pc, self, layout)?;
                }
            }
        }

        Ok(())
    }
}

fn parse_section_directive(line: &str) -> Result<Option<SectionKind>> {
    if let Some(section) = line.strip_prefix(".section ") {
        return match section.trim() {
            ".text" => Ok(Some(SectionKind::Text)),
            ".rodata" => Ok(Some(SectionKind::Rodata)),
            other => Err(CompileError::new(format!("unsupported assembly section '{}'", other), crate::error::Span::default())),
        };
    }
    Ok(None)
}

fn parse_asm_op(line: &str) -> Result<AsmOp> {
    if let Some(value) = line.strip_prefix(".word ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Word(
            u32::try_from(value).map_err(|_| {
                CompileError::new(format!("'.word' value '{}' does not fit u32", value), crate::error::Span::default())
            })?,
        ));
    }
    if let Some(value) = line.strip_prefix(".byte ") {
        let value = parse_immediate(value.trim())?;
        return Ok(AsmOp::Byte(
            u8::try_from(value)
                .map_err(|_| CompileError::new(format!("'.byte' value '{}' does not fit u8", value), crate::error::Span::default()))?,
        ));
    }
    if let Some(value) = line.strip_prefix(".ascii ") {
        return Ok(AsmOp::Ascii(parse_ascii_literal(value.trim())?));
    }
    if let Some(value) = line.strip_prefix(".align ") {
        let align_pow = parse_immediate(value.trim())?;
        if !(0..=16).contains(&align_pow) {
            return Err(CompileError::new(format!("unsupported .align value '{}'", align_pow), crate::error::Span::default()));
        }
        return Ok(AsmOp::Align(1usize << (align_pow as usize)));
    }
    Ok(AsmOp::Instruction(parse_instruction(line)?))
}

fn parse_instruction(line: &str) -> Result<Instruction> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let opcode = parts.next().unwrap().trim();
    let args = parts.next().unwrap_or("").trim();
    let args = if args.is_empty() { Vec::new() } else { args.split(',').map(|arg| arg.trim().to_string()).collect() };

    match opcode {
        "addi" => Ok(Instruction::Addi {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "add" => Ok(Instruction::Add {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sub" => Ok(Instruction::Sub {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "and" => Ok(Instruction::And {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "or" => Ok(Instruction::Or {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "mul" => Ok(Instruction::Mul {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "div" => Ok(Instruction::Div {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "rem" => Ok(Instruction::Rem {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "slt" => Ok(Instruction::Slt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "sgt" => Ok(Instruction::Sgt {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            rs2: parse_register(arg(&args, 2)?)?,
        }),
        "xori" => Ok(Instruction::Xori {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            imm: parse_immediate(arg(&args, 2)?)?,
        }),
        "seqz" => Ok(Instruction::Seqz { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "snez" => Ok(Instruction::Snez { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "neg" => Ok(Instruction::Neg { rd: parse_register(arg(&args, 0)?)?, rs: parse_register(arg(&args, 1)?)? }),
        "ld" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Ld { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "lbu" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Lbu { rd: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "sd" => {
            let (imm, rs1) = parse_memory_operand(arg(&args, 1)?)?;
            Ok(Instruction::Sd { rs2: parse_register(arg(&args, 0)?)?, rs1, imm })
        }
        "slli" => Ok(Instruction::Slli {
            rd: parse_register(arg(&args, 0)?)?,
            rs1: parse_register(arg(&args, 1)?)?,
            shamt: parse_immediate(arg(&args, 2)?)?,
        }),
        "li" => Ok(Instruction::Li { rd: parse_register(arg(&args, 0)?)?, imm: parse_immediate(arg(&args, 1)?)? }),
        "la" => Ok(Instruction::La { rd: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "call" => Ok(Instruction::Call { label: arg(&args, 0)?.to_string() }),
        "j" => Ok(Instruction::Jump { label: arg(&args, 0)?.to_string() }),
        "beqz" => Ok(Instruction::Beqz { rs: parse_register(arg(&args, 0)?)?, label: arg(&args, 1)?.to_string() }),
        "ret" => Ok(Instruction::Ret),
        "ecall" => Ok(Instruction::Ecall),
        other => Err(CompileError::new(format!("unsupported assembly instruction '{}'", other), crate::error::Span::default())),
    }
}

fn encode_instruction(out: &mut Vec<u8>, inst: &Instruction, pc: u64, parsed: &ParsedAssembly, layout: &SectionLayout) -> Result<()> {
    match inst {
        Instruction::Addi { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b000, *rs1, *imm)?.to_le_bytes()),
        Instruction::Add { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sub { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0100000).to_le_bytes())
        }
        Instruction::And { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b111, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Or { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Mul { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Div { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b100, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b110, *rs1, *rs2, 0b0000001).to_le_bytes())
        }
        Instruction::Slt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs1, *rs2, 0b0000000).to_le_bytes())
        }
        Instruction::Sgt { rd, rs1, rs2 } => {
            out.extend_from_slice(&encode_r_type(0x33, *rd, 0b010, *rs2, *rs1, 0b0000000).to_le_bytes())
        }
        Instruction::Xori { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Seqz { rd, rs } => out.extend_from_slice(&encode_i_type(0x13, *rd, 0b011, *rs, 1)?.to_le_bytes()),
        Instruction::Snez { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b011, 0, *rs, 0b0000000).to_le_bytes()),
        Instruction::Neg { rd, rs } => out.extend_from_slice(&encode_r_type(0x33, *rd, 0b000, 0, *rs, 0b0100000).to_le_bytes()),
        Instruction::Ld { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b011, *rs1, *imm)?.to_le_bytes()),
        Instruction::Lbu { rd, rs1, imm } => out.extend_from_slice(&encode_i_type(0x03, *rd, 0b100, *rs1, *imm)?.to_le_bytes()),
        Instruction::Sd { rs2, rs1, imm } => out.extend_from_slice(&encode_s_type(0x23, 0b011, *rs1, *rs2, *imm)?.to_le_bytes()),
        Instruction::Slli { rd, rs1, shamt } => {
            if !(0..=63).contains(shamt) {
                return Err(CompileError::new("slli shift amount must be in 0..=63", crate::error::Span::default()));
            }
            out.extend_from_slice(&encode_i_type(0x13, *rd, 0b001, *rs1, *shamt)?.to_le_bytes());
        }
        Instruction::Li { rd, imm } => encode_li_sequence(out, *rd, *imm)?,
        Instruction::La { rd, label } => encode_address_sequence(out, *rd, pc, parsed.symbol_address(label, layout)?)?,
        Instruction::Call { label } => encode_call_sequence(out, pc, parsed.symbol_address(label, layout)?)?,
        Instruction::Jump { label } => {
            let target = parsed.symbol_address(label, layout)?;
            out.extend_from_slice(&encode_j_type(0x6f, 0, relative_offset(pc, target)?)?.to_le_bytes());
        }
        Instruction::Beqz { rs, label } => {
            let target = parsed.symbol_address(label, layout)?;
            out.extend_from_slice(&encode_b_type(0x63, 0b000, *rs, 0, relative_offset(pc, target)?)?.to_le_bytes());
        }
        Instruction::Ret => out.extend_from_slice(&encode_i_type(0x67, 0, 0b000, 1, 0)?.to_le_bytes()),
        Instruction::Ecall => out.extend_from_slice(&encode_ecall().to_le_bytes()),
    }
    Ok(())
}

fn encode_li_sequence(out: &mut Vec<u8>, rd: u8, imm: i64) -> Result<()> {
    let (hi, lo) = split_hi_lo(imm)?;
    out.extend_from_slice(&encode_u_type(0x37, rd, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
    Ok(())
}

fn encode_address_sequence(out: &mut Vec<u8>, rd: u8, pc: u64, target: u64) -> Result<()> {
    let (hi, lo) = split_hi_lo(relative_offset(pc, target)?)?;
    out.extend_from_slice(&encode_u_type(0x17, rd, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x13, rd, 0b000, rd, lo)?.to_le_bytes());
    Ok(())
}

fn encode_call_sequence(out: &mut Vec<u8>, pc: u64, target: u64) -> Result<()> {
    let (hi, lo) = split_hi_lo(relative_offset(pc, target)?)?;
    out.extend_from_slice(&encode_u_type(0x17, 1, hi).to_le_bytes());
    out.extend_from_slice(&encode_i_type(0x67, 1, 0b000, 1, lo)?.to_le_bytes());
    Ok(())
}

fn op_size(op: &AsmOp, current_offset: usize) -> usize {
    match op {
        AsmOp::Label => 0,
        AsmOp::Instruction(Instruction::Li { .. }) => 8,
        AsmOp::Instruction(Instruction::La { .. }) => 8,
        AsmOp::Instruction(Instruction::Call { .. }) => 8,
        AsmOp::Instruction(_) => 4,
        AsmOp::Word(_) => 4,
        AsmOp::Byte(_) => 1,
        AsmOp::Ascii(bytes) => bytes.len(),
        AsmOp::Align(bytes) => padding_for(current_offset, *bytes),
    }
}

fn write_elf_header(out: &mut [u8], entry: u64) -> Result<()> {
    if out.len() != ELF_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(b"\x7fELF");
    out[4] = 2;
    out[5] = 1;
    out[6] = 1;
    out[16..18].copy_from_slice(&2u16.to_le_bytes());
    out[18..20].copy_from_slice(&243u16.to_le_bytes());
    out[20..24].copy_from_slice(&1u32.to_le_bytes());
    out[24..32].copy_from_slice(&entry.to_le_bytes());
    out[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    out[40..48].copy_from_slice(&0u64.to_le_bytes());
    out[48..52].copy_from_slice(&0u32.to_le_bytes());
    out[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
    out[54..56].copy_from_slice(&(ELF_PROGRAM_HEADER_SIZE as u16).to_le_bytes());
    out[56..58].copy_from_slice(&1u16.to_le_bytes());
    Ok(())
}

fn write_program_header(out: &mut [u8], offset: u64, vaddr: u64, size: u64) -> Result<()> {
    if out.len() != ELF_PROGRAM_HEADER_SIZE {
        return Err(CompileError::new("invalid ELF program header buffer size", crate::error::Span::default()));
    }
    out.fill(0);
    out[0..4].copy_from_slice(&1u32.to_le_bytes());
    out[4..8].copy_from_slice(&5u32.to_le_bytes());
    out[8..16].copy_from_slice(&offset.to_le_bytes());
    out[16..24].copy_from_slice(&vaddr.to_le_bytes());
    out[24..32].copy_from_slice(&vaddr.to_le_bytes());
    out[32..40].copy_from_slice(&size.to_le_bytes());
    out[40..48].copy_from_slice(&size.to_le_bytes());
    out[48..56].copy_from_slice(&(ELF_SEGMENT_ALIGN as u64).to_le_bytes());
    Ok(())
}

fn strip_comment(line: &str) -> Option<&str> {
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        match ch {
            '"' if !escape => in_string = !in_string,
            '#' if !in_string => return Some(line[..idx].trim()),
            '\\' if in_string => {
                escape = !escape;
                continue;
            }
            _ => {}
        }
        escape = false;
    }
    let trimmed = line.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn parse_ascii_literal(value: &str) -> Result<Vec<u8>> {
    let Some(inner) = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return Err(CompileError::new(format!("invalid .ascii literal '{}'", value), crate::error::Span::default()));
    };

    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.extend_from_slice(ch.to_string().as_bytes());
            continue;
        }

        let escaped = chars
            .next()
            .ok_or_else(|| CompileError::new("unterminated escape sequence in .ascii literal", crate::error::Span::default()))?;
        match escaped {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            '\\' => out.push(b'\\'),
            '"' => out.push(b'"'),
            'x' => {
                let hi = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let lo = chars
                    .next()
                    .ok_or_else(|| CompileError::new("incomplete hex escape in .ascii literal", crate::error::Span::default()))?;
                let hex = format!("{}{}", hi, lo);
                let byte = u8::from_str_radix(&hex, 16)
                    .map_err(|_| CompileError::new(format!("invalid hex escape '\\x{}'", hex), crate::error::Span::default()))?;
                out.push(byte);
            }
            other => {
                return Err(CompileError::new(
                    format!("unsupported escape sequence '\\{}' in .ascii literal", other),
                    crate::error::Span::default(),
                ))
            }
        }
    }

    Ok(out)
}

fn parse_memory_operand(value: &str) -> Result<(i64, u8)> {
    let open = value
        .find('(')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let close = value
        .rfind(')')
        .ok_or_else(|| CompileError::new(format!("invalid memory operand '{}'", value), crate::error::Span::default()))?;
    let imm = parse_immediate(value[..open].trim())?;
    let rs1 = parse_register(value[open + 1..close].trim())?;
    Ok((imm, rs1))
}

fn parse_register(name: &str) -> Result<u8> {
    let reg = match name {
        "zero" | "x0" => 0,
        "ra" | "x1" => 1,
        "sp" | "x2" => 2,
        "gp" | "x3" => 3,
        "tp" | "x4" => 4,
        "t0" | "x5" => 5,
        "t1" | "x6" => 6,
        "t2" | "x7" => 7,
        "s0" | "fp" | "x8" => 8,
        "s1" | "x9" => 9,
        "a0" | "x10" => 10,
        "a1" | "x11" => 11,
        "a2" | "x12" => 12,
        "a3" | "x13" => 13,
        "a4" | "x14" => 14,
        "a5" | "x15" => 15,
        "a6" | "x16" => 16,
        "a7" | "x17" => 17,
        "s2" | "x18" => 18,
        "s3" | "x19" => 19,
        "s4" | "x20" => 20,
        "s5" | "x21" => 21,
        "s6" | "x22" => 22,
        "s7" | "x23" => 23,
        "s8" | "x24" => 24,
        "s9" | "x25" => 25,
        "s10" | "x26" => 26,
        "s11" | "x27" => 27,
        "t3" | "x28" => 28,
        "t4" | "x29" => 29,
        "t5" | "x30" => 30,
        "t6" | "x31" => 31,
        other => return Err(CompileError::new(format!("unknown register '{}'", other), crate::error::Span::default())),
    };
    Ok(reg)
}

fn parse_immediate(value: &str) -> Result<i64> {
    if let Some(hex) = value.strip_prefix("-0x") {
        return i64::from_str_radix(hex, 16)
            .map(|value| -value)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    if let Some(hex) = value.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16)
            .map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()));
    }
    value.parse::<i64>().map_err(|_| CompileError::new(format!("invalid immediate '{}'", value), crate::error::Span::default()))
}

fn arg<'a>(args: &'a [String], index: usize) -> Result<&'a str> {
    args.get(index)
        .map(|value| value.as_str())
        .ok_or_else(|| CompileError::new("malformed assembly instruction", crate::error::Span::default()))
}

fn encode_r_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, rs2: u8, funct7: u32) -> u32 {
    (funct7 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode
}

fn encode_i_type(opcode: u32, rd: u8, funct3: u32, rs1: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    Ok((imm << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | ((rd as u32) << 7) | opcode)
}

fn encode_s_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    let imm = encode_signed_bits(imm, 12)?;
    let imm_lo = imm & 0x1f;
    let imm_hi = (imm >> 5) & 0x7f;
    Ok((imm_hi << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) | (funct3 << 12) | (imm_lo << 7) | opcode)
}

fn encode_b_type(opcode: u32, funct3: u32, rs1: u8, rs2: u8, imm: i64) -> Result<u32> {
    if imm % 2 != 0 {
        return Err(CompileError::new("branch target is not 2-byte aligned", crate::error::Span::default()));
    }
    let imm = encode_signed_bits(imm, 13)?;
    let bit12 = (imm >> 12) & 0x1;
    let bits10_5 = (imm >> 5) & 0x3f;
    let bits4_1 = (imm >> 1) & 0xf;
    let bit11 = (imm >> 11) & 0x1;
    Ok((bit12 << 31)
        | (bits10_5 << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | (bits4_1 << 8)
        | (bit11 << 7)
        | opcode)
}

fn encode_u_type(opcode: u32, rd: u8, imm: i64) -> u32 {
    (((imm as i32 as u32) & 0x000f_ffff) << 12) | ((rd as u32) << 7) | opcode
}

fn encode_j_type(opcode: u32, rd: u8, imm: i64) -> Result<u32> {
    if imm % 2 != 0 {
        return Err(CompileError::new("jump target is not 2-byte aligned", crate::error::Span::default()));
    }
    let imm = encode_signed_bits(imm, 21)?;
    let bit20 = (imm >> 20) & 0x1;
    let bits10_1 = (imm >> 1) & 0x3ff;
    let bit11 = (imm >> 11) & 0x1;
    let bits19_12 = (imm >> 12) & 0xff;
    Ok((bit20 << 31) | (bits10_1 << 21) | (bit11 << 20) | (bits19_12 << 12) | ((rd as u32) << 7) | opcode)
}

fn encode_ecall() -> u32 {
    0x0000_0073
}

fn encode_signed_bits(value: i64, bits: u32) -> Result<u32> {
    let min = -(1i64 << (bits - 1));
    let max = (1i64 << (bits - 1)) - 1;
    if value < min || value > max {
        return Err(CompileError::new(
            format!("immediate '{}' does not fit {}-bit signed field", value, bits),
            crate::error::Span::default(),
        ));
    }
    Ok((value as i32 as u32) & ((1u32 << bits) - 1))
}

fn split_hi_lo(value: i64) -> Result<(i64, i64)> {
    if !(i32::MIN as i64..=i32::MAX as i64).contains(&value) {
        return Err(CompileError::new(
            format!("value '{}' is outside the supported 32-bit immediate range", value),
            crate::error::Span::default(),
        ));
    }
    let hi = (value + 0x800) >> 12;
    let lo = value - (hi << 12);
    if !(-2048..=2047).contains(&lo) {
        return Err(CompileError::new(format!("low immediate '{}' is out of range after split", lo), crate::error::Span::default()));
    }
    Ok((hi, lo))
}

fn relative_offset(pc: u64, target: u64) -> Result<i64> {
    i64::try_from(target as i128 - pc as i128)
        .map_err(|_| CompileError::new("relative offset overflowed i64", crate::error::Span::default()))
}

fn align_up(value: usize, align: usize) -> usize {
    if align <= 1 {
        return value;
    }
    (value + align - 1) & !(align - 1)
}

fn align_frame(value: usize) -> usize {
    align_up(value.max(16), 16)
}

fn ckb_source_name(source: u64) -> &'static str {
    match source {
        CKB_SOURCE_INPUT => "Input",
        CKB_SOURCE_OUTPUT => "Output",
        CKB_SOURCE_CELL_DEP => "CellDep",
        _ => "Unknown",
    }
}

fn padding_for(offset: usize, align: usize) -> usize {
    align_up(offset, align) - offset
}

fn pad_to_alignment(out: &mut Vec<u8>, align: usize) {
    let pad = padding_for(out.len(), align);
    out.resize(out.len() + pad, 0);
}
