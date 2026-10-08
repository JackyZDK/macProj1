//! 全局主题：移动 App 风格配色 + 统一字体大小。
//! 主色/语义色参照 iOS 系统色板，浅色明亮风。

use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

// —— 语义色板（参照 iOS 系统色）——
pub const PRIMARY: Color32 = Color32::from_rgb(0x00, 0x7A, 0xFF); // 主色（iOS 系统蓝）
pub const GREEN: Color32 = Color32::from_rgb(0x34, 0xC7, 0x59); // 成功/已连接
pub const ORANGE: Color32 = Color32::from_rgb(0xFF, 0x95, 0x00); // 警告/需注意
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x3B, 0x30); // 错误/危险
pub const UP_BLUE: Color32 = Color32::from_rgb(0x00, 0x8A, 0xC7); // up/信息
pub const TEXT_MAIN: Color32 = Color32::from_rgb(0x1C, 0x1C, 0x1E); // 正文深灰
pub const TEXT_SUB: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x93); // 次要文字
pub const BG_WINDOW: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF7); // 窗口背景（iOS 系统灰）
pub const BG_CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF); // 卡片/白
pub const BORDER: Color32 = Color32::from_rgb(0xE5, 0xE5, 0xEA); // 浅描边

/// 统一字体大小并应用浅色移动端视觉。
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    // 全界面字号统一：正文/按钮 14，表格等宽 13，小字 12，仅标题略大 16。
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(16.0)),
        (TextStyle::Body, FontId::proportional(14.0)),
        (TextStyle::Button, FontId::proportional(14.0)),
        (TextStyle::Small, FontId::proportional(12.0)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
    ]
    .into();
    ctx.set_style(style);

    let mut v = egui::Visuals::light();
    v.panel_fill = BG_WINDOW;
    v.window_fill = BG_CARD;
    v.extreme_bg_color = BG_CARD;
    v.faint_bg_color = Color32::from_rgb(0xF7, 0xF7, 0xFA);
    v.override_text_color = Some(TEXT_MAIN);
    v.selection.bg_fill = PRIMARY;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = PRIMARY;

    // 控件：白底 + 浅描边 + 统一圆角（移动 App 圆润感）
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.rounding = egui::Rounding::same(6.0);
    }
    v.widgets.inactive.bg_fill = BG_CARD;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_MAIN);
    v.widgets.inactive.weak_bg_fill = BG_WINDOW;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xE9, 0xF0, 0xFE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_MAIN);
    v.widgets.active.bg_fill = Color32::from_rgb(0xD6, 0xE6, 0xFD);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, PRIMARY);
    ctx.set_visuals(v);
}

/// 实心主色按钮（主要操作，如：刷新路由/添加脚本），白字。
pub fn primary_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE))
        .fill(PRIMARY)
        .stroke(Stroke::NONE)
}

/// 实心绿色按钮（启动类），白字。
pub fn green_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE))
        .fill(GREEN)
        .stroke(Stroke::NONE)
}

/// 实心红色按钮（危险/关闭/删除），白字。
pub fn danger_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(Color32::WHITE))
        .fill(RED)
        .stroke(Stroke::NONE)
}

/// 白底描边按钮（次要操作，如：查看/取消），主色字。
pub fn plain_button(text: &'static str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(PRIMARY))
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
}