//! 包管理器
//!
//! 管理 CellScript 包的依赖、版本和发布

use crate::error::{CompileError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 包清单 (Cell.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageManifest {
    /// 包信息
    pub package: PackageInfo,
    /// 依赖
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    /// 开发依赖
    #[serde(default)]
    pub dev_dependencies: HashMap<String, Dependency>,
    /// 构建配置
    #[serde(default)]
    pub build: BuildConfig,
    /// 检查/发布策略
    #[serde(default)]
    pub policy: PolicyConfig,
    /// 元数据
    #[serde(default)]
    pub metadata: HashMap<String, toml::Value>,
}

/// 包信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// 包名
    pub name: String,
    /// 版本 (遵循 SemVer)
    pub version: String,
    /// 作者
    #[serde(default)]
    pub authors: Vec<String>,
    /// 描述
    #[serde(default)]
    pub description: String,
    /// 许可证
    #[serde(default)]
    pub license: String,
    /// 仓库 URL
    #[serde(default)]
    pub repository: String,
    /// 主页
    #[serde(default)]
    pub homepage: String,
    /// 文档 URL
    #[serde(default)]
    pub documentation: String,
    /// 关键字
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 分类
    #[serde(default)]
    pub categories: Vec<String>,
    /// 最低 CellScript 版本
    #[serde(default)]
    pub cellscript_version: String,
    /// 入口文件
    #[serde(default = "default_entry")]
    pub entry: String,
    /// 包含的文件
    #[serde(default)]
    pub include: Vec<String>,
    /// 排除的文件
    #[serde(default)]
    pub exclude: Vec<String>,
}

fn default_entry() -> String {
    "src/main.cell".to_string()
}

/// 依赖
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// 简单版本
    Simple(String),
    /// 详细配置
    Detailed(DetailedDependency),
}

/// 详细依赖配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    /// 版本要求
    pub version: String,
    /// Git 仓库
    #[serde(default)]
    pub git: Option<String>,
    /// Git 分支
    #[serde(default)]
    pub branch: Option<String>,
    /// Git 标签
    #[serde(default)]
    pub tag: Option<String>,
    /// Git 修订
    #[serde(default)]
    pub rev: Option<String>,
    /// 本地路径
    #[serde(default)]
    pub path: Option<String>,
    /// 是否可选
    #[serde(default)]
    pub optional: bool,
    /// 特性
    #[serde(default)]
    pub features: Vec<String>,
    /// 默认特性
    #[serde(default = "default_true")]
    pub default_features: bool,
}

fn default_true() -> bool {
    true
}

/// 构建配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    /// 脚本
    #[serde(default)]
    pub script: Option<String>,
    /// 依赖
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
}

/// 包级检查策略
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// 拒绝 fail-closed lowering 路径，适合生产/CI
    #[serde(default)]
    pub production: bool,
    /// 显式拒绝 fail-closed runtime features/obligations
    #[serde(default)]
    pub deny_fail_closed: bool,
    /// 拒绝 symbolic Cell/runtime requirements
    #[serde(default)]
    pub deny_symbolic_runtime: bool,
    /// 拒绝 CKB transaction/syscall runtime requirements
    #[serde(default)]
    pub deny_ckb_runtime: bool,
    /// 拒绝需要外部 runtime/scheduler 兑现的 verifier obligations
    #[serde(default)]
    pub deny_runtime_obligations: bool,
}

/// 包管理器
pub struct PackageManager {
    /// 根目录
    root: PathBuf,
    /// 已解析的依赖
    resolved: HashMap<String, ResolvedPackage>,
}

/// 已解析的包
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// 包名
    pub name: String,
    /// 版本
    pub version: String,
    /// 路径
    pub path: PathBuf,
    /// 来源
    pub source: PackageSource,
    /// 依赖
    pub dependencies: Vec<String>,
}

/// 包来源
#[derive(Debug, Clone)]
pub enum PackageSource {
    /// 本地路径
    Local(PathBuf),
    /// Git 仓库
    Git { url: String, revision: String },
    /// 注册表
    Registry { name: String, version: String },
}

/// 版本要求
#[derive(Debug, Clone)]
pub enum VersionReq {
    /// 精确版本
    Exact(String),
    /// 兼容版本 (^1.2.3)
    Compatible(String),
    /// 范围 (>=1.0.0, <2.0.0)
    Range(String),
    /// 任意
    Any,
}

impl PackageManager {
    /// 创建新的包管理器
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();

        Self { root, resolved: HashMap::new() }
    }

    /// 读取包清单
    pub fn read_manifest(&self) -> Result<PackageManifest> {
        let manifest_path = self.root.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span("Cell.toml not found. Run 'cellc init' to create a new package."));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;

        Ok(manifest)
    }

    /// 写入包清单
    pub fn write_manifest(&self, manifest: &PackageManifest) -> Result<()> {
        let manifest_path = self.root.join("Cell.toml");
        let content = toml::to_string_pretty(manifest)?;
        std::fs::write(&manifest_path, content)?;
        Ok(())
    }

    /// 初始化新包
    pub fn init(&self, name: &str) -> Result<()> {
        // 创建目录结构
        std::fs::create_dir_all(self.root.join("src"))?;
        std::fs::create_dir_all(self.root.join("tests"))?;
        std::fs::create_dir_all(self.root.join("examples"))?;

        // 创建 Cell.toml
        let manifest = PackageManifest {
            package: PackageInfo {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                authors: vec![],
                description: String::new(),
                license: String::new(),
                repository: String::new(),
                homepage: String::new(),
                documentation: String::new(),
                keywords: vec![],
                categories: vec![],
                cellscript_version: String::new(),
                entry: "src/main.cell".to_string(),
                include: vec![],
                exclude: vec![],
            },
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            build: BuildConfig::default(),
            policy: PolicyConfig::default(),
            metadata: HashMap::new(),
        };

        self.write_manifest(&manifest)?;

        // 创建默认入口文件
        let main_content = format!(
            r#"module {};

// Entry point for {}
"#,
            name, name
        );
        std::fs::write(self.root.join("src/main.cell"), main_content)?;

        // 创建 .gitignore
        let gitignore = r#"# CellScript
.cell/
build/
dist/
*.o
*.bin
"#;
        std::fs::write(self.root.join(".gitignore"), gitignore)?;

        Ok(())
    }

    /// 添加依赖
    pub fn add_dependency(&self, name: &str, version: &str) -> Result<()> {
        let mut manifest = self.read_manifest()?;

        manifest.dependencies.insert(name.to_string(), Dependency::Simple(version.to_string()));

        self.write_manifest(&manifest)?;
        Ok(())
    }

    /// 移除依赖
    pub fn remove_dependency(&self, name: &str) -> Result<()> {
        let mut manifest = self.read_manifest()?;
        manifest.dependencies.remove(name);
        self.write_manifest(&manifest)?;
        Ok(())
    }

    /// 解析依赖
    pub fn resolve_dependencies(&mut self) -> Result<()> {
        let manifest = self.read_manifest()?;

        for (name, dep) in &manifest.dependencies {
            self.resolve_dependency(name, dep)?;
        }

        Ok(())
    }

    /// 解析单个依赖
    fn resolve_dependency(&mut self, name: &str, dep: &Dependency) -> Result<()> {
        if self.resolved.contains_key(name) {
            return Ok(());
        }

        let resolved = match dep {
            Dependency::Simple(version) => self.resolve_from_registry(name, version)?,
            Dependency::Detailed(detailed) => {
                if let Some(path) = &detailed.path {
                    self.resolve_from_path(name, path)?
                } else if let Some(git) = &detailed.git {
                    self.resolve_from_git(name, git, detailed)?
                } else {
                    self.resolve_from_registry(name, &detailed.version)?
                }
            }
        };

        self.resolved.insert(name.to_string(), resolved);
        Ok(())
    }

    /// 从注册表解析
    fn resolve_from_registry(&self, name: &str, version: &str) -> Result<ResolvedPackage> {
        Err(CompileError::without_span(format!(
            "registry dependency '{}' with version '{}' is not supported yet; use a local path dependency",
            name, version
        )))
    }

    /// 从本地路径解析
    fn resolve_from_path(&self, name: &str, path: &str) -> Result<ResolvedPackage> {
        let package_path = self.root.join(path);
        let manifest_path = package_path.join("Cell.toml");

        if !manifest_path.exists() {
            return Err(CompileError::without_span(format!("Dependency '{}' not found at path '{}'", name, path)));
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PackageManifest = toml::from_str(&content)?;

        Ok(ResolvedPackage {
            name: name.to_string(),
            version: manifest.package.version,
            path: package_path,
            source: PackageSource::Local(PathBuf::from(path)),
            dependencies: manifest.dependencies.keys().cloned().collect(),
        })
    }

    /// 从 Git 解析
    fn resolve_from_git(&self, name: &str, url: &str, detailed: &DetailedDependency) -> Result<ResolvedPackage> {
        let revision = detailed.rev.clone().or(detailed.tag.clone()).or(detailed.branch.clone()).unwrap_or_else(|| "main".to_string());
        Err(CompileError::without_span(format!(
            "git dependency '{}' from '{}' at revision '{}' is not supported yet; use a local path dependency",
            name, url, revision
        )))
    }

    /// 获取已解析的依赖
    pub fn get_resolved(&self) -> &HashMap<String, ResolvedPackage> {
        &self.resolved
    }

    /// 构建依赖图
    pub fn build_dependency_graph(&self) -> DependencyGraph {
        let mut graph = DependencyGraph::new();

        for (name, package) in &self.resolved {
            graph.add_node(name.clone());
            for dep in &package.dependencies {
                graph.add_edge(name.clone(), dep.clone());
            }
        }

        graph
    }

    /// 检查循环依赖
    pub fn check_circular_deps(&self) -> Result<()> {
        let graph = self.build_dependency_graph();

        if let Some(cycle) = graph.find_cycle() {
            return Err(CompileError::without_span(format!("Circular dependency detected: {}", cycle.join(" -> "))));
        }

        Ok(())
    }

    /// 获取依赖的源码路径
    pub fn get_source_paths(&self) -> Vec<PathBuf> {
        self.resolved.values().map(|p| p.path.join("src")).collect()
    }
}

/// 依赖图
pub struct DependencyGraph {
    nodes: Vec<String>,
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// 创建新的依赖图
    pub fn new() -> Self {
        Self { nodes: Vec::new(), edges: HashMap::new() }
    }

    /// 添加节点
    pub fn add_node(&mut self, name: String) {
        if !self.nodes.contains(&name) {
            self.nodes.push(name);
        }
    }

    /// 添加边
    pub fn add_edge(&mut self, from: String, to: String) {
        self.edges.entry(from).or_default().push(to);
    }

    /// 查找循环
    pub fn find_cycle(&self) -> Option<Vec<String>> {
        let mut visited = HashMap::new();
        let mut rec_stack = Vec::new();

        for node in &self.nodes {
            if !visited.contains_key(node) {
                if let Some(cycle) = self.dfs_find_cycle(node, &mut visited, &mut rec_stack) {
                    return Some(cycle);
                }
            }
        }

        None
    }

    /// DFS 查找循环
    fn dfs_find_cycle(&self, node: &str, visited: &mut HashMap<String, bool>, rec_stack: &mut Vec<String>) -> Option<Vec<String>> {
        visited.insert(node.to_string(), true);
        rec_stack.push(node.to_string());

        if let Some(neighbors) = self.edges.get(node) {
            for neighbor in neighbors {
                if !visited.contains_key(neighbor) {
                    if let Some(cycle) = self.dfs_find_cycle(neighbor, visited, rec_stack) {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(neighbor) {
                    // 发现循环
                    let idx = rec_stack.iter().position(|n| n == neighbor).unwrap();
                    let mut cycle = rec_stack[idx..].to_vec();
                    cycle.push(neighbor.to_string());
                    return Some(cycle);
                }
            }
        }

        rec_stack.pop();
        None
    }
}

/// 版本解析
pub mod version {
    use super::*;

    /// 解析版本要求
    pub fn parse_version_req(req: &str) -> Result<VersionReq> {
        if req == "*" {
            return Ok(VersionReq::Any);
        }

        if req.starts_with('^') {
            return Ok(VersionReq::Compatible(req[1..].to_string()));
        }

        if req.starts_with('=') {
            return Ok(VersionReq::Exact(req[1..].to_string()));
        }

        if req.contains(',') || req.contains('>') || req.contains('<') {
            return Ok(VersionReq::Range(req.to_string()));
        }

        // 默认为兼容版本
        Ok(VersionReq::Compatible(req.to_string()))
    }

    /// 检查版本是否满足要求
    pub fn satisfies(version: &str, req: &VersionReq) -> bool {
        match req {
            VersionReq::Any => true,
            VersionReq::Exact(v) => version == v,
            VersionReq::Compatible(v) => is_compatible(version, v),
            VersionReq::Range(r) => satisfies_range(version, r),
        }
    }

    /// 检查是否兼容 (^)
    fn is_compatible(version: &str, base: &str) -> bool {
        let v_parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
        let b_parts: Vec<u32> = base.split('.').filter_map(|p| p.parse().ok()).collect();

        if v_parts.is_empty() || b_parts.is_empty() {
            return false;
        }

        // 主版本必须相同
        if v_parts[0] != b_parts[0] {
            return false;
        }

        // 如果主版本为 0，次版本也必须相同
        if v_parts[0] == 0 {
            if v_parts.len() < 2 || b_parts.len() < 2 {
                return false;
            }
            if v_parts[1] != b_parts[1] {
                return false;
            }
        }

        true
    }

    /// 检查是否满足范围
    fn satisfies_range(_version: &str, _range: &str) -> bool {
        // 简化实现
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_manifest_serialization() {
        let manifest = PackageManifest {
            package: PackageInfo {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                authors: vec!["Test Author".to_string()],
                description: "Test package".to_string(),
                license: "MIT".to_string(),
                repository: String::new(),
                homepage: String::new(),
                documentation: String::new(),
                keywords: vec!["test".to_string()],
                categories: vec!["test".to_string()],
                cellscript_version: String::new(),
                entry: "src/main.cell".to_string(),
                include: vec![],
                exclude: vec![],
            },
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            build: BuildConfig::default(),
            policy: PolicyConfig::default(),
            metadata: HashMap::new(),
        };

        let toml_str = toml::to_string(&manifest).unwrap();
        assert!(toml_str.contains("name = \"test\""));
        assert!(toml_str.contains("version = \"0.1.0\""));
    }

    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::new();
        graph.add_node("A".to_string());
        graph.add_node("B".to_string());
        graph.add_node("C".to_string());
        graph.add_edge("A".to_string(), "B".to_string());
        graph.add_edge("B".to_string(), "C".to_string());

        assert!(graph.find_cycle().is_none());

        // 添加循环
        graph.add_edge("C".to_string(), "A".to_string());
        assert!(graph.find_cycle().is_some());
    }

    #[test]
    fn test_version_compatibility() {
        assert!(version::satisfies("1.2.3", &VersionReq::Compatible("1.0.0".to_string())));
        assert!(version::satisfies("1.5.0", &VersionReq::Compatible("1.2.3".to_string())));
        assert!(!version::satisfies("2.0.0", &VersionReq::Compatible("1.0.0".to_string())));
        assert!(!version::satisfies("0.2.0", &VersionReq::Compatible("0.1.0".to_string())));
        assert!(version::satisfies("0.1.5", &VersionReq::Compatible("0.1.0".to_string())));
    }

    #[test]
    fn package_manager_resolves_local_path_dependencies() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("deps/math/src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.math]
version = "0.1.0"
path = "deps/math"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("deps/math/Cell.toml"),
            r#"
[package]
name = "math"
version = "0.1.0"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(root);
        manager.resolve_dependencies().unwrap();

        let math = manager.get_resolved().get("math").expect("path dependency should resolve");
        assert_eq!(math.name, "math");
        assert_eq!(math.version, "0.1.0");
        assert!(matches!(math.source, PackageSource::Local(_)));
        assert_eq!(manager.get_source_paths(), vec![root.join("deps/math/src")]);
    }

    #[test]
    fn package_manager_rejects_registry_dependencies_fail_closed() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cell.toml"),
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
remote = "1.2.3"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(temp.path());
        let error = manager.resolve_dependencies().unwrap_err();

        assert!(error.message.contains("registry dependency 'remote'"));
        assert!(error.message.contains("not supported yet"));
        assert!(error.message.contains("local path dependency"));
        assert!(manager.get_resolved().is_empty());
    }

    #[test]
    fn package_manager_rejects_git_dependencies_fail_closed() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("Cell.toml"),
            r#"
[package]
name = "app"
version = "0.1.0"

[dependencies.remote]
version = "0.1.0"
git = "https://example.invalid/remote.git"
rev = "abc123"
"#,
        )
        .unwrap();

        let mut manager = PackageManager::new(temp.path());
        let error = manager.resolve_dependencies().unwrap_err();

        assert!(error.message.contains("git dependency 'remote'"));
        assert!(error.message.contains("https://example.invalid/remote.git"));
        assert!(error.message.contains("abc123"));
        assert!(error.message.contains("local path dependency"));
        assert!(manager.get_resolved().is_empty());
    }
}
