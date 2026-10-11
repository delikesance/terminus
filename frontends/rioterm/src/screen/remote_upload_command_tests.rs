use super::*;

#[test]
fn posix_dir_defaults_to_tmp_and_drops_a_trailing_slash() {
    assert_eq!(posix_upload_dir(None), Ok("/tmp".to_string()));
    assert_eq!(
        posix_upload_dir(Some("/var/tmp/")),
        Ok("/var/tmp".to_string())
    );
}

#[test]
fn posix_dir_refuses_control_characters() {
    assert!(posix_upload_dir(Some("/srv/a\nb")).is_err());
    assert_eq!(posix_upload_dir(Some("/")), Ok("/".to_string()));
    assert_eq!(posix_remote_path("/", "a.png"), "/a.png");
}

#[test]
fn posix_command_creates_the_dir_then_writes_private_to_the_path() {
    assert_eq!(
        posix_upload_command("/var/tmp/new dir", "/var/tmp/new dir/a.png"),
        r"sh -c 'umask 077 && mkdir -p '\''/var/tmp/new dir'\'' && cat > '\''/var/tmp/new dir/a.png'\'''"
    );
    assert_eq!(
        posix_upload_command("/srv", "/srv/a.png"),
        "sh -c 'umask 077 && mkdir -p /srv && cat > /srv/a.png'"
    );
}

#[test]
fn posix_tilde_dir_is_expanded_by_the_remote_shell_not_quoted() {
    assert_eq!(
        posix_upload_command("~/drop", "~/drop/a.png"),
        r#"sh -c 'umask 077 && mkdir -p "$HOME"/drop && cat > "$HOME"/drop/a.png'"#
    );
    assert_eq!(posix_shell_word("~/my dir"), r#""$HOME"/'my dir'"#);
}

#[test]
fn windows_dir_defaults_to_the_host_temp_and_is_created() {
    let command = windows_upload_command(None, "x.txt").expect("default dir");
    assert!(command.contains("$d = $env:TEMP;"), "{command}");
    assert!(
        command.contains("New-Item -ItemType Directory -Force -Path $d | Out-Null;"),
        "{command}"
    );
    assert!(command.contains("$f = Join-Path $d 'x.txt';"), "{command}");
    assert!(command.ends_with("; $f\""), "{command}");
}

#[test]
fn windows_configured_dir_doubles_single_quotes() {
    let command = windows_upload_command(Some(r"D:\O'Drop"), "x.txt").expect("dir");
    assert!(command.contains(r"$d = 'D:\O''Drop';"), "{command}");
}

#[test]
fn windows_dir_with_a_double_quote_or_control_char_is_refused() {
    assert!(windows_upload_command(Some(r#"D:\"x"#), "x.txt").is_err());
    assert!(windows_upload_command(Some("D:\\a\tb"), "x.txt").is_err());
    assert!(windows_upload_command(Some(r"%USERPROFILE%\drop"), "x.txt").is_err());
}

#[test]
fn upload_reuses_tab_args_and_writes_into_the_dir() {
    let tab = vec!["-p".to_string(), "2222".to_string(), "me@box".to_string()];
    let path = posix_remote_path("/tmp", "terminus-paste-1.png");
    assert_eq!(path, "/tmp/terminus-paste-1.png");
    assert_eq!(
        upload_args(tab, &posix_upload_command("/tmp", &path)),
        vec![
            "-T",
            "-o",
            "ConnectTimeout=10",
            "-p",
            "2222",
            "me@box",
            "sh -c 'umask 077 && mkdir -p /tmp && cat > /tmp/terminus-paste-1.png'",
        ]
    );
}
