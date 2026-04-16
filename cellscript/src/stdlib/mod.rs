//! CellScript 标准库
//!
//! 提供 CKB syscall 包装器、数学函数、哈希函数和环境函数。
//!
//! VM 内对象 ABI 使用 Molecule；Borsh 仅用于 Rust-side scheduler witness
//! metadata，不作为 CellScript VM 标准库函数暴露。

pub mod collections;

use crate::ir::IrType;

/// 标准库模块
pub struct StdLib;

impl StdLib {
    /// 获取标准库函数列表
    pub fn functions() -> Vec<StdFunction> {
        vec![
            // ckbvm 系统调用包装器
            StdFunction { name: "syscall_load_tx_hash".to_string(), params: vec![], return_type: Some(IrType::Hash) },
            StdFunction { name: "syscall_load_script_hash".to_string(), params: vec![], return_type: Some(IrType::Hash) },
            StdFunction {
                name: "syscall_load_cell".to_string(),
                params: vec![
                    ("index".to_string(), IrType::U64),
                    ("source".to_string(), IrType::U64),
                    ("field".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "syscall_load_input".to_string(),
                params: vec![
                    ("index".to_string(), IrType::U64),
                    ("source".to_string(), IrType::U64),
                    ("field".to_string(), IrType::U64),
                ],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "syscall_load_witness".to_string(),
                params: vec![("index".to_string(), IrType::U64), ("source".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction { name: "syscall_current_cycles".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction {
                name: "syscall_debug_print".to_string(),
                params: vec![("msg".to_string(), IrType::Array(Box::new(IrType::U8), 0))],
                return_type: None,
            },
            // 数学函数
            StdFunction {
                name: "math_min".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_max".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_isqrt".to_string(),
                params: vec![("n".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            StdFunction {
                name: "math_abs_diff".to_string(),
                params: vec![("a".to_string(), IrType::U64), ("b".to_string(), IrType::U64)],
                return_type: Some(IrType::U64),
            },
            // 哈希函数
            StdFunction {
                name: "hash_blake3".to_string(),
                params: vec![("data".to_string(), IrType::Array(Box::new(IrType::U8), 0))],
                return_type: Some(IrType::Hash),
            },
            // 环境函数
            StdFunction { name: "env_current_daa_score".to_string(), params: vec![], return_type: Some(IrType::U64) },
            StdFunction { name: "env_remaining_cycles".to_string(), params: vec![], return_type: Some(IrType::U64) },
        ]
    }

    /// 检查是否为标准库函数
    pub fn is_std_function(name: &str) -> bool {
        Self::functions().iter().any(|f| f.name == name)
    }

    /// 获取标准库函数
    pub fn get_function(name: &str) -> Option<StdFunction> {
        Self::functions().into_iter().find(|f| f.name == name)
    }

    /// 生成标准库 RISC-V 汇编代码
    pub fn generate_assembly() -> String {
        let mut asm = String::new();

        asm.push_str("# CellScript Standard Library\n\n");
        asm.push_str(".section .text\n\n");

        // 系统调用包装器
        asm.push_str(&Self::generate_syscalls());

        // 数学函数
        asm.push_str(&Self::generate_math());

        // 哈希函数
        asm.push_str(&Self::generate_hash());

        // 环境函数
        asm.push_str(&Self::generate_env());

        asm
    }

    /// 生成系统调用包装器
    fn generate_syscalls() -> String {
        let mut asm = String::new();

        // syscall_load_tx_hash (2061)
        asm.push_str("# Syscall: load_tx_hash (2061)\n");
        asm.push_str(".global __syscall_load_tx_hash\n");
        asm.push_str("__syscall_load_tx_hash:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2061\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_load_script_hash (2062)
        asm.push_str("# Syscall: load_script_hash (2062)\n");
        asm.push_str(".global __syscall_load_script_hash\n");
        asm.push_str("__syscall_load_script_hash:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2062\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_load_cell (2071)
        asm.push_str("# Syscall: load_cell (2071)\n");
        asm.push_str(".global __syscall_load_cell\n");
        asm.push_str("__syscall_load_cell:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2071\n");
        asm.push_str("    # a0 = index, a1 = source, a2 = field\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_load_input (2073)
        asm.push_str("# Syscall: load_input (2073)\n");
        asm.push_str(".global __syscall_load_input\n");
        asm.push_str("__syscall_load_input:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2073\n");
        asm.push_str("    # a0 = index, a1 = source, a2 = field\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_load_witness (2074)
        asm.push_str("# Syscall: load_witness (2074)\n");
        asm.push_str(".global __syscall_load_witness\n");
        asm.push_str("__syscall_load_witness:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2074\n");
        asm.push_str("    # a0 = index, a1 = source\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_current_cycles (2042)
        asm.push_str("# Syscall: current_cycles (2042)\n");
        asm.push_str(".global __syscall_current_cycles\n");
        asm.push_str("__syscall_current_cycles:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2042\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // syscall_debug_print (2177)
        asm.push_str("# Syscall: debug_print (2177)\n");
        asm.push_str(".global __syscall_debug_print\n");
        asm.push_str("__syscall_debug_print:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2177\n");
        asm.push_str("    # a0 = msg pointer, a1 = msg length\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        asm
    }

    /// 生成数学函数
    fn generate_math() -> String {
        let mut asm = String::new();

        // math_min
        asm.push_str("# Math: min\n");
        asm.push_str(".global __math_min\n");
        asm.push_str("__math_min:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    blt a0, a1, .Lmin_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmin_ret_a:\n");
        asm.push_str("    ret\n\n");

        // math_max
        asm.push_str("# Math: max\n");
        asm.push_str(".global __math_max\n");
        asm.push_str("__math_max:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    bgt a0, a1, .Lmax_ret_a\n");
        asm.push_str("    mv a0, a1\n");
        asm.push_str(".Lmax_ret_a:\n");
        asm.push_str("    ret\n\n");

        // math_isqrt (整数平方根 - 牛顿迭代法)
        asm.push_str("# Math: isqrt (integer square root)\n");
        asm.push_str(".global __math_isqrt\n");
        asm.push_str("__math_isqrt:\n");
        asm.push_str("    addi sp, sp, -32\n");
        asm.push_str("    sd ra, 24(sp)\n");
        asm.push_str("    sd s0, 16(sp)\n");
        asm.push_str("    sd s1, 8(sp)\n");
        asm.push_str("    # a0 = n\n");
        asm.push_str("    beqz a0, .Lisqrt_ret\n");
        asm.push_str("    mv s0, a0          # x = n\n");
        asm.push_str("    srli s1, a0, 1\n");
        asm.push_str("    addi s1, s1, 1     # y = (x + 1) / 2\n");
        asm.push_str(".Lisqrt_loop:\n");
        asm.push_str("    bge s1, s0, .Lisqrt_ret\n");
        asm.push_str("    mv s0, s1          # x = y\n");
        asm.push_str("    div t0, a0, s0\n");
        asm.push_str("    add s1, s0, t0\n");
        asm.push_str("    srli s1, s1, 1     # y = (x + n/x) / 2\n");
        asm.push_str("    j .Lisqrt_loop\n");
        asm.push_str(".Lisqrt_ret:\n");
        asm.push_str("    mv a0, s0\n");
        asm.push_str("    ld ra, 24(sp)\n");
        asm.push_str("    ld s0, 16(sp)\n");
        asm.push_str("    ld s1, 8(sp)\n");
        asm.push_str("    addi sp, sp, 32\n");
        asm.push_str("    ret\n\n");

        // math_abs_diff
        asm.push_str("# Math: abs_diff\n");
        asm.push_str(".global __math_abs_diff\n");
        asm.push_str("__math_abs_diff:\n");
        asm.push_str("    # a0 = a, a1 = b\n");
        asm.push_str("    sub t0, a0, a1\n");
        asm.push_str("    bgez t0, .Labs_diff_ret\n");
        asm.push_str("    neg t0, t0\n");
        asm.push_str(".Labs_diff_ret:\n");
        asm.push_str("    mv a0, t0\n");
        asm.push_str("    ret\n\n");

        asm
    }

    /// 生成哈希函数
    fn generate_hash() -> String {
        let mut asm = String::new();

        // hash_blake3 (Spora 扩展)
        asm.push_str("# Hash: blake3 (Spora extension)\n");
        asm.push_str(".global __hash_blake3\n");
        asm.push_str("__hash_blake3:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    # a0 = data pointer, a1 = data length\n");
        asm.push_str("    # result (32 bytes) returned in buffer pointed by a0\n");
        asm.push_str("    li a7, 2100  # BLAKE3 syscall number (TBD)\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        asm
    }

    /// 生成环境函数
    fn generate_env() -> String {
        let mut asm = String::new();

        // env_current_daa_score
        asm.push_str("# Env: current_daa_score\n");
        asm.push_str(".global __env_current_daa_score\n");
        asm.push_str("__env_current_daa_score:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    # Load from header dep\n");
        asm.push_str("    li a7, 2072  # LOAD_HEADER\n");
        asm.push_str("    li a0, 0     # header index\n");
        asm.push_str("    li a1, 0     # field = DAA score\n");
        asm.push_str("    ecall\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        // env_remaining_cycles
        asm.push_str("# Env: remaining_cycles\n");
        asm.push_str(".global __env_remaining_cycles\n");
        asm.push_str("__env_remaining_cycles:\n");
        asm.push_str("    addi sp, sp, -16\n");
        asm.push_str("    sd ra, 8(sp)\n");
        asm.push_str("    li a7, 2042  # CURRENT_CYCLES\n");
        asm.push_str("    ecall\n");
        asm.push_str("    # a0 = current cycles\n");
        asm.push_str("    li t0, 10000000  # max cycles\n");
        asm.push_str("    sub a0, t0, a0   # remaining\n");
        asm.push_str("    ld ra, 8(sp)\n");
        asm.push_str("    addi sp, sp, 16\n");
        asm.push_str("    ret\n\n");

        asm
    }
}

/// 标准库函数定义
#[derive(Debug, Clone)]
pub struct StdFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: Option<IrType>,
}

/// 调度器见证元数据生成
pub struct SchedulerMetadata;

impl SchedulerMetadata {
    /// 生成调度器见证元数据
    pub fn generate(effect_class: &str, parallelizable: bool, touches_shared: Vec<[u8; 32]>, estimated_cycles: u64) -> Vec<u8> {
        use borsh::{to_vec, BorshSerialize};

        #[derive(BorshSerialize)]
        struct SchedulerWitness {
            magic: u16,  // 0xCE11
            version: u8, // 0
            effect_class: u8,
            parallelizable: bool,
            touches_shared_count: u32,
            touches_shared: Vec<[u8; 32]>,
            estimated_cycles: u64,
        }

        let effect_class_id = match effect_class {
            "Pure" => 0,
            "ReadOnly" => 1,
            "Mutating" => 2,
            "Creating" => 3,
            "Destroying" => 4,
            _ => 0,
        };

        let witness = SchedulerWitness {
            magic: 0xCE11,
            version: 0,
            effect_class: effect_class_id,
            parallelizable,
            touches_shared_count: touches_shared.len() as u32,
            touches_shared,
            estimated_cycles,
        };

        to_vec(&witness).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_std_functions() {
        let funcs = StdLib::functions();
        assert!(!funcs.is_empty());
        assert!(!StdLib::is_std_function("borsh_serialize_u64"));
        assert!(!StdLib::is_std_function("borsh_deserialize_u64"));
        assert!(StdLib::is_std_function("syscall_load_cell"));
        assert!(StdLib::is_std_function("math_isqrt"));
    }

    #[test]
    fn test_get_function() {
        let func = StdLib::get_function("math_min");
        assert!(func.is_some());
        let func = func.unwrap();
        assert_eq!(func.params.len(), 2);
    }

    #[test]
    fn test_generate_assembly() {
        let asm = StdLib::generate_assembly();
        assert!(!asm.contains("__borsh_serialize_u64"));
        assert!(!asm.contains("__borsh_deserialize_u64"));
        assert!(asm.contains("__syscall_load_cell"));
        assert!(asm.contains("__math_isqrt"));
    }
}
