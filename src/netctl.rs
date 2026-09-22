//! 网卡启停控制。
//!
//! macOS 策略：
//! 1. 进程 euid==0（sudo 运行）：直接 `ifconfig <name> up|down`。
//! 2. 普通用户：通过 `networksetup -listnetworkserviceorder` 解析接口对应的
//!    网络服务名（注意：服务名 ≠ Hardware Port 名，如服务名可能是 "Wi-Fi2" 而
//!    端口名是 "Wi-Fi"，必须用服务名），执行
//!    `networksetup -setnetworkserviceenabled <服务名> on|off`。
//!    —— 禁用/启用网络服务，不 down 接口，但功能等效"关闭该网卡连接"。
//!
//! Windows：使用 `netsh interface set interface <name> admin=disable|enable`（待验证）。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Start,
    Stop,
}

impl Op {
    pub fn verb(&self) -> &'static str {
        match self {
            Op::Start => "up",
            Op::Stop => "down",
        }
    }
    pub fn action_label(&self) -> &'static str {
        match self {
            Op::Start => "启动",
            Op::Stop => "关闭",
        }
    }
}

struct CmdResult {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn run(cmd: &str, args: &[&str]) -> CmdResult {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(o) => CmdResult {
            ok: o.status.success(),
            stdout: String::from_utf8_lossy(&o.stdout).to_string(),
            stderr: String::from_utf8_lossy(&o.stderr).to_string(),
        },
        Err(e) => CmdResult {
            ok: false,
            stdout: String::new(),
            stderr: format!("无法执行 {}: {}", cmd, e),
        },
    }
}

fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// 执行启动/关闭，返回日志用描述（OK 或错误说明）。
pub fn set_interface(name: &str, op: Op) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        if is_root() {
            return set_interface_mac_root(name, op);
        }
        set_interface_mac_user(name, op)
    }
    #[cfg(windows)]
    {
        set_interface_windows(name, op)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = (name, op);
        Err("该平台暂不支持启停".into())
    }
}

#[cfg(target_os = "macos")]
fn set_interface_mac_root(name: &str, op: Op) -> Result<String, String> {
    let r = run("ifconfig", &[name, op.verb()]);
    if r.ok {
        Ok(format!("{} {}（ifconfig {}）", op.action_label(), name, op.verb()))
    } else {
        Err(format!("ifconfig {} {} 失败: {}", name, op.verb(), r.stderr.trim()))
    }
}

#[cfg(target_os = "macos")]
fn set_interface_mac_user(name: &str, op: Op) -> Result<String, String> {
    // 将接口名解析为网络服务名（服务名 ≠ Hardware Port 名，见模块文档）。
    let service = match service_name_for_device(name) {
        Some(s) => s,
        None => {
            return Err(format!(
                "未能在 networksetup 中找到接口 {} 对应的网络服务；请尝试使用 sudo 运行",
                name
            ))
        }
    };
    let keyword = match op {
        Op::Start => "on",
        Op::Stop => "off",
    };
    let r = run(
        "networksetup",
        &["-setnetworkserviceenabled", &service, keyword],
    );
    if r.ok {
        Ok(format!(
            "{} {}（networksetup -setnetworkserviceenabled {} {}）",
            op.action_label(),
            name,
            service,
            keyword
        ))
    } else {
        Err(format!(
            "networksetup -setnetworkserviceenabled {} {} 失败: {}",
            service,
            keyword,
            r.stderr.trim()
        ))
    }
}

/// 解析 `networksetup -listnetworkserviceorder`，返回 Device 对应的**网络服务名**。
///
/// 输出形如：
/// ```text
/// (1) USB 10/100/1000 LAN
/// (Hardware Port: USB 10/100/1000 LAN, Device: en7)
///
/// (3) Wi-Fi2
/// (Hardware Port: Wi-Fi, Device: en0)
/// ```
/// 必须匹配 `(序号) 服务名` 行，而不是 Hardware Port 名。
#[cfg(target_os = "macos")]
pub(crate) fn service_name_for_device(name: &str) -> Option<String> {
    let r = run("networksetup", &["-listnetworkserviceorder"]);
    if !r.ok {
        return None;
    }
    let mut current_service: Option<String> = None;
    for line in r.stdout.lines() {
        let line = line.trim();
        // 必须先匹配 "(Hardware Port: ..." 行：它也以 "(" 开头，若放后面会被服务名分支误吞。
        if let Some(rest) = line.strip_prefix("(Hardware Port: ") {
            // "(Hardware Port: XXX, Device: en0)"：从中解析 Device
            if let Some(dev_start) = rest.find("Device: ") {
                let dev = rest[dev_start + "Device: ".len()..]
                    .trim_end_matches(')')
                    .trim();
                if dev == name {
                    return current_service;
                }
            }
        } else if line.starts_with('(') {
            // "(N) ServiceName"：取括号后的服务名
            if let Some(idx) = line.find(')') {
                current_service = Some(line[idx + 1..].trim().to_string());
            }
        }
    }
    None
}

#[cfg(windows)]
fn set_interface_windows(name: &str, op: Op) -> Result<String, String> {
    let action = match op {
        Op::Start => "enable",
        Op::Stop => "disable",
    };
    let r = run(
        "netsh",
        &["interface", "set", "interface", name, format!("admin={}", action).as_str()],
    );
    if r.ok {
        Ok(format!("{} {}（netsh admin={}）", op.action_label(), name, action))
    } else {
        Err(format!("netsh 操作 {} {} 失败: {}", name, action, r.stderr.trim()))
    }
}