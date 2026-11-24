//! One cloud section within File; the heading owns the provider's mark.
pub fn install(services: &mut photocraft_ui_egui::Services, request: impl Fn(Option<bool>) -> Result<(), String> + Clone + 'static) {
    let section = photocraft_ui_egui::service_commands::Section {
        id: "host.cloud",
        title: "Sovereign Cloud",
        icon: Some(egui::include_image!("../../assets/icp/logo.svg")),
    };
    for (id, label, after, copy) in [
        ("host.cloud.save", "Save", "file.revert", Some(false)),
        ("host.cloud.save_copy", "Save As…", "host.cloud.save", Some(true)),
        ("host.cloud.projects", "Open…", "host.cloud.save_copy", None),
    ] {
        let request = request.clone();
        services.commands.push(photocraft_ui_egui::service_commands::Command {
            id,
            label,
            path: &["File"],
            after,
            requires_document: copy.is_some(),
            enabled: true,
            section: Some(section.clone()),
            request: Box::new(move || request(copy)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use photocraft_ui_egui::{PhotocraftApp, menus};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn cloud_section_keeps_order_document_guards_and_menu_activation() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let received = calls.clone();
        let mut services = photocraft_ui_egui::Services::default();
        install(&mut services, move |copy| {
            received.borrow_mut().push(copy);
            Ok(())
        });
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        // Exercise the browser's in-window menu even when the test runs on macOS.
        craft::ui::testing::windows::set_os(app.ctx(), craft::host::target::Os::Linux);
        let items = menus::menu_rows(&app);
        for (id, parent, enabled) in [
            ("host.cloud.save", "file.revert", false),
            ("host.cloud.save_copy", "host.cloud.save", false),
            ("host.cloud.projects", "host.cloud.save_copy", true),
        ] {
            let at = items.iter().position(|item| item.command.id == id).unwrap();
            assert_eq!(items[at - 1].command.id, parent);
            assert_eq!(items[at].enabled, enabled);
        }
        app.run("file.new", Default::default()).unwrap();
        let mut h = Harness::builder().with_context(app.ctx().clone()).with_size(egui::vec2(1280.0, 900.0)).with_max_steps(64).build_eframe(move |_| app);
        h.run_steps(3);
        for label in ["Save", "Save As…", "Open…"] {
            h.get_by_label("File").click();
            h.run_steps(3);
            h.get_by_label("Sovereign Cloud").click();
            h.run_steps(3);
            // The parent File menu also has local Save/Open items; the last
            // matching node belongs to the newly opened cloud submenu.
            h.query_all_by_label(label).last().unwrap().click();
            h.run_steps(3);
        }
        assert_eq!(*calls.borrow(), vec![Some(false), Some(true), None]);

        // The upstream submenu owns keyboard navigation and queues exactly one action.
        h.get_by_label("File").click();
        h.run_steps(3);
        let submenu = h.get_by_label("Sovereign Cloud").rect();
        h.hover_at(submenu.center());
        h.run_steps(3);
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(3);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(*calls.borrow(), vec![Some(false), Some(true), None, Some(false)]);

        // Hiding the first action must keep exactly one heading on the remaining block.
        h.state_mut().session.edit_prefs(|p| p.menus.hidden.push("host.cloud.save".into()));
        h.get_by_label("File").click();
        h.run_steps(3);
        assert_eq!(h.query_all_by_label("Sovereign Cloud").count(), 1);
    }
}
