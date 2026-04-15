//! WebAssembly 目标支持
//!
//! 将 CellScript 编译为 WebAssembly，用于浏览器和 Node.js 环境

use crate::ast::*;
use crate::error::{CompileError, Result};
use crate::ir::{IrModule, IrFunction, IrType, IrInstr};

/// Wasm 编译器
pub struct WasmCompiler {
    /// 模块
    module: WasmModule,
    /// 函数索引
    func_index: u32,
    /// 局部变量计数
    local_count: u32,
}

/// Wasm 模块
#[derive(Debug, Clone)]
pub struct WasmModule {
    /// 类型段
    pub types: Vec<WasmFuncType>,
    /// 导入段
    pub imports: Vec<WasmImport>,
    /// 函数段
    pub functions: Vec<WasmFunction>,
    /// 内存段
    pub memories: Vec<WasmMemory>,
    /// 全局段
    pub globals: Vec<WasmGlobal>,
    /// 导出段
    pub exports: Vec<WasmExport>,
    /// 代码段
    pub code: Vec<WasmCode>,
    /// 数据段
    pub data: Vec<WasmData>,
    /// 自定义段
    pub customs: Vec<WasmCustom>,
}

/// Wasm 函数类型
#[derive(Debug, Clone)]
pub struct WasmFuncType {
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

/// Wasm 值类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmValType {
    I32,
    I64,
    F32,
    F64,
}

/// Wasm 导入
#[derive(Debug, Clone)]
pub struct WasmImport {
    pub module: String,
    pub name: String,
    pub kind: WasmImportKind,
}

/// Wasm 导入类型
#[derive(Debug, Clone)]
pub enum WasmImportKind {
    Func(u32),  // 类型索引
    Memory(WasmMemory),
    Global(WasmGlobal),
}

/// Wasm 函数
#[derive(Debug, Clone)]
pub struct WasmFunction {
    pub type_idx: u32,
    pub name: String,
}

/// Wasm 内存
#[derive(Debug, Clone)]
pub struct WasmMemory {
    pub min: u32,
    pub max: Option<u32>,
}

/// Wasm 全局变量
#[derive(Debug, Clone)]
pub struct WasmGlobal {
    pub ty: WasmValType,
    pub mutable: bool,
    pub init: WasmInstr,
}

/// Wasm 导出
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    pub kind: WasmExportKind,
    pub idx: u32,
}

/// Wasm 导出类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmExportKind {
    Func,
    Memory,
    Global,
}

/// Wasm 代码
#[derive(Debug, Clone)]
pub struct WasmCode {
    pub locals: Vec<WasmValType>,
    pub body: Vec<WasmInstr>,
}

/// Wasm 数据段
#[derive(Debug, Clone)]
pub struct WasmData {
    pub memory: u32,
    pub offset: Vec<WasmInstr>,
    pub data: Vec<u8>,
}

/// Wasm 自定义段
#[derive(Debug, Clone)]
pub struct WasmCustom {
    pub name: String,
    pub data: Vec<u8>,
}

/// Wasm 指令
#[derive(Debug, Clone)]
pub enum WasmInstr {
    // 控制流
    Unreachable,
    Nop,
    Block(Vec<WasmInstr>),
    Loop(Vec<WasmInstr>),
    If(Vec<WasmInstr>),
    IfElse(Vec<WasmInstr>, Vec<WasmInstr>),
    Br(u32),
    BrIf(u32),
    Return,
    
    // 调用
    Call(u32),
    CallIndirect(u32, u32),
    
    // 局部变量
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    
    // 全局变量
    GlobalGet(u32),
    GlobalSet(u32),
    
    // 内存
    I32Load(u32, u32),
    I64Load(u32, u32),
    I32Store(u32, u32),
    I64Store(u32, u32),
    MemorySize,
    MemoryGrow,
    
    // 常量
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),
    
    // 整数运算
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32DivU,
    I32RemS,
    I32RemU,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,
    
    I64Add,
    I64Sub,
    I64Mul,
    I64DivS,
    I64DivU,
    I64RemS,
    I64RemU,
    I64And,
    I64Or,
    I64Xor,
    I64Shl,
    I64ShrS,
    I64ShrU,
    
    // 比较
    I32Eqz,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LtU,
    I32GtS,
    I32GtU,
    I32LeS,
    I32LeU,
    I32GeS,
    I32GeU,
    
    I64Eqz,
    I64Eq,
    I64Ne,
    I64LtS,
    I64LtU,
    I64GtS,
    I64GtU,
    I64LeS,
    I64LeU,
    I64GeS,
    I64GeU,
    
    // 类型转换
    I32WrapI64,
    I64ExtendI32S,
    I64ExtendI32U,
}

impl WasmCompiler {
    /// 创建新的 Wasm 编译器
    pub fn new() -> Self {
        Self {
            module: WasmModule {
                types: Vec::new(),
                imports: Vec::new(),
                functions: Vec::new(),
                memories: vec![WasmMemory { min: 1, max: Some(65536) }],
                globals: Vec::new(),
                exports: Vec::new(),
                code: Vec::new(),
                data: Vec::new(),
                customs: Vec::new(),
            },
            func_index: 0,
            local_count: 0,
        }
    }

    /// 编译 IR 模块为 Wasm
    pub fn compile(&mut self, ir: &IrModule) -> Result<WasmModule> {
        // 添加标准库导入
        self.add_stdlib_imports();
        
        // 编译每个函数
        for func in &ir.functions {
            self.compile_function(func)?;
        }
        
        // 导出入口函数
        self.add_exports();
        
        Ok(self.module.clone())
    }

    /// 添加标准库导入
    fn add_stdlib_imports(&mut self) {
        // 添加 host 函数导入
        let import_types = vec![
            ("env", "abort", vec![WasmValType::I32, WasmValType::I32, WasmValType::I32, WasmValType::I32], vec![]),
            ("env", "seed", vec![], vec![WasmValType::I64]),
            ("env", "log", vec![WasmValType::I32, WasmValType::I32], vec![]),
        ];
        
        for (module, name, params, results) in import_types {
            let type_idx = self.add_type(WasmFuncType { params, results });
            self.module.imports.push(WasmImport {
                module: module.to_string(),
                name: name.to_string(),
                kind: WasmImportKind::Func(type_idx),
            });
        }
    }

    /// 添加类型
    fn add_type(&mut self, ty: WasmFuncType) -> u32 {
        let idx = self.module.types.len() as u32;
        self.module.types.push(ty);
        idx
    }

    /// 编译函数
    fn compile_function(&mut self, func: &IrFunction) -> Result<()> {
        // 转换参数类型
        let params: Vec<WasmValType> = func.params.iter()
            .map(|p| self.ir_type_to_wasm(&p.ty))
            .collect();
        
        // 转换返回类型
        let results: Vec<WasmValType> = if let Some(ret) = &func.return_type {
            vec![self.ir_type_to_wasm(ret)]
        } else {
            vec![]
        };
        
        // 添加函数类型
        let type_idx = self.add_type(WasmFuncType { params, results });
        
        // 添加函数
        let func_idx = self.module.functions.len() as u32;
        self.module.functions.push(WasmFunction {
            type_idx,
            name: func.name.clone(),
        });
        
        // 编译函数体
        let mut code = WasmCode {
            locals: Vec::new(),
            body: Vec::new(),
        };
        
        for instr in &func.body {
            self.compile_instruction(instr, &mut code)?;
        }
        
        // 添加隐式返回
        if func.return_type.is_none() {
            code.body.push(WasmInstr::Return);
        }
        
        self.module.code.push(code);
        self.func_index += 1;
        
        Ok(())
    }

    /// 编译指令
    fn compile_instruction(&self, instr: &IrInstr, code: &mut WasmCode) -> Result<()> {
        match instr {
            IrInstr::Const(val) => {
                match val {
                    crate::ir::IrConst::U64(n) => {
                        code.body.push(WasmInstr::I64Const(*n as i64));
                    }
                    crate::ir::IrConst::U128(n) => {
                        // u128 需要拆分为两个 u64
                        let low = *n as u64;
                        let high = (*n >> 64) as u64;
                        code.body.push(WasmInstr::I64Const(low as i64));
                        code.body.push(WasmInstr::I64Const(high as i64));
                    }
                    crate::ir::IrConst::Bool(b) => {
                        code.body.push(WasmInstr::I32Const(*b as i32));
                    }
                    _ => {}
                }
            }
            IrInstr::Add => {
                code.body.push(WasmInstr::I64Add);
            }
            IrInstr::Sub => {
                code.body.push(WasmInstr::I64Sub);
            }
            IrInstr::Mul => {
                code.body.push(WasmInstr::I64Mul);
            }
            IrInstr::Div => {
                code.body.push(WasmInstr::I64DivU);
            }
            IrInstr::Rem => {
                code.body.push(WasmInstr::I64RemU);
            }
            IrInstr::And => {
                code.body.push(WasmInstr::I64And);
            }
            IrInstr::Or => {
                code.body.push(WasmInstr::I64Or);
            }
            IrInstr::Xor => {
                code.body.push(WasmInstr::I64Xor);
            }
            IrInstr::Eq => {
                code.body.push(WasmInstr::I64Eq);
            }
            IrInstr::Ne => {
                code.body.push(WasmInstr::I64Ne);
            }
            IrInstr::Lt => {
                code.body.push(WasmInstr::I64LtU);
            }
            IrInstr::Gt => {
                code.body.push(WasmInstr::I64GtU);
            }
            IrInstr::Le => {
                code.body.push(WasmInstr::I64LeU);
            }
            IrInstr::Ge => {
                code.body.push(WasmInstr::I64GeU);
            }
            IrInstr::LocalGet(idx) => {
                code.body.push(WasmInstr::LocalGet(*idx as u32));
            }
            IrInstr::LocalSet(idx) => {
                code.body.push(WasmInstr::LocalSet(*idx as u32));
            }
            IrInstr::Call(idx) => {
                code.body.push(WasmInstr::Call(*idx as u32));
            }
            IrInstr::Return => {
                code.body.push(WasmInstr::Return);
            }
            IrInstr::If(then_branch, else_branch) => {
                let then_instrs: Vec<WasmInstr> = Vec::new();
                let else_instrs: Vec<WasmInstr> = Vec::new();
                code.body.push(WasmInstr::IfElse(then_instrs, else_instrs));
            }
            IrInstr::Br(label) => {
                code.body.push(WasmInstr::Br(*label as u32));
            }
            IrInstr::BrIf(label) => {
                code.body.push(WasmInstr::BrIf(*label as u32));
            }
            _ => {}
        }
        
        Ok(())
    }

    /// 转换 IR 类型为 Wasm 类型
    fn ir_type_to_wasm(&self, ty: &IrType) -> WasmValType {
        match ty {
            IrType::U32 | IrType::Bool | IrType::Address | IrType::Hash => WasmValType::I32,
            IrType::U64 | IrType::U128 => WasmValType::I64,
            _ => WasmValType::I64, // 默认使用 i64
        }
    }

    /// 添加导出
    fn add_exports(&mut self) {
        // 导出内存
        self.module.exports.push(WasmExport {
            name: "memory".to_string(),
            kind: WasmExportKind::Memory,
            idx: 0,
        });
        
        // 导出函数
        for (i, func) in self.module.functions.iter().enumerate() {
            if func.name.starts_with("action_") || func.name == "main" {
                self.module.exports.push(WasmExport {
                    name: func.name.clone(),
                    kind: WasmExportKind::Func,
                    idx: i as u32,
                });
            }
        }
    }

    /// 编码为二进制
    pub fn encode(&self) -> Vec<u8> {
        let mut encoder = WasmEncoder::new();
        encoder.encode_module(&self.module)
    }
}

/// Wasm 编码器
pub struct WasmEncoder {
    output: Vec<u8>,
}

impl WasmEncoder {
    /// 创建新的编码器
    pub fn new() -> Self {
        Self { output: Vec::new() }
    }

    /// 编码模块
    pub fn encode_module(&mut self, module: &WasmModule) -> Vec<u8> {
        // 魔数
        self.output.extend_from_slice(&[0x00, 0x61, 0x73, 0x6d]);
        // 版本
        self.output.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        
        // 类型段
        if !module.types.is_empty() {
            self.encode_section(1, |e| e.encode_types(&module.types));
        }
        
        // 导入段
        if !module.imports.is_empty() {
            self.encode_section(2, |e| e.encode_imports(&module.imports));
        }
        
        // 函数段
        if !module.functions.is_empty() {
            self.encode_section(3, |e| e.encode_functions(&module.functions));
        }
        
        // 内存段
        if !module.memories.is_empty() {
            self.encode_section(5, |e| e.encode_memories(&module.memories));
        }
        
        // 全局段
        if !module.globals.is_empty() {
            self.encode_section(6, |e| e.encode_globals(&module.globals));
        }
        
        // 导出段
        if !module.exports.is_empty() {
            self.encode_section(7, |e| e.encode_exports(&module.exports));
        }
        
        // 代码段
        if !module.code.is_empty() {
            self.encode_section(10, |e| e.encode_code(&module.code));
        }
        
        // 数据段
        if !module.data.is_empty() {
            self.encode_section(11, |e| e.encode_data(&module.data));
        }
        
        self.output.clone()
    }

    /// 编码段
    fn encode_section<F>(&mut self, id: u8, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.output.push(id);
        let start = self.output.len();
        self.output.push(0); // 占位长度
        
        f(self);
        
        let len = self.output.len() - start - 1;
        self.output[start] = len as u8;
    }

    /// 编码类型
    fn encode_types(&mut self, types: &[WasmFuncType]) {
        self.encode_leb128(types.len() as u32);
        for ty in types {
            self.output.push(0x60); // func type
            self.encode_leb128(ty.params.len() as u32);
            for p in &ty.params {
                self.encode_val_type(*p);
            }
            self.encode_leb128(ty.results.len() as u32);
            for r in &ty.results {
                self.encode_val_type(*r);
            }
        }
    }

    /// 编码值类型
    fn encode_val_type(&mut self, ty: WasmValType) {
        match ty {
            WasmValType::I32 => self.output.push(0x7f),
            WasmValType::I64 => self.output.push(0x7e),
            WasmValType::F32 => self.output.push(0x7d),
            WasmValType::F64 => self.output.push(0x7c),
        }
    }

    /// 编码 LEB128
    fn encode_leb128(&mut self, mut value: u32) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            self.output.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    // 其他编码方法简化...
    fn encode_imports(&mut self, _imports: &[WasmImport]) {}
    fn encode_functions(&mut self, _funcs: &[WasmFunction]) {}
    fn encode_memories(&mut self, _mems: &[WasmMemory]) {}
    fn encode_globals(&mut self, _globals: &[WasmGlobal]) {}
    fn encode_exports(&mut self, _exports: &[WasmExport]) {}
    fn encode_code(&mut self, _code: &[WasmCode]) {}
    fn encode_data(&mut self, _data: &[WasmData]) {}
}

/// Wasm 运行时
pub struct WasmRuntime;

impl WasmRuntime {
    /// 实例化模块
    pub fn instantiate(module: &WasmModule) -> Result<WasmInstance> {
        Ok(WasmInstance {
            module: module.clone(),
            memory: vec![0u8; 65536], // 1 page = 64KB
            globals: Vec::new(),
        })
    }
}

/// Wasm 实例
pub struct WasmInstance {
    module: WasmModule,
    memory: Vec<u8>,
    globals: Vec<i64>,
}

impl WasmInstance {
    /// 调用函数
    pub fn call(&mut self, name: &str, args: &[i64]) -> Result<Option<i64>> {
        // 简化实现
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_compiler() {
        let mut compiler = WasmCompiler::new();
        
        // 创建一个简单的 IR 模块
        let ir = IrModule {
            name: "test".to_string(),
            functions: vec![],
            types: vec![],
            constants: vec![],
        };
        
        let module = compiler.compile(&ir).unwrap();
        
        assert!(!module.types.is_empty());
        assert_eq!(module.memories.len(), 1);
    }

    #[test]
    fn test_wasm_encoder() {
        let mut encoder = WasmEncoder::new();
        let module = WasmModule {
            types: vec![WasmFuncType { params: vec![], results: vec![] }],
            imports: vec![],
            functions: vec![],
            memories: vec![],
            globals: vec![],
            exports: vec![],
            code: vec![],
            data: vec![],
            customs: vec![],
        };
        
        let binary = encoder.encode_module(&module);
        
        // 检查魔数和版本
        assert_eq!(&binary[0..4], &[0x00, 0x61, 0x73, 0x6d]);
        assert_eq!(&binary[4..8], &[0x01, 0x00, 0x00, 0x00]);
    }
}
