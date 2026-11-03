use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_crabspot"))
}

#[test]
fn inspect_reports_java21_class() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/classes/fixtures/Fixture.class"
    );
    let output = binary().args(["inspect", fixture]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("fixtures/Fixture"), "{stdout}");
    assert!(stdout.contains("65"), "{stdout}");
}

#[test]
fn invalid_class_is_an_error() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/java/Fixture.java"
    );
    let output = binary().args(["inspect", source]).output().unwrap();
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn missing_file_is_an_error() {
    let absent = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/does-not-exist.class"
    );
    let output = binary().args(["inspect", absent]).output().unwrap();
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn unsupported_execution_is_an_error() {
    let output = binary().args(["run", "Example"]).output().unwrap();
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn incomplete_and_extra_arguments_are_errors() {
    for arguments in [
        vec![],
        vec!["inspect"],
        vec!["inspect", "one.class", "two.class"],
    ] {
        let output = binary().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
    }
}

#[test]
fn preview_requires_explicit_cli_option() {
    let mut bytes =
        include_bytes!("../../../tests/fixtures/classes/fixtures/Fixture.class").to_vec();
    bytes[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(scratch).unwrap();
    let path = scratch.join(format!("preview-{}.class", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    let rejected = binary().arg("inspect").arg(&path).output().unwrap();
    let accepted = binary()
        .args(["inspect", "--enable-preview"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("preview"));
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("65.65535 (preview)"));
}

#[test]
fn oversized_file_reports_resource_limit_before_parsing() {
    let scratch = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(scratch).unwrap();
    let path = scratch.join(format!("oversized-{}.class", std::process::id()));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let output = binary().arg("inspect").arg(&path).output().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("class-file bytes exceeds configured limit")
    );
}
