//! Optional host-supplied menu requests. Storage providers stay outside the editor engine.
use crate::PhotocraftApp;
use craft::menu::Row;
use serde_json::{Value, json};

/// A non-interactive heading shared by consecutive host commands in the same menu.
#[derive(Clone)]
pub struct Section {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: Option<egui::ImageSource<'static>>,
}

/// An interactive host request, not an engine editing operation. IDs use the `host.` namespace
/// so a provider cannot replace an existing editor command. The callback queues work; it must
/// not report an asynchronous operation as completed. Hosts register nothing by default.
pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    pub path: &'static [&'static str],
    pub after: &'static str,
    pub requires_document: bool,
    pub enabled: bool,
    /// Optional host-provided grouping. The shared editor does not know the storage provider.
    pub section: Option<Section>,
    pub request: Box<dyn Fn() -> Result<(), String>>,
}

fn find<'a>(app: &'a PhotocraftApp, id: &str) -> Option<&'a Command> {
    id.starts_with("host.").then(|| app.services.commands.iter().find(|c| c.id == id)).flatten()
}

pub(crate) fn is_enabled(app: &PhotocraftApp, id: &str) -> Option<bool> {
    find(app, id).map(|c| c.enabled && (!c.requires_document || app.session.active().is_some()))
}

pub(crate) fn invoke(app: &PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    let c = find(app, id)?;
    Some((|| {
        // Remote services need their own capability model before automation can use them.
        if app.automation_input {
            return Err("Host services require an interactive request".into());
        }
        if is_enabled(app, id) != Some(true) {
            return Err("Host command is currently unavailable".into());
        }
        (c.request)()?;
        Ok(json!({"queued": id}))
    })())
}

/// Host sections use the upstream menu model's ordinary submenus.
pub(crate) fn insert_menu_items(app: &PhotocraftApp, items: &mut Vec<Row>) {
    for c in &app.services.commands {
        if !c.id.starts_with("host.") || c.path.is_empty() || items.iter().any(|i| i.command.id == c.id) {
            continue;
        }
        let at = items.iter().position(|i| i.command.id == c.after)
            .or_else(|| items.iter().rposition(|i| i.path.first().map(String::as_str) == c.path.first().copied()));
        let mut path = c.path.to_vec();
        if let Some(section) = &c.section { path.push(section.title); }
        items.insert(at.map_or(items.len(), |i| i + 1), Row::new(&path, c.id, c.label).enabled(is_enabled(app, c.id) == Some(true)));
    }
}

pub(crate) fn source_states(app: &PhotocraftApp, name: &'static str) -> std::sync::Arc<crate::app::framework::SourceStates> {
    let facts: Vec<(&str, bool)> = app.services.commands.iter().filter(|c| c.id.starts_with("host."))
        .map(|c| (c.id, is_enabled(app, c.id) == Some(true))).collect();
    crate::app::framework::derived_states(app, name, &facts, || facts.iter().map(|(id, on)|
        (craft::keymap::CommandRef::new(*id), craft::ui::runtime::commands::CommandState::new().enabled(*on))).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn host_command_is_optional_ordered_guarded_and_queues_once() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let ctx = egui::Context::default();
        let id = "host.test.save";
        assert!(!crate::menus::menu_rows(&app).iter().any(|m| m.command.id == id));
        let calls = Rc::new(Cell::new(0));
        let received = calls.clone();
        app.services.commands.push(Command {
            id,
            label: "Remote Save…",
            path: &["File"],
            after: "file.saveAs",
            requires_document: true,
            enabled: true,
            section: None,
            request: Box::new(move || {
                received.set(received.get() + 1);
                Ok(())
            }),
        });
        let items = crate::menus::menu_rows(&app);
        let at = items.iter().position(|m| m.command.id == id).unwrap();
        assert_eq!(items[at - 1].command.id, "file.saveAs");
        assert!(!items[at].enabled);
        assert!(crate::menus::invoke(&mut app, &ctx, id, json!({})).is_err());
        app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
        assert_eq!(crate::menus::invoke(&mut app, &ctx, id, json!({})).unwrap(), json!({"queued": id}));
        assert_eq!(calls.get(), 1);
        app.automation_input = true;
        assert!(crate::menus::invoke(&mut app, &ctx, id, json!({})).is_err());
        app.automation_input = false;
        app.services.commands[0].enabled = false;
        assert!(crate::menus::invoke(&mut app, &ctx, id, json!({})).is_err());
        assert_eq!(calls.get(), 1);
        app.services.commands[0].id = "file.save";
        assert!(find(&app, "file.save").is_none(), "host commands cannot shadow editor commands");
    }
}
