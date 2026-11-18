fn ic2_embedded_content(root: &std::path::Path, enabled: &std::collections::BTreeSet<String>, dev: &[std::path::PathBuf]) -> core::ops::ControlFlow<std::path::PathBuf> {
    static CONTENT: &[u8] = horizon_client::native_data!("ic2", "ic2-content.hmod");
    let dir = std::env::current_dir().unwrap_or_default().join(".horizon-cache");
    let file = dir.join("ic2-embedded-content.hmod");
    if dev.contains(&file) {
        return core::ops::ControlFlow::Continue(());
    }
    let same = std::fs::read(&file).is_ok_and(|on_disk| on_disk == CONTENT);
    if !same && std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, CONTENT)).is_err() {
        log::warn!("Horizon: IC2's content could not be written to {}", file.display());
        return core::ops::ControlFlow::Continue(());
    }
    let mut with = dev.to_vec();
    with.push(file);
    core::ops::ControlFlow::Break(content_root(root, enabled, &with))
}
