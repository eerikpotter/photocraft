/// Browser-host contract: report unsaved work without exposing document contents.
pub fn publish_dirty_state(dirty: bool) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        let value = if dirty { "true" } else { "false" };
        if root.get_attribute("data-craft-dirty").as_deref() != Some(value) {
            let _ = root.set_attribute("data-craft-dirty", value);
        }
    }
}
