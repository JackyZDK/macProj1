//! 网卡管理器入口。

mod app;
mod log;
mod net;
mod netctl;
mod scripts;
mod theme;

use eframe::egui;

use crate::log::{AppLogs, Sev};

/// 加载系统 CJK 字体，避免界面中文显示为乱码/方块（egui 默认字体不含中文字形）。
/// 按平台尝试候选路径，找到第一个可用字体后追加到 Proportional / Monospace 字体链末尾。
fn setup_fonts(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Supplemental/Songti.ttc",
        ]
    } else if cfg!(target_os = "windows") {
        &[
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyh.ttf",
            "C:\\Windows\\Fonts\\simhei.ttf",
            "C:\\Windows\\Fonts\\simsun.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ]
    };

    let mut fonts = egui::FontDefinitions::default();
    for path in candidates {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".to_owned());
        }
        break;
    }
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    // 命令行模式：--list 打印全部网卡枚举（无 GUI），便于测试与对照 ifconfig。
    if std::env::args().nth(1).as_deref() == Some("--list") {
        let all = net::list_adapters();
        for a in &all {
            println!(
                "{:<8} physical={:<5} up={:<5} running={:<5} ipv4={:<16} netmask={:<16} friendly={}",
                a.name,
                a.is_physical,
                a.up,
                a.running,
                a.ipv4.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                a.netmask_v4.map(|v| v.to_string()).unwrap_or_else(|| "-".into()),
                a.friendly,
            );
        }
        println!("\n物理网卡 {} 个", net::physical_adapters(&all).len());
        return Ok(());
    }

    // 命令行模式：--probe <ifname> 只读打印该接口解析出的网络服务名（不执行任何写操作）。
    if std::env::args().nth(1).as_deref() == Some("--probe") {
        if let Some(name) = std::env::args().nth(2) {
            match netctl::service_name_for_device(&name) {
                Some(svc) => println!("接口 {} -> 网络服务名: {}", name, svc),
                None => println!("接口 {} 未映射到任何网络服务", name),
            }
        }
        return Ok(());
    }

    // 命令行模式：--ctl <name> <start|stop>，无 GUI 的启停测试出口。
    if std::env::args().nth(1).as_deref() == Some("--ctl") {
        let name = std::env::args().nth(2).unwrap_or_default();
        let op = match std::env::args().nth(3).as_deref() {
            Some("start") => netctl::Op::Start,
            Some("stop") => netctl::Op::Stop,
            _ => {
                eprintln!("用法: nicmgr --ctl <name> <start|stop>");
                std::process::exit(2);
            }
        };
        match netctl::set_interface(&name, op) {
            Ok(desc) => println!("OK: {}", desc),
            Err(e) => {
                eprintln!("ERROR: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // 命令行模式：--scr <list|add|delete|run|view>，脚本管理的无 GUI 验证入口。
    if std::env::args().nth(1).as_deref() == Some("--scr") {
        match std::env::args().nth(2).as_deref() {
            Some("list") => {
                let items = scripts::list_scripts();
                if items.is_empty() {
                    println!("（暂无脚本，目录: {}", scripts::scripts_dir().display());
                }
                for s in &items {
                    println!(
                        "{:<12} root={:<5} {}",
                        s.name,
                        s.needs_root,
                        s.path.display()
                    );
                }
            }
            Some("add") => {
                // 用法: --scr add <name> <0|1> < <scriptfile>（内容读 stdin）
                let name = std::env::args().nth(3).unwrap_or_default();
                let needs_root = std::env::args().nth(4).as_deref() == Some("1");
                let mut content = String::new();
                use std::io::Read;
                if std::io::stdin().read_to_string(&mut content).is_err() {
                    eprintln!("读取脚本内容失败");
                    std::process::exit(2);
                }
                match scripts::add_script(&name, &content, needs_root) {
                    Ok(desc) => println!("OK: {}", desc),
                    Err(e) => {
                        eprintln!("ERROR: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Some("delete") => {
                let name = std::env::args().nth(3).unwrap_or_default();
                match scripts::delete_script(&name) {
                    Ok(desc) => println!("OK: {}", desc),
                    Err(e) => {
                        eprintln!("ERROR: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Some("run") => {
                let name = std::env::args().nth(3).unwrap_or_default();
                match scripts::list_scripts().into_iter().find(|s| s.name == name) {
                    Some(item) => match scripts::run_script(&item) {
                        Ok(desc) => println!("OK: {}", desc),
                        Err(e) => {
                            eprintln!("ERROR: {}", e);
                            std::process::exit(1);
                        }
                    },
                    None => {
                        eprintln!("脚本 {} 不存在", name);
                        std::process::exit(1);
                    }
                }
            }
            _ => {
                eprintln!("用法: nicmgr --scr <list|add <name> <0|1> < <file>|delete <name>|run <name>>");
                std::process::exit(2);
            }
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 600.0])
            .with_min_inner_size([640.0, 440.0])
            .with_title("网卡管理器"),
        ..Default::default()
    };

    let mut logs = AppLogs::default();
    logs.push(Sev::Info, "程序启动".to_string());
    logs.push(
        Sev::Info,
        format!("权限模式: {}", if unsafe { libc::geteuid() == 0 } { "管理员(root)" } else { "普通用户" }),
    );

    eframe::run_native(
        "网卡管理器",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::NetApp::new_with_logs(logs)))
        }),
    )
}