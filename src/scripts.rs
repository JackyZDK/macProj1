//! 脚本管理：用户维护的 shell 脚本（用于路由刷新等操作）。
//!
//! 存储位置：
//! - macOS：`~/Library/Application Support/nicmgr/scripts/`
//! - Windows：`%LOCALAPPDATA%\nicmgr\scripts\`
//! - 其他 Unix：`~/.local/share/nicmgr/scripts/`
//!
//! 每个脚本两个文件：`<name>.sh`（内容）与 `<name>.meta`（一行 `needs_root=0|1`）。
//! macOS 执行：`sh <path>`；Windows 执行：`cmd /c <path>`（待验证）。

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ScriptItem {
    pub name: String,
    pub needs_root: bool,
    pub path: PathBuf,
}

/// 脚本存储目录。
pub fn scripts_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join("Library/Application Support/nicmgr/scripts")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(appdata).join("nicmgr/scripts")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/share/nicmgr/scripts")
    }
}

fn ext() -> &'static str {
    if cfg!(target_os = "windows") {
        "bat"
    } else {
        "sh"
    }
}

fn script_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.{}", name, ext()))
}

fn meta_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.meta", name))
}

fn read_needs_root(dir: &Path, name: &str) -> bool {
    fs::read_to_string(meta_path(dir, name))
        .map(|s| s.trim() == "needs_root=1")
        .unwrap_or(false)
}

/// 列出全部脚本（按名称排序）。
pub fn list_scripts() -> Vec<ScriptItem> {
    let dir = scripts_dir();
    let suffix = format!(".{}", ext());
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                continue;
            }
            let fname = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !fname.ends_with(&suffix) {
                continue;
            }
            let name = fname.trim_end_matches(&suffix).to_string();
            out.push(ScriptItem {
                needs_root: read_needs_root(&dir, &name),
                path: p,
                name,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 添加脚本。name 去首尾空白并按基名校验；内容不能为空；重名报错。
pub fn add_script(name: &str, content: &str, needs_root: bool) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("脚本名不能为空".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("脚本名含非法字符（/ \\ .. 不允许）".into());
    }
    if content.trim().is_empty() {
        return Err("脚本内容不能为空".into());
    }
    let dir = scripts_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建脚本目录失败: {}", e))?;
    let sp = script_path(&dir, name);
    if sp.exists() {
        return Err(format!("脚本 {} 已存在", name));
    }
    fs::write(&sp, content).map_err(|e| format!("写入脚本失败: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&sp, fs::Permissions::from_mode(0o755));
    }
    fs::write(
        meta_path(&dir, name),
        format!("needs_root={}\n", if needs_root { 1 } else { 0 }),
    )
    .map_err(|e| format!("写入脚本元数据失败: {}", e))?;
    Ok(format!("已添加脚本 {}", name))
}

/// 删除脚本（连同元数据）。
pub fn delete_script(name: &str) -> Result<String, String> {
    let dir = scripts_dir();
    let sp = script_path(&dir, name);
    let mut removed = false;
    if sp.exists() {
        fs::remove_file(&sp).map_err(|e| format!("删除脚本失败: {}", e))?;
        removed = true;
    }
    let _ = fs::remove_file(meta_path(&dir, name));
    if !removed {
        return Err(format!("脚本 {} 不存在", name));
    }
    Ok(format!("已删除脚本 {}", name))
}

/// 查看脚本内容（查看弹窗用）。
pub fn script_content(item: &ScriptItem) -> String {
    fs::read_to_string(&item.path).unwrap_or_default()
}

/// 运行脚本。若要求 root 而当前进程非 root，返回明确错误（GUI 下 sudo 无法交互输密码）。
pub fn run_script(item: &ScriptItem) -> Result<String, String> {
    if !item.path.exists() {
        return Err(format!("脚本文件不存在: {}", item.path.display()));
    }
    if item.needs_root && !is_root() {
        return Err("该脚本需要管理员权限，请以 sudo 运行本程序".into());
    }
    let out = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .arg("/c")
            .arg(&item.path)
            .output()
    } else {
        std::process::Command::new("sh").arg(&item.path).output()
    };
    let out = out.map_err(|e| format!("执行脚本失败: {}", e))?;
    if out.status.success() {
        Ok(format!("脚本 {} 执行成功{}", item.name, stdout_tail(&out.stdout)))
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = if err.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err.trim().to_string()
        };
        Err(format!("脚本 {} 执行失败: {}", item.name, err))
    }
}

fn is_root() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// 截取 stdout 前 3 行（单条日志行长度可控）。
fn stdout_tail(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout).trim().to_string();
    if text.is_empty() {
        return String::new();
    }
    let joined = text.lines().take(3).collect::<Vec<_>>().join(" | ");
    let s: String = joined.chars().take(200).collect();
    if joined.chars().count() > 200 {
        format!("：{}…", s)
    } else {
        format!("：{}", s)
    }
}