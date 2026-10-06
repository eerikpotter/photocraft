//! Platform-independent cloud controls, shared by the browser and native UI tests.
use photocraft_ui_egui::theme::{ThemeKind, Tokens};

pub fn palette(ctx: &egui::Context) -> Tokens {
    Tokens::for_kind(if Tokens::get(ctx).dark() { ThemeKind::Studio } else { ThemeKind::StudioLight })
}

/// Scope the cloud's visual identity to this UI. The editor's theme is untouched.
pub fn style(ui: &mut egui::Ui) {
    let t = palette(ui.ctx());
    let s = ui.style_mut();
    s.spacing.item_spacing = egui::vec2(10.0, 8.0);
    s.visuals = if t.dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    s.visuals.override_text_color = Some(t.text);
    s.visuals.weak_text_color = Some(t.text_dim);
    s.visuals.panel_fill = t.card;
    s.visuals.window_fill = t.card;
    s.visuals.extreme_bg_color = t.field;
    s.visuals.selection.bg_fill = t.accent_soft;
    s.visuals.selection.stroke = egui::Stroke::new(1.0, t.accent_text);
    s.visuals.widgets.inactive.bg_fill = t.field;
    s.visuals.widgets.inactive.weak_bg_fill = t.field;
    s.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, t.field_border);
    s.visuals.widgets.hovered.bg_fill = t.hover;
    s.visuals.widgets.hovered.weak_bg_fill = t.hover;
    s.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, t.accent_border);
    s.visuals.widgets.active.bg_fill = t.pressed;
    s.visuals.widgets.active.weak_bg_fill = t.pressed;
    for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active] {
        w.corner_radius = egui::CornerRadius::same(t.radius_sm as u8);
    }
}

pub fn section(ui: &egui::Ui) -> egui::Frame {
    let t = palette(ui.ctx());
    egui::Frame::new().fill(t.card).stroke(egui::Stroke::new(1.0, t.card_border)).corner_radius(t.radius_lg).inner_margin(16.0)
}

pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    // Blue for the action, violet for selected surfaces, both from the existing design tokens.
    let blue = Tokens::for_kind(ThemeKind::Pro);
    ui.add(egui::Button::new(egui::RichText::new(label).color(blue.primary_text)).fill(blue.accent).min_size(egui::vec2(128.0, 34.0)))
}

pub fn header(ui: &mut egui::Ui) {
    let t = palette(ui.ctx());
    egui::Frame::new().fill(t.accent_soft).corner_radius(t.radius_lg).inner_margin(16.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(egui::include_image!("../../assets/icp/logo.svg")).fit_to_exact_size(egui::vec2(40.0, 24.0)));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Sovereign Cloud").size(22.0).strong());
                ui.label(egui::RichText::new("Your files. Your identity.").color(t.accent_text));
            });
        });
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SaveAction {
    Save,
    Cancel,
}

pub fn file_name(name: &str) -> String {
    let name = name.trim();
    if name.to_ascii_lowercase().ends_with(".pcraft") { name.into() } else { format!("{name}.pcraft") }
}
pub fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 { format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0)) } else { format!("{} KiB", bytes.div_ceil(1024)) }
}

/// First-save/copy naming dialog, invoked only by a File-menu request.
pub fn save_form(ui: &mut egui::Ui, name: &mut String, copy: bool, enabled: bool) -> Option<SaveAction> {
    let mut action = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.heading(if copy { "Save a copy to Cloud" } else { "Save to Cloud" });
        ui.weak(if copy { "Give the new copy a name." } else { "Name this file for your cloud storage." });
        ui.add_space(14.0);
        ui.label("File name");
        ui.add(egui::TextEdit::singleline(name).hint_text("Untitled").desired_width(f32::INFINITY));
        if name.trim().len() > 160 {
            ui.colored_label(palette(ui.ctx()).warning, "Choose a shorter file name (160 bytes maximum).");
        }
        ui.add_space(12.0);
        let valid = !name.trim().is_empty() && name.trim().len() <= 160;
        ui.horizontal(|ui| {
            if ui.add_enabled_ui(valid, |ui| primary(ui, if copy { "Save copy" } else { "Save" })).inner.clicked() {
                action = Some(SaveAction::Save);
            }
            if ui.button("Cancel").clicked() {
                action = Some(SaveAction::Cancel);
            }
        });
    });
    action
}

pub fn transfer(ui: &mut egui::Ui, message: &str, progress: Option<f32>) {
    let t = palette(ui.ctx());
    section(ui).fill(t.accent_soft).stroke(egui::Stroke::new(1.0, t.accent_border)).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(20.0).color(t.accent));
            ui.strong(message);
        });
        if let Some(fraction) = progress {
            ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).show_percentage().desired_width(ui.available_width()));
        }
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

/// A labelled notification, never a file card. Returns true on dismiss.
pub fn notification(ui: &mut egui::Ui, message: &str, error: bool) -> bool {
    let t = palette(ui.ctx());
    let mut dismiss = false;
    ui.separator();
    ui.horizontal(|ui| {
        ui.colored_label(if error { t.warning } else { t.accent }, if error { "Needs attention" } else { "Activity" });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            dismiss = ui.small_button("Dismiss").clicked();
        });
    });
    ui.label(message);
    dismiss
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RevisionAction {
    Open,
    Delete,
}
pub fn revision_row(ui: &mut egui::Ui, timestamp: &str, bytes: u64, latest: bool) -> Option<RevisionAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(timestamp);
            ui.weak(format!("{}{}", if latest { "Latest version · " } else { "" }, size_label(bytes)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("…", |ui| {
                if ui.button("Delete version…").clicked() {
                    action = Some(RevisionAction::Delete);
                    ui.close();
                }
            });
            if ui.button("Open").clicked() {
                action = Some(RevisionAction::Open);
            }
        });
    });
    action
}

pub fn file_summary(ui: &mut egui::Ui, name: &str, timestamp: &str, bytes: u64) -> bool {
    ui.strong(file_name(name));
    ui.horizontal_wrapped(|ui| {
        ui.weak(format!("Saved {timestamp} · {}", size_label(bytes)));
    });
    ui.button("Open file").clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use photocraft_ui_egui::PhotocraftApp;

    #[test]
    fn naming_dialog_confirms_once_and_can_be_cancelled() {
        for copy in [false, true] {
            let mut h = Harness::builder().with_size(egui::vec2(460.0, 280.0)).build_ui_state(
                |ui, action: &mut Option<SaveAction>| {
                    if let Some(a) = save_form(ui, &mut "Poster".to_string(), copy, true) {
                        *action = Some(a);
                    }
                },
                None,
            );
            h.get_by_label(if copy { "Save copy" } else { "Save" }).click();
            h.run_steps(3);
            assert_eq!(*h.state(), Some(SaveAction::Save));
            h.get_by_label("Cancel").click();
            h.run_steps(3);
            assert_eq!(*h.state(), Some(SaveAction::Cancel));
        }
    }

    #[test]
    fn saving_and_invalid_names_cannot_start_another_save() {
        for (name, enabled) in [("Poster", false), ("   ", true), (&"a".repeat(161), true)] {
            let mut h = Harness::builder().build_ui_state(
                |ui, action: &mut Option<SaveAction>| {
                    *action = save_form(ui, &mut name.to_string(), false, enabled);
                },
                None,
            );
            h.get_by_label("Save").click();
            h.run_steps(3);
            assert!(h.state().is_none());
        }
    }

    #[test]
    fn cloud_views_render_in_all_themes() {
        let output = std::env::var("PHOTOCRAFT_CLOUD_PREVIEWS").ok();
        for (index, theme) in ThemeKind::ALL.into_iter().enumerate() {
            for width in [360.0, 520.0] {
                for view in ["files", "saving"] {
                    let builder = Harness::builder().with_size(egui::vec2(width, 560.0));
                    let builder = if output.is_some() { builder.wgpu() } else { builder };
                    let mut h = builder.build_ui(|ui| {
                        style(ui);
                        header(ui);
                        ui.add_space(8.0);
                        section(ui).show(ui, |ui| {
                            if view == "files" {
                                ui.heading("My files");
                                ui.weak("1 file · Times shown in your local timezone");
                                ui.add_space(12.0);
                                file_summary(ui, "Poster", "07/10/2026, 01:25:00", 3 * 1024 * 1024);
                                ui.separator();
                                ui.label("Version history");
                                revision_row(ui, "07/10/2026, 01:25:00", 3 * 1024 * 1024, true);
                            } else {
                                ui.heading("My files");
                                ui.weak("Your saved files will appear here.");
                            }
                        });
                        ui.add_space(12.0);
                        if view == "saving" {
                            transfer(ui, "Uploading your file…", Some(0.625));
                        } else {
                            notification(ui, "Opened Poster.pcraft · saved 07/10/2026, 01:25:00", false);
                        }
                    });
                    PhotocraftApp::setup_context(&h.ctx, theme);
                    h.run_steps(3);
                    if view == "files" {
                        assert!(h.query_by_label("Open file").is_some());
                        assert!(h.query_by_label("07/10/2026, 01:25:00").is_some());
                    } else {
                        assert!(h.query_by_label("Save").is_none());
                        assert!(h.query_by_label("Save file").is_none());
                    }
                    if let Some(dir) = &output {
                        h.render().unwrap().save(format!("{dir}/cloud-{view}-{index}-{width}.png")).unwrap();
                    }
                }
            }
        }
    }

    #[test]
    fn file_names_do_not_duplicate_native_extension() {
        assert_eq!(file_name(" Poster "), "Poster.pcraft");
        assert_eq!(file_name("Poster.pcraft"), "Poster.pcraft");
        assert_eq!(file_name("Poster.PCRAFT"), "Poster.PCRAFT");
    }
}
