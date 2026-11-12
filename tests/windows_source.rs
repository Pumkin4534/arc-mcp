#![cfg(windows)]
use arc_mcp::evidence::fingerprint;
use std::fs;

#[test]
fn windows_unicode_spaces_and_source_changes_are_fingerprinted() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project with spaces é");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("source é.txt"), "first").unwrap();
    fs::write(root.join("SECRET.PEM"), "excluded fixture, not a real key").unwrap();
    let before = fingerprint(&root).unwrap();
    assert_eq!(before.files, 1);
    fs::write(root.join("source é.txt"), "second").unwrap();
    assert_ne!(before.sha256, fingerprint(&root).unwrap().sha256);
}

#[test]
fn windows_symlink_referent_content_is_not_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let external = temp.path().join("outside.txt");
    fs::write(&external, "outside one").unwrap();
    std::os::windows::fs::symlink_file(&external, root.join("link.txt")).unwrap();
    let before = fingerprint(&root).unwrap();
    fs::write(external, "outside two").unwrap();
    let after = fingerprint(&root).unwrap();
    assert_eq!(before.sha256, after.sha256);
    assert!(
        after
            .warnings
            .iter()
            .any(|warning| warning.contains("referent"))
    );
}
