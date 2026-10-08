//! egui 界面：Tab 页（网卡管理 / 路由与脚本）+ 操作日志区。
//! - 网卡管理：表格（状态/IP/掩码/启停按钮）
//! - 路由与脚本：选用用户维护的脚本来执行「路由刷新」；脚本通过添加/删除维护
//! - 底部：全局操作日志

use std::time::{Duration, Instant};

use eframe::egui;

use crate::log::{AppLogs, Sev};
use crate::net::{list_adapters, physical_adapters, NetAdapter};
use crate::netctl::{self, Op};
use crate::scripts::{self, ScriptItem};
use crate::theme::{self, GREEN, ORANGE, RED, TEXT_MAIN, TEXT_SUB, UP_BLUE};

#[derive(PartialEq, Clone, Copy)]
enum AppTab {
    Net,
    Route,
}

/// 一次待确认的操作。
enum ConfirmReq {
    /// 网卡启停
    Toggle { name: String, op: Op, multi: bool },
    /// 删除脚本
    DeleteScript { name: String },
}

/// 「添加脚本」弹窗表单。
struct AddScriptDlg {
    name: String,
    content: String,
    needs_root: bool,
}

pub struct NetApp {
    adapters: Vec<NetAdapter>,
    show_all: bool,
    logs: AppLogs,
    last_refresh: Instant,
    confirm: Option<ConfirmReq>,
    tab: AppTab,
    scripts: Vec<ScriptItem>,
    /// 路由刷新下拉选中的脚本名（None = 未选中/无脚本）
    selected_script: Option<String>,
    add_script: Option<AddScriptDlg>,
    /// 查看脚本弹窗（脚本名, 内容）
    view_script: Option<(String, String)>,
}

impl Default for NetApp {
    fn default() -> Self {
        Self {
            adapters: Vec::new(),
            show_all: false,
            logs: AppLogs::default(),
            last_refresh: Instant::now() - Duration::from_secs(10),
            confirm: None,
            tab: AppTab::Net,
            scripts: Vec::new(),
            selected_script: None,
            add_script: None,
            view_script: None,
        }
    }
}

impl NetApp {
    pub fn new() -> Self {
        let mut app = Self::default();
        app.refresh();
        app.load_scripts();
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

    fn load_scripts(&mut self) {
        self.scripts = scripts::list_scripts();
        // 选中项失效则重置
        if let Some(sel) = &self.selected_script {
            if !self.scripts.iter().any(|s| &s.name == sel) {
                self.selected_script = None;
            }
        }
        if self.selected_script.is_none() {
            self.selected_script = self.scripts.first().map(|s| s.name.clone());
        }
    }

    /// 请求一次网卡启停（二次确认）。
    fn request_toggle(&mut self, name: &str, op: Op) {
        if self.confirm.is_none() {
            self.confirm = Some(ConfirmReq::Toggle {
                name: name.to_string(),
                op,
                multi: false,
            });
        }
    }

    /// 请求一次批量启停（二次确认）。
    fn request_toggle_all(&mut self, op: Op) {
        if self.confirm.is_none() {
            self.confirm = Some(ConfirmReq::Toggle {
                name: String::new(),
                op,
                multi: true,
            });
        }
    }

    /// 请求删除脚本（二次确认）。
    fn request_delete_script(&mut self, name: &str) {
        if self.confirm.is_none() {
            self.confirm = Some(ConfirmReq::DeleteScript {
                name: name.to_string(),
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

    /// 真正执行删除脚本。
    fn do_delete_script(&mut self, name: &str) {
        match scripts::delete_script(name) {
            Ok(desc) => self.success(desc),
            Err(e) => self.failure(e),
        }
        self.load_scripts();
    }

    /// 运行指定脚本（由名称查找）。
    fn run_script_by_name(&mut self, name: &str) {
        let Some(item) = self.scripts.iter().find(|s| s.name == name).cloned() else {
            self.failure(format!("脚本 {} 不存在（已删除？）", name));
            self.load_scripts();
            return;
        };
        self.logs.push(Sev::Info, format!("执行脚本 {}…", item.name));
        match scripts::run_script(&item) {
            Ok(desc) => self.success(desc),
            Err(e) => self.failure(e),
        }
        // 脚本可能修改了路由/网络状态，刷新网卡列表
        self.refresh();
    }

    /// 「路由刷新」：运行当前下拉选中的脚本。
    fn route_refresh(&mut self) {
        let name = self.selected_script.clone();
        match name {
            Some(n) => self.run_script_by_name(&n),
            None => {
                self.logs
                    .push(Sev::Warn, "没有可用的脚本，请先在「脚本管理」中添加。".to_string());
            }
        }
    }

    /// 打开「添加脚本」弹窗（带模板预填）。
    fn open_add_script(&mut self) {
        if self.add_script.is_none() {
            self.add_script = Some(AddScriptDlg {
                name: String::new(),
                content: "# 路由刷新脚本模板 —— 请按需编辑（每行一条命令）\n# 常用示例（去掉 # 启用）：\n#   /usr/sbin/dscacheutil -flushcache          # 刷新 DNS 缓存（需 root）\n#   /usr/bin/killall -HUP mDNSResponder        # 刷新 mDNS（需 root）\n#   /sbin/route -n flush                       # 清空路由表（危险，需 root）\necho \"路由刷新脚本已执行\"\n".to_string(),
                needs_root: false,
            });
        }
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
            .show(ctx, |ui| match req {
                ConfirmReq::Toggle { name, op, multi } => {
                    ui.label(if *multi {
                        format!(
                            "确定要对当前显示的全部接口执行「{}」吗？",
                            op.action_label()
                        )
                    } else {
                        format!("确定要「{}」接口 {} 吗？", op.action_label(), name)
                    });
                    if *op == Op::Stop {
                        ui.colored_label(RED, "警告：关闭将断开该接口的网络连接。");
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::plain_button("取消")).clicked() {
                            decided = Some(false);
                        }
                        if *op == Op::Stop {
                            if ui.add(theme::danger_button("确认")).clicked() {
                                decided = Some(true);
                            }
                        } else if ui.add(theme::primary_button("确认")).clicked() {
                            decided = Some(true);
                        }
                    });
                }
                ConfirmReq::DeleteScript { name } => {
                    ui.label(format!("确定要删除脚本 {} 吗？", name));
                    ui.colored_label(RED, "删除后不可恢复。");
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::plain_button("取消")).clicked() {
                            decided = Some(false);
                        }
                        if ui.add(theme::danger_button("删除")).clicked() {
                            decided = Some(true);
                        }
                    });
                }
            });

        if let Some(ok) = decided {
            let req = self.confirm.take().unwrap();
            if ok {
                match req {
                    ConfirmReq::Toggle { name, op, multi } => {
                        if multi {
                            self.do_toggle_all(op);
                        } else {
                            self.do_toggle(&name, op);
                        }
                    }
                    ConfirmReq::DeleteScript { name } => self.do_delete_script(&name),
                }
            } else {
                let label = match &req {
                    ConfirmReq::Toggle { op, .. } => op.action_label().to_string(),
                    ConfirmReq::DeleteScript { name } => format!("删除脚本 {}", name),
                };
                self.logs.push(Sev::Info, format!("已取消{}操作", label));
            }
            return true;
        }
        false
    }

    fn show_add_script_window(&mut self, ctx: &egui::Context) {
        let Some(dlg) = self.add_script.as_mut() else {
            return;
        };
        let mut save: Option<bool> = None;
        let name_valid = !dlg.name.trim().is_empty() && !dlg.content.trim().is_empty();
        egui::Window::new("添加脚本")
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("脚本名：");
                    ui.add(egui::TextEdit::singleline(&mut dlg.name).desired_width(200.0));
                    ui.label(format!(".{}", if cfg!(target_os = "windows") { "bat" } else { "sh" }));
                });
                ui.add_space(4.0);
                ui.label("脚本内容（执行时以 sh 运行）：");
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut dlg.content)
                                .desired_rows(9)
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    });
                ui.checkbox(&mut dlg.needs_root, "需要管理员权限（root）执行");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::plain_button("取消")).clicked() {
                        save = Some(false);
                    }
                    if ui
                        .add_enabled(name_valid, theme::primary_button("保存"))
                        .clicked()
                    {
                        save = Some(true);
                    }
                });
            });

        if let Some(ok) = save {
            let dlg = self.add_script.take().unwrap();
            if ok {
                match scripts::add_script(&dlg.name, &dlg.content, dlg.needs_root) {
                    Ok(desc) => self.success(desc),
                    Err(e) => self.failure(e),
                }
            } else {
                self.logs.push(Sev::Info, "已取消添加脚本".to_string());
            }
            self.load_scripts();
        }
    }

    fn show_view_script_window(&mut self, ctx: &egui::Context) {
        let Some((name, content)) = self.view_script.clone() else {
            return;
        };
        let mut close = false;
        egui::Window::new(format!("查看脚本 {}", name))
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut content.as_str())
                                .code_editor()
                                .desired_rows(12)
                                .interactive(false),
                        );
                    });
                ui.add_space(6.0);
                if ui.add(theme::plain_button("关闭")).clicked() {
                    close = true;
                }
            });
        if close {
            self.view_script = None;
        }
    }

    /// 网卡管理 Tab。
    fn ui_net(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("刷新")).clicked() {
                self.refresh();
                self.logs.push(Sev::Info, "手动刷新网卡列表".to_string());
            }
            ui.separator();
            ui.checkbox(&mut self.show_all, "显示全部接口（含虚拟）");
            ui.separator();
            if self.show_all {
                if ui.add(theme::green_button("启动全部(显示中)")).clicked() {
                    self.request_toggle_all(Op::Start);
                }
                if ui.add(theme::danger_button("关闭全部(显示中)")).clicked() {
                    self.request_toggle_all(Op::Stop);
                }
            }
        });
        ui.add_space(4.0);

        let shown = self.shown_adapters();
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
                                ui.colored_label(TEXT_SUB, "虚拟");
                            }
                            ui.label(&a.friendly);

                            let (label, color) = match a.status_label() {
                                "已连接" => ("已连接", GREEN),
                                "up" => ("up", UP_BLUE),
                                _ => ("down", RED),
                            };
                            ui.colored_label(color, label);

                            match a.ipv4 {
                                Some(ip) => ui.monospace(ip.to_string()),
                                None => ui.colored_label(TEXT_SUB, "-"),
                            };
                            match a.netmask_v4 {
                                Some(m) => ui.monospace(m.to_string()),
                                None => ui.colored_label(TEXT_SUB, "-"),
                            };

                            let name = a.name.clone();
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(a.is_physical, theme::green_button("启动"))
                                    .clicked()
                                {
                                    let n = name.clone();
                                    self.request_toggle(&n, Op::Start);
                                }
                                if ui
                                    .add_enabled(a.is_physical, theme::danger_button("关闭"))
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
    }

    /// 路由与脚本 Tab。
    fn ui_route(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);

        // —— 路由刷新区 ——
        ui.strong("路由刷新");
        ui.label("通过执行下方选中的管理脚本来刷新路由/网络状态；脚本可继续在下方维护。");
        ui.add_space(4.0);

        let mut sel = self.selected_script.clone();
        let ready = !self.scripts.is_empty() && sel.is_some();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("route_script")
                .width(260.0)
                .selected_text(sel.clone().unwrap_or_else(|| "选择脚本…".to_string()))
                .show_ui(ui, |ui| {
                    for s in &self.scripts {
                        ui.selectable_value(&mut sel, Some(s.name.clone()), &s.name);
                    }
                });
            // 注意：先完成下拉交互，再借 self 执行。
            if ui
                .add_enabled(ready, theme::primary_button("刷新路由"))
                .clicked()
            {
                self.logs.push(Sev::Info, "触发路由刷新".to_string());
                self.route_refresh();
            }
        });
        if !ready {
            ui.colored_label(ORANGE, "暂无可执行脚本，请先在下方「脚本管理」中添加。");
        }
        self.selected_script = sel;

        ui.add_space(8.0);
        ui.separator();

        // —— 脚本管理区 ——
        ui.horizontal(|ui| {
            ui.strong("脚本管理");
            ui.add_space(8.0);
            if ui.add(theme::primary_button("添加脚本")).clicked() {
                self.open_add_script();
            }
        });
        ui.add_space(4.0);

        if self.scripts.is_empty() {
            ui.colored_label(TEXT_SUB, "（暂无脚本）");
        } else {
            // 克隆副本再迭代：渲染期间需要 &mut self（运行/删除），避免与 self.scripts 借用冲突。
            let items = self.scripts.clone();
            egui::Grid::new("scripts")
                .striped(true)
                .min_col_width(60.0)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.strong("名称");
                    ui.strong("需要 root");
                    ui.strong("路径");
                    ui.strong("操作");
                    ui.end_row();

                    for s in &items {
                        ui.monospace(&s.name);
                        if s.needs_root {
                            ui.colored_label(ORANGE, "是");
                        } else {
                            ui.colored_label(TEXT_SUB, "否");
                        }
                        ui.monospace(s.path.display().to_string());

                        let name = s.name.clone();
                        ui.horizontal(|ui| {
                            if ui.add(theme::plain_button("查看")).clicked() {
                                let content = scripts::script_content(s);
                                self.view_script = Some((name.clone(), content));
                            }
                            if ui.add(theme::primary_button("运行")).clicked() {
                                let n = name.clone();
                                self.run_script_by_name(&n);
                            }
                            if ui.add(theme::danger_button("删除")).clicked() {
                                let n = name.clone();
                                self.request_delete_script(&n);
                            }
                        });
                        ui.end_row();
                    }
                });
        }
        ui.add_space(4.0);
    }
}

impl eframe::App for NetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 每 2s 自动刷新网卡列表状态
        if self.last_refresh.elapsed() >= Duration::from_secs(2) {
            self.refresh();
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading("网卡管理器");
                ui.separator();
                ui.selectable_value(&mut self.tab, AppTab::Net, "网卡管理");
                ui.selectable_value(&mut self.tab, AppTab::Route, "路由与脚本");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_root_shim() {
                        ui.colored_label(ORANGE, "管理员模式");
                    } else {
                        ui.colored_label(
                            TEXT_SUB,
                            "普通模式（启停/root 脚本可能需管理员权限）",
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
                ui.heading(format!(
                    "操作日志（{} 条，文件见 {}）",
                    self.logs.len(),
                    AppLogs::log_file().display()
                ));
                ui.add_space(2.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add_space(2.0);
                        for e in self.logs.latest_first() {
                            let color = match e.sev {
                                Sev::Info => TEXT_MAIN,
                                Sev::Warn => ORANGE,
                                Sev::Error => RED,
                            };
                            ui.horizontal(|ui| {
                                ui.monospace(
                                    egui::RichText::new(format!("[{}]", e.ts)).color(TEXT_SUB),
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
            match self.tab {
                AppTab::Net => self.ui_net(ui),
                AppTab::Route => self.ui_route(ui),
            }
        });

        // 弹窗最后渲染，避免交互冲突
        self.show_confirm_window(ctx);
        self.show_add_script_window(ctx);
        self.show_view_script_window(ctx);

        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

/// root 检测：与 netctl 共用的判断，置于界面以显示当前权限模式。
fn is_root_shim() -> bool {
    unsafe { libc::geteuid() == 0 }
}