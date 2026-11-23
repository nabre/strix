// Modified by the Strix contributors (fork of WordCraft); see NOTICE.
//! Release and source Flatpaks must allow release checks and host update helpers.
fn finish_args(manifest: &str) -> Vec<&str> {
    manifest
        .split("finish-args:")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with(' ') || line.starts_with('#'))
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|arg| arg.trim_matches(['\'', '"']))
        .collect()
}

#[test]
fn flatpak_update_permissions() {
    let manifests = [
        include_str!("../../../packaging/linux/flatpak/io.github.nabre.strix.bundle.yml"),
        include_str!("../../../packaging/linux/flatpak/io.github.nabre.strix.yml"),
    ];
    let args = manifests.map(finish_args);
    assert_eq!(args[0], args[1], "source and release permissions must agree");
    for permissions in args {
        // Strix: release checks need the network. Upstream also grants
        // `--talk-name=org.freedesktop.Flatpak` so the shared updater can update the installation
        // through `flatpak-spawn --host`; that runs commands outside the sandbox, so Strix leaves
        // it to the owner (docs/upstream.md) and the Flatpak is updated by Flatpak itself.
        assert!(permissions.contains(&"--share=network"), "missing update permission: --share=network");
    }
}
