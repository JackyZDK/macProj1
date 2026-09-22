//! 网卡枚举：列出主机全部网络接口，标注物理/虚拟、up 状态、IPv4 与子网掩码。
//!
//! - Unix（macOS 主路径）：基于 `nix::ifaddrs`，直接取自 ifaddrs 结构（IFF_UP / IFF_RUNNING / netmask）。
//! - Windows：基于 `if-addrs` crate，物理性/运行状态为尽力推断（标注"待验证"）。

use std::collections::HashMap;
use std::net::Ipv4Addr;

#[derive(Debug, Clone)]
pub struct NetAdapter {
    pub name: String,      // 接口名，如 en0
    pub friendly: String,  // 友好名（macOS 硬件端口名，如 Wi-Fi）
    pub is_physical: bool, // 是否物理网卡（过滤虚拟接口）
    pub up: bool,          // IFF_UP：接口是否已开启
    pub running: bool,     // IFF_RUNNING：是否有链路（Wi-Fi 是否已连接）
    pub ipv4: Option<Ipv4Addr>,
    pub netmask_v4: Option<Ipv4Addr>,
}

impl NetAdapter {
    /// 状态展示：down / up(未连接) / 已连接
    pub fn status_label(&self) -> &'static str {
        if !self.up {
            "down"
        } else if self.running {
            "已连接"
        } else {
            "up"
        }
    }
}

/// 虚拟接口前缀（macOS）。en* 视为物理（Wi-Fi/以太网/Thunderbolt 桥接），其余常见虚拟前缀排除。
fn is_virtual_name(name: &str) -> bool {
    for p in [
        "lo", "utun", "awdl", "llw", "bridge", "gif", "stf", "tun", "tap", "vmnet", "vboxnet",
        "ipsec", "ppp", "ap1", "ap2", "llw", "anpi", "nan", "ipsec0",
    ] {
        if name.starts_with(p) {
            return true;
        }
    }
    false
}

/// 返回全部接口（含虚拟），供调试。
pub fn list_adapters() -> Vec<NetAdapter> {
    #[cfg(unix)]
    {
        list_adapters_unix()
    }
    #[cfg(not(unix))]
    {
        list_adapters_windows()
    }
}

/// 只返回物理网卡。
pub fn physical_adapters(adapters: &[NetAdapter]) -> Vec<NetAdapter> {
    adapters.iter().filter(|a| a.is_physical).cloned().collect()
}

#[cfg(unix)]
fn list_adapters_unix() -> Vec<NetAdapter> {
    use nix::ifaddrs::getifaddrs;
    use nix::net::if_::InterfaceFlags;

    let friendly_map = hardware_port_map();
    let mut out: Vec<NetAdapter> = Vec::new();

    if let Ok(ifaddrs_iter) = getifaddrs() {
        // 接口可能有多条地址记录（多 IP / IPv6），先收集按接口聚合。
        let mut map: HashMap<String, NetAdapter> = HashMap::new();
        for ifa in ifaddrs_iter {
            let name = ifa.interface_name.clone();
            let up = ifa.flags.contains(InterfaceFlags::IFF_UP);
            let running = ifa.flags.contains(InterfaceFlags::IFF_RUNNING);
            let entry = map.entry(name.clone()).or_insert_with(|| {
                let friendly = friendly_map.get(&name).cloned().unwrap_or_default();
                let is_physical = !is_virtual_name(&name);
                NetAdapter {
                    name,
                    friendly,
                    is_physical,
                    up,
                    running,
                    ipv4: None,
                    netmask_v4: None,
                }
            });
            if up {
                entry.up = true;
            }
            if running {
                entry.running = true;
            }
            // IPv4 地址与掩码
            if entry.ipv4.is_none() {
                if let Some(addr) = ifa.address {
                    if let Some(sin) = addr.as_sockaddr_in() {
                        let ip = Ipv4Addr::from(sin.ip());
                        if !ip.is_loopback() {
                            entry.ipv4 = Some(ip);
                        }
                    }
                }
            }
            if entry.netmask_v4.is_none() {
                if let Some(nm) = ifa.netmask {
                    if let Some(sin) = nm.as_sockaddr_in() {
                        entry.netmask_v4 = Some(Ipv4Addr::from(sin.ip()));
                    }
                }
            }
        }
        out = map.into_values().collect();
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// macOS: 通过 `networksetup -listallhardwareports` 解析 Device -> Hardware Port 映射。
#[cfg(target_os = "macos")]
fn hardware_port_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(outp) = std::process::Command::new("networksetup")
        .arg("-listallhardwareports")
        .output()
    else {
        return map;
    };
    let text = String::from_utf8_lossy(&outp.stdout).to_string();
    let mut current_port: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Hardware Port: ") {
            current_port = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Device: ") {
            if let Some(port) = current_port.clone() {
                map.insert(rest.trim().to_string(), port);
            }
        }
    }
    map
}

#[cfg(all(unix, not(target_os = "macos")))]
fn hardware_port_map() -> HashMap<String, String> {
    HashMap::new()
}

/// Windows：用 if-addrs 枚举；物理性与运行状态为尽力推断，标注"待验证"。
#[cfg(windows)]
fn list_adapters_windows() -> Vec<NetAdapter> {
    use if_addrs::IfAddr;
    let mut out = Vec::new();
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        let mut map: HashMap<String, NetAdapter> = HashMap::new();
        for iface in ifs {
            let name = iface.name.clone();
            // Windows 上按名称前缀尽力区分虚拟接口（如 vEthernet / Loopback）
            let is_virtual = iface.name.starts_with("vEthernet")
                || iface.name.starts_with("Loopback")
                || iface.name.starts_with("isatap")
                || iface.name.starts_with("Wireless");
            let entry = map.entry(name.clone()).or_insert_with(|| NetAdapter {
                name,
                friendly: String::new(),
                is_physical: !is_virtual,
                up: true, // if-addrs 不提供 IFF_UP；Windows 侧待验证
                running: true,
                ipv4: None,
                netmask_v4: None,
            });
            if let IfAddr::V4(v4) = iface.addr {
                if entry.ipv4.is_none() && !v4.ip.is_loopback() {
                    entry.ipv4 = Some(v4.ip);
                }
                if entry.netmask_v4.is_none() {
                    entry.netmask_v4 = Some(v4.netmask);
                }
            }
        }
        out = map.into_values().collect();
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}