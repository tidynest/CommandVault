//! The built binary's command line. Nothing here opens a window.
use std::process::Command;

#[test]
fn version_help_and_an_unknown_argument() {
    let bin = env!("CARGO_BIN_EXE_command_vault");
    let version = Command::new(bin).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("CommandVault {}", env!("CARGO_PKG_VERSION"))
    );

    let help = Command::new(bin).arg("--help").output().unwrap();
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("Usage:") && text.contains("Data:"), "{text}");

    let bogus = Command::new(bin).arg("--bogus").output().unwrap();
    assert_eq!(bogus.status.code(), Some(2));
    assert!(bogus.stdout.is_empty());
    assert!(String::from_utf8_lossy(&bogus.stderr).contains("--bogus"));
}
