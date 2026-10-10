use super::*;

/// Probe remote OS family, arch, temp dir, and archive tools.
pub(super) async fn probe_remote_env(conn: &SftpConnection) -> RemoteEnv {
    let script = concat!(
        "# terminus-sftp-probe-v2\n",
        "TAR=0; ZIP=0; UNZIP=0; PS=0\n",
        "command -v tar >/dev/null 2>&1 && TAR=1\n",
        "command -v zip >/dev/null 2>&1 && ZIP=1\n",
        "command -v unzip >/dev/null 2>&1 && UNZIP=1\n",
        "(command -v powershell.exe >/dev/null 2>&1 || command -v pwsh >/dev/null 2>&1 || command -v powershell >/dev/null 2>&1) && PS=1\n",
        "UNAME=$(uname -s 2>/dev/null || printf unknown)\n",
        "ARCH=$(uname -m 2>/dev/null || printf x86_64)\n",
        "FAMILY=unix\n",
        "case \"$UNAME\" in\n",
        "  MINGW*|MSYS*|CYGWIN*|Windows_NT|windows*) FAMILY=windows ;;\n",
        "esac\n",
        "if [ -n \"${WINDIR:-}${SYSTEMROOT:-}\" ]; then FAMILY=windows; fi\n",
        "TMP=${TMPDIR:-/tmp}\n",
        "if [ \"$FAMILY\" = windows ]; then\n",
        "  TMP=${TEMP:-${TMPDIR:-/tmp}}\n",
        "elif [ -d /tmp ] && [ -w /tmp ]; then\n",
        "  TMP=/tmp\n",
        "fi\n",
        "printf 'tar=%s zip=%s unzip=%s ps=%s family=%s arch=%s tmp=%s\\n' \\\n",
        "  \"$TAR\" \"$ZIP\" \"$UNZIP\" \"$PS\" \"$FAMILY\" \"$ARCH\" \"$TMP\"\n",
    );
    match conn.exec(script).await {
        Ok((0, stdout, _)) => parse_probe_stdout(&String::from_utf8_lossy(&stdout)),
        Ok((code, _, err)) => {
            debug!(
                code,
                stderr = %String::from_utf8_lossy(&err),
                "archive env probe non-zero"
            );
            RemoteEnv {
                tools: RemoteArchiveTools::none(),
                family: RemoteFamily::Unix,
                arch: "x86_64".into(),
                tmp: "/tmp".into(),
            }
        }
        Err(err) => {
            debug!(error = %err, "archive env probe failed");
            RemoteEnv {
                tools: RemoteArchiveTools::none(),
                family: RemoteFamily::Unix,
                arch: "x86_64".into(),
                tmp: "/tmp".into(),
            }
        }
    }
}

/// Pick native tar/zip/PowerShell; error if the remote has none.
pub(super) async fn acquire_pack_session(
    conn: &SftpConnection,
) -> Result<RemotePackSession, String> {
    let env = probe_remote_env(conn).await;

    if env.tools.tar {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::NativeTar,
            kind: ArchiveKind::TarGz,
        });
    }
    if env.tools.zip {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::NativeZip,
            kind: ArchiveKind::Zip,
        });
    }
    if env.family == RemoteFamily::Windows && env.tools.powershell {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::PowerShell,
            kind: ArchiveKind::Zip,
        });
    }

    Err(NO_REMOTE_ARCHIVE_TOOLS.into())
}

/// Acquire a session that can **extract** `kind` with native tools only.
pub(super) async fn acquire_extract_session(
    conn: &SftpConnection,
    kind: ArchiveKind,
) -> Result<RemotePackSession, String> {
    let env = probe_remote_env(conn).await;
    match kind {
        ArchiveKind::TarGz => {
            if env.tools.tar {
                Ok(RemotePackSession {
                    env,
                    engine: PackEngine::NativeTar,
                    kind,
                })
            } else {
                Err(NO_REMOTE_ARCHIVE_TOOLS.into())
            }
        }
        ArchiveKind::Zip => {
            if env.tools.unzip {
                return Ok(RemotePackSession {
                    env,
                    engine: PackEngine::NativeZip,
                    kind,
                });
            }
            if env.family == RemoteFamily::Windows && env.tools.powershell {
                return Ok(RemotePackSession {
                    env,
                    engine: PackEngine::PowerShell,
                    kind,
                });
            }
            Err(NO_REMOTE_ARCHIVE_TOOLS.into())
        }
    }
}

pub(super) fn archive_create_script(
    parent: &str,
    base: &str,
    name: &str,
    out: &str,
    session: &RemotePackSession,
) -> String {
    match session.engine {
        PackEngine::NativeTar => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=create\n",
                "TERMINUS_ARCHIVE_PARENT={parent}\n",
                "TERMINUS_ARCHIVE_BASE={base}\n",
                "TERMINUS_ARCHIVE_NAME={name}\n",
                "TERMINUS_ARCHIVE_OUT={out}\n",
                "set -e\n",
                "cd {parent}\n",
                "ROOT={base}\n",
                "CLEANUP=\n",
                "if [ {base} != {name} ]; then\n",
                "  ln -snf {base} {name}\n",
                "  ROOT={name}\n",
                "  CLEANUP=1\n",
                "fi\n",
                "tar -czhf {out} -h \"$ROOT\"\n",
                "if [ -n \"$CLEANUP\" ]; then rm -f {name}; fi\n",
                "printf '%s\\n' {out}\n",
            ),
            parent = sh_quote(parent),
            base = sh_quote(base),
            name = sh_quote(name),
            out = sh_quote(out),
        ),
        PackEngine::NativeZip => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=create\n",
                "TERMINUS_ARCHIVE_PARENT={parent}\n",
                "TERMINUS_ARCHIVE_BASE={base}\n",
                "TERMINUS_ARCHIVE_NAME={name}\n",
                "TERMINUS_ARCHIVE_OUT={out}\n",
                "set -e\n",
                "cd {parent}\n",
                "ROOT={base}\n",
                "CLEANUP=\n",
                "if [ {base} != {name} ]; then\n",
                "  ln -snf {base} {name}\n",
                "  ROOT={name}\n",
                "  CLEANUP=1\n",
                "fi\n",
                "zip -rq {out} \"$ROOT\"\n",
                "if [ -n \"$CLEANUP\" ]; then rm -f {name}; fi\n",
                "printf '%s\\n' {out}\n",
            ),
            parent = sh_quote(parent),
            base = sh_quote(base),
            name = sh_quote(name),
            out = sh_quote(out),
        ),
        PackEngine::PowerShell => {
            let src = if parent == "/" {
                format!("/{base}")
            } else if parent.contains('\\') {
                format!("{parent}\\{base}")
            } else {
                format!("{parent}/{base}")
            };
            // Compress-Archive uses the leaf folder name as zip root.
            format!(
                "powershell -NoProfile -Command \"Compress-Archive -Path {} -DestinationPath {} -Force\"",
                ps_quote(&src),
                ps_quote(out),
            )
        }
    }
}

pub(super) fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub(super) fn archive_extract_script(
    dest: &str,
    archive: &str,
    session: &RemotePackSession,
) -> String {
    match session.engine {
        PackEngine::NativeTar => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=extract\n",
                "TERMINUS_ARCHIVE_PARENT={dest}\n",
                "TERMINUS_ARCHIVE_BASE=\n",
                "TERMINUS_ARCHIVE_OUT={archive}\n",
                "set -e\n",
                "mkdir -p {dest}\n",
                "tar -C {dest} -xzf {archive}\n",
            ),
            dest = sh_quote(dest),
            archive = sh_quote(archive),
        ),
        PackEngine::NativeZip => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=extract\n",
                "TERMINUS_ARCHIVE_PARENT={dest}\n",
                "TERMINUS_ARCHIVE_BASE=\n",
                "TERMINUS_ARCHIVE_OUT={archive}\n",
                "set -e\n",
                "mkdir -p {dest}\n",
                "unzip -qo {archive} -d {dest}\n",
            ),
            dest = sh_quote(dest),
            archive = sh_quote(archive),
        ),
        PackEngine::PowerShell => format!(
            "powershell -NoProfile -Command \"Expand-Archive -Path {} -DestinationPath {} -Force\"",
            ps_quote(archive),
            ps_quote(dest),
        ),
    }
}

pub(super) async fn remote_create_archive(
    conn: &SftpConnection,
    from_path: &str,
    name: &str,
    session: &RemotePackSession,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<String, String> {
    let (parent, base) = remote_parent_base(from_path);
    let out = join_tmp(
        &session.env.tmp,
        &format!(
            "terminus-sftp-{}-{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4(),
            archive_ext(session.kind)
        ),
    );

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Creating remote {}…", archive_ext(session.kind)),
            done: 0,
            total: 0,
        },
    );

    let script = archive_create_script(&parent, &base, name, &out, session);
    let (code, stdout, stderr) = conn.exec(&script).await.map_err(|e| e.to_string())?;
    if code != 0 {
        return Err(format!(
            "remote archive create failed (exit {code}): {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    let reported = String::from_utf8_lossy(&stdout);
    let remote = reported
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.contains('='))
        .unwrap_or(&out)
        .to_string();
    Ok(remote)
}

pub(super) async fn try_remote_pack_to_local(
    conn: &SftpConnection,
    from_path: &str,
    name: &str,
    staging: &Path,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<ArchiveKind, String> {
    let session = acquire_pack_session(conn).await?;
    let local = staging_with_ext(staging, session.kind);
    let remote =
        remote_create_archive(conn, from_path, name, &session, events, wake).await?;
    let download = transfer_download(conn, &remote, &local, events, wake).await;
    let _ = conn.remove(&remote).await;
    download?;
    Ok(session.kind)
}

pub(super) async fn remote_extract_uploaded(
    conn: &SftpConnection,
    local_archive: &Path,
    to_cwd: &str,
    _name: &str,
    kind: ArchiveKind,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let session = acquire_extract_session(conn, kind).await?;
    let remote_name = format!(
        "terminus-sftp-in-{}-{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        archive_ext(kind)
    );
    let remote = join_tmp(&session.env.tmp, &remote_name);
    transfer_upload(conn, local_archive, &remote, events, wake).await?;
    let script = archive_extract_script(to_cwd, &remote, &session);
    let (code, _stdout, stderr) = conn.exec(&script).await.map_err(|e| e.to_string())?;
    let _ = conn.remove(&remote).await;
    if code != 0 {
        let err = String::from_utf8_lossy(&stderr);
        return Err(format_dest_write_error(
            to_cwd,
            &format!("remote archive extract failed (exit {code}): {err}"),
        ));
    }
    Ok(())
}

pub(super) fn extract_local_archive(
    archive: &Path,
    dest_cwd: &Path,
    kind: ArchiveKind,
) -> Result<(), String> {
    match kind {
        ArchiveKind::Zip => unzip_local(archive, dest_cwd),
        ArchiveKind::TarGz => {
            std::fs::create_dir_all(dest_cwd).map_err(|e| e.to_string())?;
            let status = std::process::Command::new("tar")
                .arg("-C")
                .arg(dest_cwd)
                .arg("-xzf")
                .arg(archive)
                .status()
                .map_err(|e| format!("local tar extract: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("local tar extract failed ({status})"))
            }
        }
    }
}
