# Mac 网卡管理器（macOS 优先，兼容 Windows）— 设计文档

> 状态：待评审（评审通过后实施）

## 1. 需求确认

| # | 需求 | 说明 |
|---|------|------|
| 1 | 显示物理网卡列表 | 状态（up/down）、IPv4 地址、子网掩码；过滤虚拟接口（lo0/utun/awdl/vmnet 等） |
| 2 | 「启动 / 关闭」按钮 | 每个物理网卡一行两个按钮，执行启/停 |
| 3 | 操作日志 | 界面内展示 + 落盘文件 |
| 4 | 跨平台 | 主跑 macOS（arm64，macOS 27），源码级兼容 Windows；Windows 功能声明为"待验证" |

## 2. 技术选型

| 项 | 选择 | 理由 |
|----|------|------|
| 语言 | Rust 2021 edition | 单二进制分发，跨平台 |
| GUI | `eframe` + `egui` 0.29 | 纯 Rust 轻量、表格/滚动区简单、无需系统 WebView |
| 网卡枚举 | `nix` 0.29（getifaddrs + ioctl）+ `if-addrs`（Windows 分支） | nix 直接拿接口 flags（IFF_UP/IFF_RUNNING）与 netmask；Windows 用 if-addrs 兜底 |
| 启停 | 命令下发，不做内核 ioctl 直改 | 实现简单、可控、易审计 |
| 日志 | 自研轻量 logger：内存环形 Vec + 文件追加 | 不引重型日志框架 |

依赖清单（Cargo.toml）：
- `eframe = "0.29"`（含 egui）
- `nix = { version = "0.29", features = ["net", "ioctl"] }`（仅 unix）
- `if-addrs = "0.13"`（Windows/enum 兜底）
- `libc = "0.2"`（syscall 常量）

## 3. 接口契约（模块边界）

### 3.1 网卡枚举 `src/net.rs`

```rust
pub struct NetAdapter {
    pub name: String,          // 接口名，如 en0
    pub friendly: String,      // 友好名（macOS 硬件端口名，如 Wi-Fi）
    pub is_physical: bool,     // 是否物理网卡
    pub up: bool,              // IFF_UP
    pub running: bool,         // IFF_RUNNING（有链路）
    pub ipv4: Option<Ipv4Addr>,
    pub netmask_v4: Option<Ipv4Addr>,
    // 预留：pub ipv6: Option<Ipv6Addr>,
}

pub fn list_adapters() -> Vec<NetAdapter>;   // 返回全部接口（含虚拟）
pub fn physical_adapters(vec: &[NetAdapter]) -> Vec<NetAdapter>; // 过滤虚拟接口
```

过滤规则（macOS）：保留 `en*`（Wi-Fi/以太网/Thunderbolt 桥接网卡）；排除
`lo0`、`utun*`、`awdl*`、`llw*`、`bridge*`、`gif*`、`stf*`、`tun*`、`tap*`、`vmnet*`、`vboxnet*`。
Windows：按 `if-addrs` 结果过滤 Loopback / vEthernet / isatap / 虚拟网卡前缀，标注"待验证"。

### 3.2 启停控制 `src/netctl.rs`

```rust
pub enum Op { Start, Stop }

/// 启动/关闭指定接口，返回操作结果描述（用于日志）
pub fn set_interface(name: &str, op: Op) -> Result<String, String>;
```

执行策略（macOS）：
1. 若进程 euid==0（sudo 运行）：直接执行 `ifconfig <name> up|down` —— 可靠、立刻生效。
2. 非 root：先解析 `networksetup -listallhardwareports` 找到 `Device: <name>` 对应的
   `Hardware Port`（如 "Wi-Fi"），再执行 `networksetup -setnetworkserviceenabled <service> on|off`。
   - 注意：networksetup 的 on/off 对应启用/禁用该网络服务，接口不 down，但功能上等效"关闭该网卡连接"。
   - 若失败（多为权限不足）返回明确错误：需要管理员权限。

权限说明：关闭/开启物理网卡在 macOS 上普遍需要管理员权限；普通用户在
networksetup 路径下也会失败，日志中提示"请使用 sudo 运行本程序以获得完整启停能力"。

### 3.3 日志 `src/log.rs`

```rust
pub enum Sev { Info, Warn, Error }
pub struct AppLogs { ring: VecDeque<(Sev, String, Instant)>, }
impl AppLogs {
    pub fn push(&mut self, sev: Sev, msg: String);   // 写入内存 + 追加到文件
    pub fn iter(&self) -> impl Iterator<Item = &(Sev, String, Instant)>;
    pub fn log_file() -> PathBuf;  // ~/Library/Logs/nicmgr.log（macOS）
}
```

日志文件路径：macOS `~/Library/Logs/nicmgr.log`；Windows `%LOCALAPPDATA%\nicmgr\nicmgr.log`（待验证）。
每条日志：时间戳 `[HH:MM:SS] [level] message`。

### 3.4 GUI `src/main.rs` + `src/app.rs`

布局（egui CentralPanel + TopBottomPanel + SidePanel）：
- 顶部：标题「网卡管理器」+「刷新」按钮 + 当前 root 状态提示（euid==0 ? "管理员模式" : "普通模式（启停可能需 sudo）"）
- 中部：表格列 = 名称 | 友好名 | 状态（up/down/已连接） | IP | 子网掩码 | 操作（[启动][关闭] 按钮）
- 底部：日志区（等宽字体，滚动，仅显示内存环形缓冲最近 1000 条）
- 每 2s 自动刷新枚举（不打断按钮操作）

## 4. 任务分解

- [ ] T1 初始化 cargo 工程骨架（Cargo.toml、src 结构、gitignore 已存在）
- [ ] T2 网卡枚举模块 net.rs（nix getifaddrs + ioctl 取 flags/netmask，物理过滤）
- [ ] T3 启停控制模块 netctl.rs（root→ifconfig；非 root→networksetup 映射与调用）
- [ ] T4 日志模块 log.rs（内存环形 + 文件追加）
- [ ] T5 GUI app.rs + main.rs（表格、按钮、日志区、自动刷新、root 状态提示）
- [ ] T6 验证：
  - `cargo build` 通过
  - 运行 `cargo run` 观察枚举结果与真实网卡一致（对照 `ifconfig` 输出）
  - 在管理员模式下验证对虚拟/物理接口的 up/down（以无副作用接口如放弃验证，先于非关键接口测试）
  - Windows 交叉编译不验证（无 win 环境），源码分支编译由 cfg 隔离

## 5. 风险与说明

| 风险 | 缓解 |
|------|------|
| 启停需要 root | UI 明示权限模式；非 root 走 networksetup 并给出明确错误 |
| "物理网卡"判定（macOS 上 en 前缀含 Thunderbolt/USB 虚拟网卡） | 优先保留 en*，文档注明；后续可按硬件端口细分 |
| Windows 无法在 mac 上验证 | cfg 隔离 Windows 分支，标注"待验证"，不阻塞 mac 主流程 |
| networksetup 服务名映射解析失败 | 解析失败时回退 ifconfig（若权限允许），并记日志 |