//! egui 界面：网卡表格（状态/IP/掩码/启停按钮）+ 操作日志区。

use std::time::{Duration, Instant};

use eframe::egui;

use crate::log::{AppLogs, Sev};
use crate::net::{list_adapters, physical_adapters, NetAdapter};
use crate::netctl::{self, Op};

pub struct NetApp {
    adapters: Vec<NetAdapter>,
    show_all: bool,
    logs: AppLogs,
    last_refresh: Instant,
    /// 待用户二次确认的操作（点按钮后先弹确认框，确认后才执行）
    confirm: Option<ConfirmReq>,
}

/// 一次待确认的启停操作。
struct ConfirmReq {
    name: String,
    op: Op,
    multi: bool, // true = 批量操作（全部接口）
}

impl Default for NetApp {
    fn default() -> Self {
        Self {
            adapters: Vec::new(),
            show_all: false,
            logs: AppLogs::default(),
            last_refresh: Instant::now() - Duration::from_secs(10),
            confirm: None,
        }
    }
}

impl NetApp {
    pub fn new() -> Self {
        let mut app = Self::default();
        app.refresh();
        app
    }

    pub fn new_with_logs(logs: AppLogs) -> Self {
        let mut app = Self::new();
        app.logs = logs;
        app
    }

    fn refresh(&mut self) {
        let all = list_adapters();
        self.adapters = all;
        self.last_refresh = Instant::now();
    }

    /// 请求一次操作：记录待确认项（若确认框未打开）。
    fn request_toggle(&mut self, name: &str, op: Op) {
        if self.confirm.is_none() {
            self.confirm = Some(ConfirmReq {
                name: name.to_string(),
                op,
                multi: false,
            });
        }
    }

    /// 请求一次批量操作。
    fn request_toggle_all(&mut self, op: Op) {
        if self.confirm.is_none() {
            self.confirm = Some(ConfirmReq {
                name: String::new(),
                op,
                multi: true,
            });
        }
    }

    /// 真正执行单个接口启停。
    fn do_toggle(&mut self, name: &str, op: Op) {
        self.logs.push(Sev::Info, format!("{} {}…", op.action_label(), name));
        match netctl::set_interface(name, op) {
            Ok(desc) => self.success(desc),
            Err(e) => self.failure(e),
        }
        self.refresh();
    }

    /// 真正执行批量启停。
    fn do_toggle_all(&mut self, op: Op) {
        let targets: Vec<String> = self
            .shown_adapters()
            .iter()
            .map(|a| a.name.clone())
            .collect();
        let n = targets.len();
        for name in &targets {
            let _ = netctl::set_interface(name, op);
        }
        self.logs.push(
            Sev::Info,
            format!("已对 {} 个接口执行{}", n, op.action_label()),
        );
        self.refresh();
    }

    fn success(&mut self, msg: String) {
        self.logs.push(Sev::Info, msg);
    }

    fn failure(&mut self, msg: String) {
        self.logs.push(Sev::Error, msg);
    }

    fn shown_adapters(&self) -> Vec<NetAdapter> {
        if self.show_all {
            self.adapters.clone()
        } else {
            physical_adapters(&self.adapters)
        }
    }

    /// 渲染待确认弹窗；返回 true 表示已处理完毕（用户点了确认或取消）。
    fn show_confirm_window(&mut self, ctx: &egui::Context) -> bool {
        let Some(req) = self.confirm.as_ref() else {
            return false;
        };
        let mut decided: Option<bool> = None;
        egui::Window::new("确认操作")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(if req.multi {
                    format!(
                        "确定要对当前显示的全部接口执行「{}」吗？",
                        req.op.action_label()
                    )
                } else {
                    format!(
                        "确定要「{}」接口 {} 吗？",
                        req.op.action_label(),
                        req.name
                    )
                });
                if req.op == Op::Stop {
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 90, 90),
                        "警告：关闭将断开该接口的网络连接。",
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        decided = Some(false);
                    }
                    if ui.button("确认").clicked() {
                        decided = Some(true);
                    }
                });
            });

        if let Some(ok) = decided {
            let req = self.confirm.take().unwrap();
            if ok {
                if req.multi {
                    self.do_toggle_all(req.op);
                } else {
                    self.do_toggle(&req.name, req.op);
                }
            } else {
                self.logs.push(Sev::Info, format!("已取消{}操作", req.op.action_label()));
            }
            return true;
        }
        false
    }
}

impl eframe::App for NetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 每 2s 自动刷新列表状态
        if self.last_refresh.elapsed() >= Duration::from_secs(2) {
            self.refresh();
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("网卡管理器");
                ui.separator();
                if ui.button("刷新").clicked() {
                    self.refresh();
                    self.logs.push(Sev::Info, "手动刷新网卡列表".to_string());
                }
                ui.separator();
                ui.checkbox(&mut self.show_all, "显示全部接口（含虚拟）");
                ui.separator();
                if self.show_all {
                    if ui.button("启动全部(显示中)").clicked() {
                        self.request_toggle_all(Op::Start);
                    }
                    if ui.button("关闭全部(显示中)").clicked() {
                        self.request_toggle_all(Op::Stop);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_root_shim() {
                        ui.colored_label(egui::Color32::from_rgb(220, 130, 60), "管理员模式");
                    } else {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            "普通模式（启停可能需管理员权限）",
                        );
                    }
                });
            });
            ui.add_space(6.0);
        });

        egui::TopBottomPanel::bottom("logs")
            .resizable(true)
            .default_height(160.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.heading(format!("操作日志（{} 条，文件见 {}）", self.logs.len(), AppLogs::log_file().display()));
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add_space(2.0);
                        for e in self.logs.latest_first() {
                            let color = match e.sev {
                                Sev::Info => egui::Color32::LIGHT_GRAY,
                                Sev::Warn => egui::Color32::from_rgb(220, 180, 80),
                                Sev::Error => egui::Color32::from_rgb(230, 90, 90),
                            };
                            ui.horizontal(|ui| {
                                ui.monospace(
                                    egui::RichText::new(format!("[{}]", e.ts)).color(
                                        egui::Color32::from_rgb(120, 160, 210),
                                    ),
                                );
                                ui.monospace(
                                    egui::RichText::new(format!("[{}]", e.sev.label()))
                                        .color(color),
                                );
                                ui.monospace(egui::RichText::new(&e.msg).color(color));
                            });
                        }
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let shown = self.shown_adapters();
            ui.add_space(4.0);
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("adapters")
                        .striped(true)
                        .min_col_width(60.0)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.strong("名称");
                            ui.strong("类型");
                            ui.strong("友好名");
                            ui.strong("状态");
                            ui.strong("IPv4");
                            ui.strong("子网掩码");
                            ui.strong("操作");
                            ui.end_row();

                            for a in &shown {
                                ui.monospace(&a.name);
                                if a.is_physical {
                                    ui.label("物理");
                                } else {
                                    ui.colored_label(egui::Color32::GRAY, "虚拟");
                                }
                                ui.label(&a.friendly);

                                let (label, color) = match a.status_label() {
                                    "已连接" => ("已连接", egui::Color32::from_rgb(90, 200, 120)),
                                    "up" => ("up", egui::Color32::from_rgb(90, 160, 230)),
                                    _ => ("down", egui::Color32::from_rgb(180, 90, 90)),
                                };
                                ui.colored_label(color, label);

                                match a.ipv4 {
                                    Some(ip) => ui.monospace(ip.to_string()),
                                    None => ui.colored_label(egui::Color32::GRAY, "-"),
                                };
                                match a.netmask_v4 {
                                    Some(m) => ui.monospace(m.to_string()),
                                    None => ui.colored_label(egui::Color32::GRAY, "-"),
                                };

                                let name = a.name.clone();
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(a.is_physical, egui::Button::new("启动"))
                                        .clicked()
                                    {
                                        let n = name.clone();
                                        self.request_toggle(&n, Op::Start);
                                    }
                                    if ui
                                        .add_enabled(a.is_physical, egui::Button::new("关闭"))
                                        .clicked()
                                    {
                                        let n = name.clone();
                                        self.request_toggle(&n, Op::Stop);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
            ui.add_space(4.0);
        });

        // 待确认弹窗必须最后渲染，避免与表格/日志面板交互冲突。
        self.show_confirm_window(ctx);

        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

/// root 检测：与 netctl 共用的判断，置于界面以显示当前权限模式。
fn is_root_shim() -> bool {
    unsafe { libc::geteuid() == 0 }
}