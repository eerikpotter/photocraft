//! Optional host-supplied menu requests. Storage providers stay outside the editor engine.
use crate::{PhotocraftApp, menus::MenuItem};
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

pub(crate) fn insert_menu_items(app: &PhotocraftApp, items: &mut Vec<MenuItem>) {
    for c in &app.services.commands {
        if !c.id.starts_with("host.") || c.path.is_empty() || items.iter().any(|i| i.id == c.id) {
            continue;
        }
        let at =
            items.iter().position(|i| i.id == c.after).or_else(|| items.iter().rposition(|i| i.path.first().map(String::as_str) == c.path.first().copied()));
        items.insert(
            at.map_or(items.len(), |i| i + 1),
            MenuItem {
                id: c.id.into(),
                label: c.label.into(),
                path: c.path.iter().map(|s| (*s).into()).collect(),
                shortcut: None,
                enabled: is_enabled(app, c.id) == Some(true),
                checked: None,
                color: None,
            },
        );
    }
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
        assert!(!crate::menus::menu_items(&app).iter().any(|m| m.id == id));
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
        let items = crate::menus::menu_items(&app);
        let at = items.iter().position(|m| m.id == id).unwrap();
        assert_eq!(items[at - 1].id, "file.saveAs");
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
