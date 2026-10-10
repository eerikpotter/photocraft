//! Cloud counterparts beside the editor's ordinary Open, Save and Save As actions.
pub fn install(services: &mut photocraft_ui_egui::Services, request: impl Fn(Option<bool>) -> Result<(), String> + Clone + 'static) {
    for (id, label, after, copy) in [
        ("host.cloud.projects", "Open from Cloud…", "file.open", None),
        ("host.cloud.save", "Save to Cloud…", "file.save", Some(false)),
        ("host.cloud.save_copy", "Save As to Cloud…", "file.saveAs", Some(true)),
    ] {
        let request = request.clone();
        services.commands.push(photocraft_ui_egui::service_commands::Command {
            id,
            label,
            path: &["File"],
            after,
            requires_document: copy.is_some(),
            enabled: true,
            icon: Some(egui::include_image!("../../assets/icp/logo.svg")),
            request: Box::new(move || request(copy)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use photocraft_ui_egui::{PhotocraftApp, menus, theme::ThemeKind};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn cloud_counterparts_keep_order_document_guards_and_menu_activation() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let received = calls.clone();
        let mut services = photocraft_ui_egui::Services::default();
        install(&mut services, move |copy| {
            received.borrow_mut().push(copy);
            Ok(())
        });
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        let items = menus::menu_items(&app);
        for (id, parent, enabled) in
            [("host.cloud.projects", "file.open", true), ("host.cloud.save", "file.save", false), ("host.cloud.save_copy", "file.saveAs", false)]
        {
            let at = items.iter().position(|item| item.id == id).unwrap();
            assert_eq!(items[at - 1].id, parent);
            assert_eq!(items[at].enabled, enabled);
        }
        app.run("file.new", Default::default()).unwrap();
        let mut h = Harness::builder().with_size(egui::vec2(760.0, 900.0)).build_ui_state(
            |ui, app| {
                menus::menu_bar(app, ui);
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, ThemeKind::Pro);
        for label in ["Open from Cloud…", "Save to Cloud…", "Save As to Cloud…"] {
            h.get_by_label("File").click();
            h.run_steps(3);
            h.get_by_label(label).click();
            h.run_steps(3);
        }
        assert_eq!(*calls.borrow(), vec![None, Some(false), Some(true)]);
    }
}
