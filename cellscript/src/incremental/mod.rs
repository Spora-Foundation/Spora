//! 增量编译系统
//!
//! 通过缓存和依赖追踪加速重复编译

use crate::ast::Module;
use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 编译缓存
pub struct IncrementalCompiler {
    /// 缓存目录
    cache_dir: PathBuf,
    /// 依赖图
    dep_graph: DependencyGraph,
    /// 文件哈希缓存
    file_hashes: HashMap<PathBuf, u64>,
    /// 编译单元缓存
    unit_cache: HashMap<String, CompiledUnit>,
}

/// 编译单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledUnit {
    /// 源文件路径
    pub source_path: PathBuf,
    /// 源文件哈希
    pub source_hash: u64,
    /// 输出文件路径
    pub output_path: PathBuf,
    /// 输出文件哈希
    pub output_hash: u64,
    /// 依赖的文件
    pub dependencies: Vec<PathBuf>,
    /// 编译时间戳
    pub timestamp: SystemTime,
    /// 编译选项
    pub compile_options: CompileOptions,
}

/// 编译选项
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct CompileOptions {
    pub opt_level: u8,
    pub target: String,
    pub debug: bool,
}

/// 依赖图
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    /// 节点: 文件 -> 依赖它的文件
    pub dependents: HashMap<PathBuf, HashSet<PathBuf>>,
    /// 反向: 文件 -> 它依赖的文件
    pub dependencies: HashMap<PathBuf, HashSet<PathBuf>>,
}

/// 变更检测器
pub struct ChangeDetector {
    /// 文件系统快照
    snapshots: HashMap<PathBuf, FileSnapshot>,
}

/// 文件快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub hash: u64,
    pub mtime: SystemTime,
    pub size: u64,
}

impl IncrementalCompiler {
    /// 创建新的增量编译器
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        
        // 确保缓存目录存在
        fs::create_dir_all(&cache_dir).ok();
        
        Self {
            cache_dir,
            dep_graph: DependencyGraph::default(),
            file_hashes: HashMap::new(),
            unit_cache: HashMap::new(),
        }
    }

    /// 加载缓存状态
    pub fn load_cache(&mut self) -> Result<()> {
        let cache_file = self.cache_dir.join("compile_cache.json");
        
        if cache_file.exists() {
            let content = fs::read_to_string(&cache_file)?;
            let cache: IncrementalCache = serde_json::from_str(&content)?;
            self.dep_graph = cache.dep_graph;
            self.unit_cache = cache.units;
        }
        
        Ok(())
    }

    /// 保存缓存状态
    pub fn save_cache(&self) -> Result<()> {
        let cache_file = self.cache_dir.join("compile_cache.json");
        
        let cache = IncrementalCache {
            dep_graph: self.dep_graph.clone(),
            units: self.unit_cache.clone(),
        };
        
        let content = serde_json::to_string_pretty(&cache)?;
        fs::write(&cache_file, content)?;
        
        Ok(())
    }

    /// 检查是否需要重新编译
    pub fn needs_recompile(&self, source: &Path, options: &CompileOptions) -> bool {
        let source_str = source.to_string_lossy().to_string();
        
        // 检查是否有缓存
        let Some(unit) = self.unit_cache.get(&source_str) else {
            return true;
        };
        
        // 检查编译选项是否变化
        if unit.compile_options != *options {
            return true;
        }
        
        // 检查源文件是否变化
        let current_hash = match compute_file_hash(source) {
            Ok(h) => h,
            Err(_) => return true,
        };
        
        if unit.source_hash != current_hash {
            return true;
        }
        
        // 检查依赖是否变化
        for dep in &unit.dependencies {
            let dep_hash = match compute_file_hash(dep) {
                Ok(h) => h,
                Err(_) => return true,
            };
            
            // 查找依赖的编译单元
            let dep_str = dep.to_string_lossy().to_string();
            if let Some(dep_unit) = self.unit_cache.get(&dep_str) {
                if dep_unit.source_hash != dep_hash {
                    return true;
                }
            } else {
                return true;
            }
        }
        
        false
    }

    /// 获取受影响的文件
    pub fn get_affected_files(&self, changed_file: &Path) -> HashSet<PathBuf> {
        let mut affected = HashSet::new();
        let mut to_process = vec![changed_file.to_path_buf()];
        
        while let Some(file) = to_process.pop() {
            if let Some(dependents) = self.dep_graph.dependents.get(&file) {
                for dependent in dependents {
                    if affected.insert(dependent.clone()) {
                        to_process.push(dependent.clone());
                    }
                }
            }
        }
        
        affected
    }

    /// 记录编译单元
    pub fn record_compilation(
        &mut self,
        source: &Path,
        output: &Path,
        dependencies: Vec<PathBuf>,
        options: &CompileOptions,
    ) -> Result<()> {
        let source_hash = compute_file_hash(source)?;
        let output_hash = compute_file_hash(output).unwrap_or(0);
        
        let unit = CompiledUnit {
            source_path: source.to_path_buf(),
            source_hash,
            output_path: output.to_path_buf(),
            output_hash,
            dependencies: dependencies.clone(),
            timestamp: SystemTime::now(),
            compile_options: options.clone(),
        };
        
        // 更新依赖图
        for dep in &dependencies {
            self.dep_graph
                .dependents
                .entry(dep.clone())
                .or_default()
                .insert(source.to_path_buf());
        }
        
        self.dep_graph
            .dependencies
            .entry(source.to_path_buf())
            .or_default()
            .extend(dependencies);
        
        // 保存单元
        let source_str = source.to_string_lossy().to_string();
        self.unit_cache.insert(source_str, unit);
        
        Ok(())
    }

    /// 清理过期缓存
    pub fn clean_cache(&mut self, max_age_days: u64) -> Result<usize> {
        let now = SystemTime::now();
        let max_age = std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);
        
        let to_remove: Vec<String> = self
            .unit_cache
            .iter()
            .filter(|(_, unit)| {
                now.duration_since(unit.timestamp).unwrap_or(max_age) > max_age
            })
            .map(|(k, _)| k.clone())
            .collect();
        
        let count = to_remove.len();
        for key in to_remove {
            if let Some(unit) = self.unit_cache.remove(&key) {
                // 删除输出文件
                fs::remove_file(&unit.output_path).ok();
            }
        }
        
        Ok(count)
    }

    /// 获取缓存统计
    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            total_units: self.unit_cache.len(),
            total_size: self.unit_cache.values()
                .map(|u| u.output_path.metadata().map(|m| m.len()).unwrap_or(0))
                .sum(),
        }
    }

    /// 使缓存失效
    pub fn invalidate(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();
        self.unit_cache.remove(&path_str);
        
        // 递归使依赖者失效
        let affected = self.get_affected_files(path);
        for file in affected {
            let file_str = file.to_string_lossy().to_string();
            self.unit_cache.remove(&file_str);
        }
    }
}

/// 增量缓存
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IncrementalCache {
    pub dep_graph: DependencyGraph,
    pub units: HashMap<String, CompiledUnit>,
}

/// 缓存统计
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_units: usize,
    pub total_size: u64,
}

impl ChangeDetector {
    /// 创建新的变更检测器
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
        }
    }

    /// 创建快照
    pub fn snapshot(&mut self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)?;
        let hash = compute_file_hash(path)?;
        
        let snapshot = FileSnapshot {
            path: path.to_path_buf(),
            hash,
            mtime: metadata.modified()?,
            size: metadata.len(),
        };
        
        self.snapshots.insert(path.to_path_buf(), snapshot);
        Ok(())
    }

    /// 检查是否变更
    pub fn has_changed(&self, path: &Path) -> bool {
        let Some(snapshot) = self.snapshots.get(path) else {
            return true;
        };
        
        let Ok(metadata) = fs::metadata(path) else {
            return true;
        };
        
        // 快速检查：大小和时间
        if metadata.len() != snapshot.size {
            return true;
        }
        
        if let Ok(mtime) = metadata.modified() {
            if mtime != snapshot.mtime {
                // 时间变了，再检查哈希
                let Ok(hash) = compute_file_hash(path) else {
                    return true;
                };
                return hash != snapshot.hash;
            }
        }
        
        false
    }

    /// 获取所有变更的文件
    pub fn get_changed_files(&self) -> Vec<PathBuf> {
        self.snapshots
            .keys()
            .filter(|p| self.has_changed(p))
            .cloned()
            .collect()
    }
}

/// 计算文件哈希
fn compute_file_hash(path: &Path) -> Result<u64> {
    use std::collections::hash_map::DefaultHasher;
    
    let content = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(hasher.finish())
}

/// 并行编译器
pub struct ParallelCompiler {
    /// 编译器实例
    compiler: IncrementalCompiler,
    /// 并行度
    parallelism: usize,
}

impl ParallelCompiler {
    /// 创建新的并行编译器
    pub fn new(cache_dir: impl AsRef<Path>, parallelism: usize) -> Self {
        Self {
            compiler: IncrementalCompiler::new(cache_dir),
            parallelism,
        }
    }

    /// 并行编译多个文件
    pub fn compile_batch(
        &mut self,
        files: &[PathBuf],
        options: &CompileOptions,
    ) -> Vec<CompileResult> {
        // 过滤出需要编译的文件
        let to_compile: Vec<_> = files
            .iter()
            .filter(|f| self.compiler.needs_recompile(f, options))
            .cloned()
            .collect();
        
        // 按依赖顺序排序
        let sorted = self.topological_sort(&to_compile);
        
        // 并行编译（简化实现，实际使用线程池）
        let mut results = Vec::new();
        for file in sorted {
            results.push(CompileResult {
                source: file,
                success: true,
                output: None,
                error: None,
            });
        }
        
        results
    }

    /// 拓扑排序
    fn topological_sort(&self, files: &[PathBuf]) -> Vec<PathBuf> {
        // 简化实现：实际应该根据依赖图排序
        files.to_vec()
    }
}

/// 编译结果
#[derive(Debug, Clone)]
pub struct CompileResult {
    pub source: PathBuf,
    pub success: bool,
    pub output: Option<PathBuf>,
    pub error: Option<String>,
}

/// 构建系统
pub struct BuildSystem {
    /// 增量编译器
    compiler: IncrementalCompiler,
    /// 变更检测器
    detector: ChangeDetector,
}

impl BuildSystem {
    /// 创建新的构建系统
    pub fn new(cache_dir: impl AsRef<Path>) -> Self {
        Self {
            compiler: IncrementalCompiler::new(cache_dir),
            detector: ChangeDetector::new(),
        }
    }

    /// 增量构建
    pub fn build(&mut self, targets: &[PathBuf], options: &CompileOptions) -> Result<BuildSummary> {
        let start = std::time::Instant::now();
        
        // 加载缓存
        self.compiler.load_cache()?;
        
        // 检测变更
        let changed: Vec<_> = targets
            .iter()
            .filter(|t| {
                !self.compiler.unit_cache.contains_key(&t.to_string_lossy().to_string())
                    || self.detector.has_changed(t)
            })
            .cloned()
            .collect();
        
        // 获取受影响文件
        let mut to_rebuild: HashSet<PathBuf> = changed.iter().cloned().collect();
        for file in &changed {
            to_rebuild.extend(self.compiler.get_affected_files(file));
        }
        
        let needs_compile = to_rebuild.len();
        let cached = targets.len() - needs_compile;
        
        // 执行编译（简化）
        let compiled = needs_compile;
        let failed = 0;
        
        // 保存缓存
        self.compiler.save_cache()?;
        
        Ok(BuildSummary {
            total: targets.len(),
            cached,
            compiled,
            failed,
            duration: start.elapsed(),
        })
    }
}

/// 构建摘要
#[derive(Debug, Clone)]
pub struct BuildSummary {
    pub total: usize,
    pub cached: usize,
    pub compiled: usize,
    pub failed: usize,
    pub duration: std::time::Duration,
}

impl BuildSummary {
    /// 打印摘要
    pub fn print(&self) {
        println!("\n{}", "Build Summary:".bold());
        println!("  Total:    {}", self.total);
        println!("  Cached:   {}", self.cached.to_string().green());
        println!("  Compiled: {}", self.compiled.to_string().yellow());
        if self.failed > 0 {
            println!("  Failed:   {}", self.failed.to_string().red());
        }
        println!("  Time:     {:.2}s", self.duration.as_secs_f64());
    }
}

use colored::Colorize;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_incremental_compiler() {
        let temp = TempDir::new().unwrap();
        let cache_dir = temp.path().join("cache");
        
        let mut compiler = IncrementalCompiler::new(&cache_dir);
        
        // 创建测试文件
        let source = temp.path().join("test.cell");
        fs::write(&source, "module test;").unwrap();
        
        let options = CompileOptions {
            opt_level: 0,
            target: "riscv64".to_string(),
            debug: false,
        };
        
        // 首次编译需要
        assert!(compiler.needs_recompile(&source, &options));
        
        // 记录编译
        let output = temp.path().join("test.o");
        fs::write(&output, "").unwrap();
        compiler.record_compilation(&source, &output, vec![], &options).unwrap();
        
        // 再次编译不需要
        assert!(!compiler.needs_recompile(&source, &options));
        
        // 修改文件后需要
        fs::write(&source, "module test2;").unwrap();
        assert!(compiler.needs_recompile(&source, &options));
    }

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::default();
        
        graph.dependents.entry("a.cell".into()).or_default().insert("b.cell".into());
        graph.dependents.entry("a.cell".into()).or_default().insert("c.cell".into());
        
        let compiler = IncrementalCompiler::new("/tmp/cache");
        let affected = compiler.get_affected_files(&"a.cell".into());
        
        assert!(affected.contains(&"b.cell".into()));
        assert!(affected.contains(&"c.cell".into()));
    }

    #[test]
    fn test_change_detector() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        fs::write(&file, "hello").unwrap();
        
        let mut detector = ChangeDetector::new();
        detector.snapshot(&file).unwrap();
        
        assert!(!detector.has_changed(&file));
        
        fs::write(&file, "world").unwrap();
        assert!(detector.has_changed(&file));
    }
}
