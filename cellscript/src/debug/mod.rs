//! 调试信息生成
//!
//! 生成 DWARF 调试信息，支持源码级调试

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::HashMap;
use std::path::PathBuf;

/// 调试信息生成器
pub struct DebugInfoGenerator {
    /// 编译单元
    compilation_unit: CompilationUnit,
    /// 行号表
    line_table: LineNumberTable,
    /// 类型表
    type_table: TypeTable,
    /// 变量表
    variable_table: VariableTable,
    /// 当前地址
    current_address: u64,
}

/// 编译单元
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    /// 名称
    pub name: String,
    /// 源码语言
    pub language: SourceLanguage,
    /// 生产者
    pub producer: String,
    /// 源码路径
    pub source_path: PathBuf,
    /// 编译目录
    pub comp_dir: PathBuf,
}

/// 源码语言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    CellScript,
    C,
    Rust,
}

/// 行号表
#[derive(Debug, Clone, Default)]
pub struct LineNumberTable {
    /// 条目: (地址, 文件名索引, 行号, 列号)
    pub entries: Vec<(u64, u32, u32, u32)>,
    /// 文件名表
    pub file_names: Vec<PathBuf>,
}

/// 类型表
#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    /// 类型定义
    pub types: HashMap<u64, DebugType>,
    /// 下一个类型 ID
    next_id: u64,
}

/// 调试类型
#[derive(Debug, Clone)]
pub enum DebugType {
    /// 基本类型
    Base {
        id: u64,
        name: String,
        encoding: TypeEncoding,
        size: u64,
    },
    /// 指针类型
    Pointer {
        id: u64,
        pointee: u64,
        size: u64,
    },
    /// 结构体类型
    Struct {
        id: u64,
        name: String,
        size: u64,
        members: Vec<MemberInfo>,
    },
    /// 枚举类型
    Enum {
        id: u64,
        name: String,
        size: u64,
        variants: Vec<(String, i64)>,
    },
    /// 数组类型
    Array {
        id: u64,
        element_type: u64,
        count: u64,
    },
    /// 函数类型
    Function {
        id: u64,
        return_type: Option<u64>,
        params: Vec<u64>,
    },
}

/// 类型编码
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeEncoding {
    Signed,
    Unsigned,
    Float,
    Boolean,
    Address,
}

/// 成员信息
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub name: String,
    pub type_id: u64,
    pub offset: u64,
    pub size: u64,
}

/// 变量表
#[derive(Debug, Clone, Default)]
pub struct VariableTable {
    /// 变量: (名称, 类型ID, 作用域, 位置)
    pub variables: Vec<VariableInfo>,
}

/// 变量信息
#[derive(Debug, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub type_id: u64,
    pub scope: Scope,
    pub location: VariableLocation,
    pub span: Span,
}

/// 作用域
#[derive(Debug, Clone)]
pub enum Scope {
    /// 全局
    Global,
    /// 函数
    Function(String),
    /// 块
    Block(u64),
}

/// 变量位置
#[derive(Debug, Clone)]
pub enum VariableLocation {
    /// 寄存器
    Register(String),
    /// 栈偏移
    StackOffset(i64),
    /// 内存地址
    Address(u64),
}

/// DWARF 生成器
pub struct DwarfGenerator {
    /// 调试信息
    debug_info: Vec<u8>,
    /// 调试行信息
    debug_line: Vec<u8>,
    /// 调试缩写
    debug_abbrev: Vec<u8>,
    /// 调试字符串
    debug_str: Vec<u8>,
    /// 调试帧信息
    debug_frame: Vec<u8>,
}

impl DebugInfoGenerator {
    /// 创建新的调试信息生成器
    pub fn new(name: String, source_path: PathBuf) -> Self {
        Self {
            compilation_unit: CompilationUnit {
                name,
                language: SourceLanguage::CellScript,
                producer: format!("cellc {}", crate::VERSION),
                source_path,
                comp_dir: std::env::current_dir().unwrap_or_default(),
            },
            line_table: LineNumberTable::default(),
            type_table: TypeTable::default(),
            variable_table: VariableTable::default(),
            current_address: 0,
        }
    }

    /// 添加行号信息
    pub fn add_line_info(&mut self, address: u64, span: Span) {
        let file_idx = self.get_or_add_file(&self.compilation_unit.source_path);
        self.line_table.entries.push((
            address,
            file_idx,
            span.start_line as u32,
            span.start_col as u32,
        ));
    }

    /// 获取或添加文件
    fn get_or_add_file(&mut self, path: &PathBuf) -> u32 {
        if let Some(idx) = self.line_table.file_names.iter().position(|p| p == path) {
            idx as u32
        } else {
            let idx = self.line_table.file_names.len() as u32;
            self.line_table.file_names.push(path.clone());
            idx
        }
    }

    /// 注册类型
    pub fn register_type(&mut self, name: &str, ty: &Type) -> u64 {
        let id = self.type_table.next_id;
        self.type_table.next_id += 1;
        
        let debug_type = self.convert_type(id, name, ty);
        self.type_table.types.insert(id, debug_type);
        
        id
    }

    /// 转换类型
    fn convert_type(&self, id: u64, name: &str, ty: &Type) -> DebugType {
        match ty {
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128 => {
                DebugType::Base {
                    id,
                    name: name.to_string(),
                    encoding: TypeEncoding::Unsigned,
                    size: self.type_size(ty),
                }
            }
            Type::Bool => DebugType::Base {
                id,
                name: name.to_string(),
                encoding: TypeEncoding::Boolean,
                size: 1,
            },
            Type::Address | Type::Hash => DebugType::Base {
                id,
                name: name.to_string(),
                encoding: TypeEncoding::Address,
                size: 32,
            },
            Type::Array(elem, count) => {
                let elem_id = self.type_table.next_id;
                DebugType::Array {
                    id,
                    element_type: elem_id,
                    count: *count as u64,
                }
            }
            _ => DebugType::Base {
                id,
                name: name.to_string(),
                encoding: TypeEncoding::Unsigned,
                size: 8,
            }
        }
    }

    /// 计算类型大小
    fn type_size(&self, ty: &Type) -> u64 {
        match ty {
            Type::U8 => 1,
            Type::U16 => 2,
            Type::U32 => 4,
            Type::U64 => 8,
            Type::U128 => 16,
            Type::Bool => 1,
            Type::Address | Type::Hash => 32,
            Type::Array(elem, count) => self.type_size(elem) * (*count as u64),
            _ => 8,
        }
    }

    /// 注册变量
    pub fn register_variable(
        &mut self,
        name: &str,
        type_id: u64,
        scope: Scope,
        location: VariableLocation,
        span: Span,
    ) {
        self.variable_table.variables.push(VariableInfo {
            name: name.to_string(),
            type_id,
            scope,
            location,
            span,
        });
    }

    /// 生成 DWARF
    pub fn generate_dwarf(&self) -> DwarfGenerator {
        let mut dwarf = DwarfGenerator {
            debug_info: Vec::new(),
            debug_line: Vec::new(),
            debug_abbrev: Vec::new(),
            debug_str: Vec::new(),
            debug_frame: Vec::new(),
        };
        
        self.generate_debug_info(&mut dwarf);
        self.generate_debug_line(&mut dwarf);
        self.generate_debug_abbrev(&mut dwarf);
        self.generate_debug_frame(&mut dwarf);
        
        dwarf
    }

    /// 生成调试信息段
    fn generate_debug_info(&self, dwarf: &mut DwarfGenerator) {
        // DWARF 编译单元头
        dwarf.debug_info.extend_from_slice(&[0x00; 4]); // 长度占位
        dwarf.debug_info.extend_from_slice(&[0x04, 0x00]); // 版本 4
        dwarf.debug_info.push(0x08); // 地址大小
        dwarf.debug_info.push(0x00); // 缩写表偏移
        
        // 编译单元 DIE
        dwarf.debug_info.push(0x11); // TAG_compile_unit
        dwarf.debug_info.push(0x01); // 有子项
        
        // 属性: 名称
        dwarf.debug_info.push(0x03); // DW_AT_name
        self.add_string(&mut dwarf.debug_str, &self.compilation_unit.name);
        
        // 属性: 生产者
        dwarf.debug_info.push(0x25); // DW_AT_producer
        self.add_string(&mut dwarf.debug_str, &self.compilation_unit.producer);
        
        // 属性: 语言
        dwarf.debug_info.push(0x13); // DW_AT_language
        dwarf.debug_info.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // CellScript
        
        // 子 DIE: 类型
        for (_, ty) in &self.type_table.types {
            self.generate_type_die(dwarf, ty);
        }
        
        // 子 DIE: 变量
        for var in &self.variable_table.variables {
            self.generate_variable_die(dwarf, var);
        }
        
        // 结束标记
        dwarf.debug_info.push(0x00);
        
        // 回填长度
        let len = dwarf.debug_info.len() - 4;
        dwarf.debug_info[0..4].copy_from_slice(&(len as u32).to_le_bytes());
    }

    /// 生成类型 DIE
    fn generate_type_die(&self, dwarf: &mut DwarfGenerator, ty: &DebugType) {
        match ty {
            DebugType::Base { id, name, encoding, size } => {
                dwarf.debug_info.push(0x24); // TAG_base_type
                dwarf.debug_info.push(0x00); // 无子项
                
                dwarf.debug_info.push(0x03); // DW_AT_name
                self.add_string(&mut dwarf.debug_str, name);
                
                dwarf.debug_info.push(0x0b); // DW_AT_byte_size
                dwarf.debug_info.extend_from_slice(&size.to_le_bytes());
                
                dwarf.debug_info.push(0x3e); // DW_AT_encoding
                dwarf.debug_info.push(*encoding as u8);
            }
            DebugType::Struct { id, name, size, members } => {
                dwarf.debug_info.push(0x13); // TAG_structure_type
                dwarf.debug_info.push(0x01); // 有子项
                
                dwarf.debug_info.push(0x03); // DW_AT_name
                self.add_string(&mut dwarf.debug_str, name);
                
                dwarf.debug_info.push(0x0b); // DW_AT_byte_size
                dwarf.debug_info.extend_from_slice(&size.to_le_bytes());
                
                // 成员
                for member in members {
                    self.generate_member_die(dwarf, member);
                }
                
                dwarf.debug_info.push(0x00); // 结束
            }
            _ => {}
        }
    }

    /// 生成成员 DIE
    fn generate_member_die(&self, dwarf: &mut DwarfGenerator, member: &MemberInfo) {
        dwarf.debug_info.push(0x0d); // TAG_member
        dwarf.debug_info.push(0x00);
        
        dwarf.debug_info.push(0x03); // DW_AT_name
        self.add_string(&mut dwarf.debug_str, &member.name);
        
        dwarf.debug_info.push(0x38); // DW_AT_type
        dwarf.debug_info.extend_from_slice(&member.type_id.to_le_bytes());
        
        dwarf.debug_info.push(0x09); // DW_AT_data_member_location
        dwarf.debug_info.extend_from_slice(&member.offset.to_le_bytes());
    }

    /// 生成变量 DIE
    fn generate_variable_die(&self, dwarf: &mut DwarfGenerator, var: &VariableInfo) {
        dwarf.debug_info.push(0x34); // TAG_variable
        dwarf.debug_info.push(0x00);
        
        dwarf.debug_info.push(0x03); // DW_AT_name
        self.add_string(&mut dwarf.debug_str, &var.name);
        
        dwarf.debug_info.push(0x38); // DW_AT_type
        dwarf.debug_info.extend_from_slice(&var.type_id.to_le_bytes());
        
        // 位置
        dwarf.debug_info.push(0x02); // DW_AT_location
        match &var.location {
            VariableLocation::Register(reg) => {
                dwarf.debug_info.push(0x01); // DW_OP_reg
                dwarf.debug_info.push(0x00); // 寄存器号
            }
            VariableLocation::StackOffset(offset) => {
                dwarf.debug_info.push(0x91); // DW_OP_fbreg
                dwarf.debug_info.extend_from_slice(&offset.to_le_bytes());
            }
            VariableLocation::Address(addr) => {
                dwarf.debug_info.push(0x03); // DW_OP_addr
                dwarf.debug_info.extend_from_slice(&addr.to_le_bytes());
            }
        }
    }

    /// 生成行号表
    fn generate_debug_line(&self, dwarf: &mut DwarfGenerator) {
        // 行号程序头
        dwarf.debug_line.extend_from_slice(&[0x00; 4]); // 长度
        dwarf.debug_line.push(0x04); // 版本
        dwarf.debug_line.push(0x00);
        dwarf.debug_line.extend_from_slice(&[0x00; 4]); // 头长度占位
        
        // 最小指令长度
        dwarf.debug_line.push(0x01);
        // 默认 is_stmt
        dwarf.debug_line.push(0x01);
        // 行基
        dwarf.debug_line.push(0x00);
        // 行范围
        dwarf.debug_line.push(0x04);
        // 操作码基数
        dwarf.debug_line.push(0x0d);
        // 标准操作码长度
        for _ in 0..12 {
            dwarf.debug_line.push(0x01);
        }
        
        // 文件名表
        for file in &self.line_table.file_names {
            self.add_string_to_line(&mut dwarf.debug_line, &file.to_string_lossy());
            dwarf.debug_line.push(0x00); // 目录索引
            dwarf.debug_line.push(0x00); // 修改时间
            dwarf.debug_line.push(0x00); // 文件大小
        }
        dwarf.debug_line.push(0x00); // 结束标记
        
        // 行号程序
        let mut prev_addr = 0u64;
        let mut prev_line = 1u32;
        
        for (addr, file, line, col) in &self.line_table.entries {
            let addr_delta = (addr - prev_addr) as u8;
            let line_delta = (*line as i32) - (prev_line as i32);
            
            if addr_delta < 64 && line_delta >= -8 && line_delta <= 7 {
                // 特殊操作码
                let opcode = ((line_delta + 8) as u8) + (addr_delta * 14) + 13;
                dwarf.debug_line.push(opcode);
            } else {
                // 扩展操作码
                dwarf.debug_line.push(0x00); // 扩展操作码指示
                dwarf.debug_line.push(0x09); // 长度
                dwarf.debug_line.push(0x02); // DW_LNE_set_address
                dwarf.debug_line.extend_from_slice(&addr.to_le_bytes());
                
                dwarf.debug_line.push(0x00);
                dwarf.debug_line.push(0x05);
                dwarf.debug_line.push(0x01); // DW_LNE_set_line
                dwarf.debug_line.extend_from_slice(&line.to_le_bytes());
            }
            
            prev_addr = *addr;
            prev_line = *line;
        }
        
        // 结束序列
        dwarf.debug_line.push(0x00);
        dwarf.debug_line.push(0x01);
        dwarf.debug_line.push(0x01); // DW_LNE_end_sequence
    }

    /// 生成缩写表
    fn generate_debug_abbrev(&self, dwarf: &mut DwarfGenerator) {
        // 编译单元
        dwarf.debug_abbrev.push(0x11); // TAG_compile_unit
        dwarf.debug_abbrev.push(0x01); // 有子项
        dwarf.debug_abbrev.push(0x03); // DW_AT_name
        dwarf.debug_abbrev.push(0x08); // DW_FORM_string
        dwarf.debug_abbrev.push(0x25); // DW_AT_producer
        dwarf.debug_abbrev.push(0x08);
        dwarf.debug_abbrev.push(0x13); // DW_AT_language
        dwarf.debug_abbrev.push(0x06); // DW_FORM_data4
        dwarf.debug_abbrev.push(0x00);
        dwarf.debug_abbrev.push(0x00);
        
        // 基本类型
        dwarf.debug_abbrev.push(0x24); // TAG_base_type
        dwarf.debug_abbrev.push(0x00);
        dwarf.debug_abbrev.push(0x03); // DW_AT_name
        dwarf.debug_abbrev.push(0x08);
        dwarf.debug_abbrev.push(0x0b); // DW_AT_byte_size
        dwarf.debug_abbrev.push(0x0b); // DW_FORM_data1
        dwarf.debug_abbrev.push(0x3e); // DW_AT_encoding
        dwarf.debug_abbrev.push(0x0b);
        dwarf.debug_abbrev.push(0x00);
        dwarf.debug_abbrev.push(0x00);
        
        // 结束标记
        dwarf.debug_abbrev.push(0x00);
    }

    /// 生成帧信息
    fn generate_debug_frame(&self, dwarf: &mut DwarfGenerator) {
        // CIE (Common Information Entry)
        dwarf.debug_frame.extend_from_slice(&[0x00; 4]); // 长度
        dwarf.debug_frame.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]); // CIE ID
        dwarf.debug_frame.push(0x04); // 版本
        dwarf.debug_frame.push(0x00); // 增强字符串
        dwarf.debug_frame.push(0x01); // 地址大小
        dwarf.debug_frame.push(0x00); // 段大小
        
        // CFA 定义
        dwarf.debug_frame.push(0x0c); // DW_CFA_def_cfa
        dwarf.debug_frame.push(0x02); // 寄存器 2 (sp)
        dwarf.debug_frame.push(0x00); // 偏移
        
        // 返回地址
        dwarf.debug_frame.push(0x07); // DW_CFA_undefined
        dwarf.debug_frame.push(0x01); // 寄存器 1 (ra)
    }

    /// 添加字符串到字符串表
    fn add_string(&self, table: &mut Vec<u8>, s: &str) {
        table.extend_from_slice(s.as_bytes());
        table.push(0x00);
    }

    /// 添加字符串到行号表
    fn add_string_to_line(&self, table: &mut Vec<u8>, s: &str) {
        table.extend_from_slice(s.as_bytes());
        table.push(0x00);
    }
}

impl DwarfGenerator {
    /// 写入 ELF 段
    pub fn write_to_elf(&self, elf: &mut Vec<u8>, sections: &mut Vec<ElfSection>) {
        // .debug_info
        let info_offset = elf.len();
        elf.extend_from_slice(&self.debug_info);
        sections.push(ElfSection {
            name: ".debug_info".to_string(),
            offset: info_offset,
            size: self.debug_info.len(),
        });
        
        // .debug_line
        let line_offset = elf.len();
        elf.extend_from_slice(&self.debug_line);
        sections.push(ElfSection {
            name: ".debug_line".to_string(),
            offset: line_offset,
            size: self.debug_line.len(),
        });
        
        // .debug_abbrev
        let abbrev_offset = elf.len();
        elf.extend_from_slice(&self.debug_abbrev);
        sections.push(ElfSection {
            name: ".debug_abbrev".to_string(),
            offset: abbrev_offset,
            size: self.debug_abbrev.len(),
        });
        
        // .debug_str
        let str_offset = elf.len();
        elf.extend_from_slice(&self.debug_str);
        sections.push(ElfSection {
            name: ".debug_str".to_string(),
            offset: str_offset,
            size: self.debug_str.len(),
        });
        
        // .debug_frame
        let frame_offset = elf.len();
        elf.extend_from_slice(&self.debug_frame);
        sections.push(ElfSection {
            name: ".debug_frame".to_string(),
            offset: frame_offset,
            size: self.debug_frame.len(),
        });
    }
}

/// ELF 段信息
#[derive(Debug, Clone)]
pub struct ElfSection {
    pub name: String,
    pub offset: usize,
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_info_generator() {
        let gen = DebugInfoGenerator::new(
            "test".to_string(),
            PathBuf::from("test.cell"),
        );
        
        assert_eq!(gen.compilation_unit.name, "test");
        assert_eq!(gen.compilation_unit.language, SourceLanguage::CellScript);
    }

    #[test]
    fn test_line_table() {
        let mut gen = DebugInfoGenerator::new(
            "test".to_string(),
            PathBuf::from("test.cell"),
        );
        
        gen.add_line_info(0x1000, Span::default());
        gen.add_line_info(0x1004, Span::default());
        
        assert_eq!(gen.line_table.entries.len(), 2);
    }

    #[test]
    fn test_type_registration() {
        let mut gen = DebugInfoGenerator::new(
            "test".to_string(),
            PathBuf::from("test.cell"),
        );
        
        let id = gen.register_type("u64", &Type::U64);
        assert!(gen.type_table.types.contains_key(&id));
    }

    #[test]
    fn test_dwarf_generation() {
        let gen = DebugInfoGenerator::new(
            "test".to_string(),
            PathBuf::from("test.cell"),
        );
        
        let dwarf = gen.generate_dwarf();
        
        assert!(!dwarf.debug_info.is_empty());
        assert!(!dwarf.debug_abbrev.is_empty());
    }
}
