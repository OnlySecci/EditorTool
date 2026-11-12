//! On-demand GitHub release checks and verified, platform-native update handoff.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const API: &str = "https://api.github.com/repos/OnlySecci/EditorTool/releases/latest";
const DOWNLOADS: &str = "https://github.com/OnlySecci/EditorTool/releases/download/";
const LIMIT: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug)]
pub enum Kind {
    WindowsSetup,
    WindowsPortable,
    Mac,
    Deb,
    Rpm,
    LinuxPortable,
    FlatpakUser,
    FlatpakSystem,
}
#[derive(Clone, Deserialize, Debug)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct Response {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
    draft: bool,
    prerelease: bool,
}
#[derive(Clone, Debug)]
pub struct Release {
    pub tag: String,
    pub title: String,
    pub notes: String,
    pub asset: Asset,
    pub checksums: Asset,
    pub kind: Kind,
}
#[derive(Clone, Debug)]
pub struct Download {
    pub payload: PathBuf,
    pub kind: Kind,
}
fn version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim_start_matches(['v', 'V']).split('.');
    let result = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next().unwrap_or("0").parse().ok()?,
    );
    if parts.next().is_some() {
        None
    } else {
        Some(result)
    }
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .redirects(6)
        .build()
}
fn request(agent: &ureq::Agent, url: &str) -> Result<ureq::Response, String> {
    agent
        .get(url)
        .set(
            "User-Agent",
            concat!("EditorTool/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())
}
fn target() -> Result<(Kind, String), String> {
    let arch = std::env::consts::ARCH;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let kind = if cfg!(target_os = "windows") {
        if exe
            .parent()
            .is_some_and(|p| p.join("Uninstall.exe").is_file())
        {
            Kind::WindowsSetup
        } else {
            Kind::WindowsPortable
        }
    } else if cfg!(target_os = "macos") {
        Kind::Mac
    } else if let Ok(info) = fs::read_to_string("/.flatpak-info") {
        if info
            .lines()
            .filter_map(|line| line.strip_prefix("app-path="))
            .any(|path| path.starts_with("/var/lib/flatpak/app/"))
        {
            Kind::FlatpakSystem
        } else {
            Kind::FlatpakUser
        }
    } else if exe == Path::new("/usr/bin/editortool") {
        if Path::new("/usr/bin/dpkg").exists() {
            Kind::Deb
        } else if Path::new("/usr/bin/rpm").exists() {
            Kind::Rpm
        } else {
            return Err("Kein unterstützter Paketmanager gefunden.".into());
        }
    } else {
        Kind::LinuxPortable
    };
    let architecture = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Err("Architektur wird nicht unterstützt.".into()),
    };
    Ok((kind, architecture.into()))
}
pub fn check() -> Result<Option<Release>, String> {
    crate::diagnostics::event("updater", "check_started");
    let response = request(&agent(), API)?;
    let data: Response = serde_json::from_reader(response.into_reader().take(4 * 1024 * 1024))
        .map_err(|e| e.to_string())?;
    let latest = version(&data.tag_name).ok_or("Ungültige Versionsnummer im GitHub-Release.")?;
    if data.draft || data.prerelease || latest <= version(env!("CARGO_PKG_VERSION")).unwrap() {
        return Ok(None);
    }
    let (kind, arch) = target()?;
    let package_version = format!("{}.{}.{}", latest.0, latest.1, latest.2);
    let native_arch = if arch == "x64" { "x86_64" } else { "aarch64" };
    let name = match kind {
        Kind::WindowsSetup => format!("EditorTool-{package_version}-windows-{arch}-setup.exe"),
        Kind::WindowsPortable => {
            format!("EditorTool-{package_version}-windows-{arch}-portable.zip")
        }
        Kind::Mac => format!(
            "EditorTool-{package_version}-macos-{}.dmg",
            if arch == "x64" { "x86_64" } else { "arm64" }
        ),
        Kind::Deb => format!(
            "editortool_{package_version}_{}.deb",
            if arch == "x64" { "amd64" } else { "arm64" }
        ),
        Kind::Rpm => format!("editortool-{package_version}-1.{native_arch}.rpm"),
        Kind::LinuxPortable => {
            format!("EditorTool-{package_version}-linux-{native_arch}-portable.tar.gz")
        }
        Kind::FlatpakUser | Kind::FlatpakSystem => {
            format!("EditorTool-{package_version}-linux-{native_arch}.flatpak")
        }
    };
    let asset = data
        .assets
        .iter()
        .find(|a| a.name == name)
        .cloned()
        .ok_or("Für diese Installation fehlt das Update-Paket im Release.")?;
    let checksums = data
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .cloned()
        .ok_or("Prüfsummen fehlen im Release.")?;
    for item in [&asset, &checksums] {
        if !item.browser_download_url.starts_with(DOWNLOADS) {
            return Err("Unerwartete Download-Adresse im Release.".into());
        }
    }
    Ok(Some(Release {
        tag: data.tag_name,
        title: data.name.unwrap_or_else(|| name.clone()),
        notes: data.body.unwrap_or_default(),
        asset,
        checksums,
        kind,
    }))
}
pub fn download(
    release: Release,
    directory: PathBuf,
    mut progress: impl FnMut(u8),
) -> Result<Download, String> {
    crate::diagnostics::event(
        "updater",
        &format!("download_started kind={:?}", release.kind),
    );
    let agent = agent();
    let mut manifest = String::new();
    request(&agent, &release.checksums.browser_download_url)?
        .into_reader()
        .take(65537)
        .read_to_string(&mut manifest)
        .map_err(|e| e.to_string())?;
    if manifest.len() > 65536 {
        return Err("Prüfsummendatei ist zu gross.".into());
    }
    let expected = manifest
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let hash = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            if name == release.asset.name
                && hash.len() == 64
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
            {
                Some(hash.to_ascii_lowercase())
            } else {
                None
            }
        })
        .ok_or("Prüfsumme für das ausgewählte Paket fehlt.")?;
    if release.asset.size == 0 || release.asset.size > LIMIT {
        return Err("Update-Paket hat eine unerwartete Grösse.".into());
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = directory.join(format!("update-{nonce}"));
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let payload = directory.join(&release.asset.name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&payload)
        .map_err(|e| e.to_string())?;
    let mut reader = request(&agent, &release.asset.browser_download_url)?.into_reader();
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut count = 0u64;
    let mut previous = 255;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > LIMIT || count > release.asset.size {
            return Err("Update-Download ist grösser als angekündigt.".into());
        }
        hash.update(&buffer[..n]);
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        let percent = (count * 100 / release.asset.size).min(100) as u8;
        if percent != previous {
            progress(percent);
            previous = percent;
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    if count != release.asset.size || format!("{:x}", hash.finalize()) != expected {
        return Err("Update-Prüfsumme stimmt nicht. Das Paket wird nicht installiert.".into());
    }
    Ok(Download {
        payload,
        kind: release.kind,
    })
}
#[cfg(target_os = "macos")]
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}
pub fn install(download: &Download, directory: &Path) -> Result<(), String> {
    crate::diagnostics::event(
        "updater",
        &format!("install_handoff kind={:?}", download.kind),
    );
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let result = directory.join("last-result.txt");
    fs::write(&result, "pending").map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let pid = std::process::id().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let helper = download.payload.parent().unwrap().join("install.ps1");
        fs::write(&helper,r#"param([string]$payload,[string]$destination,[int]$editorProcessId,[string]$mode,[string]$result)
$ErrorActionPreference='Stop'
Wait-Process -Id $editorProcessId -ErrorAction SilentlyContinue
try {
  if($mode -eq 'setup') {
    $installer=Start-Process -FilePath $payload -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
    if($installer.ExitCode -ne 0){throw "Installer exit code: $($installer.ExitCode)"}
  } else {
    $staging=Join-Path (Split-Path -Parent $payload) 'expanded'
    Expand-Archive -LiteralPath $payload -DestinationPath $staging -Force
    Get-ChildItem -LiteralPath $staging | Where-Object Name -ne 'portable.txt' | Copy-Item -Destination (Split-Path -Parent $destination) -Recurse -Force
  }
  Set-Content -LiteralPath $result -Value 'ok' -Encoding utf8
  Start-Process -FilePath $destination
} catch {
  Set-Content -LiteralPath $result -Value ('error: '+$_.Exception.Message) -Encoding utf8
  try { Start-Process -FilePath $destination } catch {}
}
"#).map_err(|e|e.to_string())?;
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
            .arg(helper)
            .arg(&download.payload)
            .arg(&exe)
            .arg(pid)
            .arg(if matches!(download.kind, Kind::WindowsSetup) {
                "setup"
            } else {
                "portable"
            })
            .arg(result)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        let bundle = exe
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or("App-Bundle nicht gefunden.")?;
        if bundle.extension().is_none_or(|e| e != "app")
            || !bundle.join("Contents/Info.plist").is_file()
        {
            return Err("Bitte EditorTool aus seinem App-Bundle starten.".into());
        }
        let work = download.payload.parent().unwrap();
        let mount = work.join("mounted");
        fs::create_dir_all(&mount).map_err(|e| e.to_string())?;
        let nonce = work.file_name().unwrap().to_string_lossy();
        let staged = bundle
            .parent()
            .unwrap()
            .join(format!(".EditorTool-{nonce}.app"));
        let backup = bundle
            .parent()
            .unwrap()
            .join(format!(".EditorTool-previous-{nonce}.app"));
        let command = format!(
            "/usr/bin/ditto {} {} && /bin/mv {} {} && (/bin/mv {} {} || (/bin/mv {} {}; exit 1))",
            quote(&mount.join("EditorTool.app")),
            quote(&staged),
            quote(bundle),
            quote(&backup),
            quote(&staged),
            quote(bundle),
            quote(&backup),
            quote(bundle)
        );
        let helper = work.join("install.sh");
        fs::write(&helper,r#"set -eu
payload=$1; destination=$2; editor_pid=$3; result=$4; mount=$5; install_command=$6
exec >>"$result.log" 2>&1
trap 'printf "error: Installation fehlgeschlagen; siehe Update-Protokoll.\n" > "$result"; /usr/bin/hdiutil detach "$mount" >/dev/null 2>&1 || true; /usr/bin/open "$destination" || true' EXIT
while kill -0 "$editor_pid" 2>/dev/null; do sleep 1; done
/usr/bin/hdiutil attach "$payload" -nobrowse -readonly -mountpoint "$mount"
if [ -w "$(dirname "$destination")" ]; then /bin/sh -c "$install_command"
else /usr/bin/osascript -e 'on run argv' -e 'do shell script (item 1 of argv) with administrator privileges' -e 'end run' "$install_command"; fi
/usr/bin/hdiutil detach "$mount"
printf 'ok\n' > "$result"
trap - EXIT
/usr/bin/open "$destination"
"#).map_err(|e|e.to_string())?;
        Command::new("/bin/sh")
            .arg(helper)
            .arg(&download.payload)
            .arg(bundle)
            .arg(pid)
            .arg(result)
            .arg(mount)
            .arg(command)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let helper = download.payload.parent().unwrap().join("install.sh");
        fs::write(&helper,r#"set -eu
payload=$1; destination=$2; editor_pid=$3; mode=$4; result=$5
exec >>"$result.log" 2>&1
failed() {
  printf "error: Installation fehlgeschlagen; siehe Update-Protokoll.\n" > "$result"
  case "$mode" in
    flatpak-user) /usr/bin/flatpak run --user com.github.OnlySecci.EditorTool >/dev/null 2>&1 & ;;
    flatpak-system) /usr/bin/flatpak run --system com.github.OnlySecci.EditorTool >/dev/null 2>&1 & ;;
    *) "$destination" >/dev/null 2>&1 & ;;
  esac
}
trap failed EXIT
case "$mode" in
 flatpak-user|flatpak-system)
   while /usr/bin/flatpak ps --columns=application | grep -Fxq com.github.OnlySecci.EditorTool; do sleep 1; done
   if [ "$mode" = flatpak-system ]; then
     pkexec /usr/bin/flatpak install --system --noninteractive -y --bundle "$payload"
   else
     /usr/bin/flatpak install --user --noninteractive -y --bundle "$payload"
   fi
   ;;
 *)
   while kill -0 "$editor_pid" 2>/dev/null; do sleep 1; done
   case "$mode" in
    deb) pkexec /usr/bin/dpkg -i "$payload" ;;
    rpm) pkexec /usr/bin/rpm -U --replacepkgs "$payload" ;;
    portable)
      staging="$(dirname "$payload")/expanded"
      mkdir -p "$staging"
      tar -xzf "$payload" -C "$staging"
      if [ -w "$(dirname "$destination")" ]; then
        cp "$staging/EditorTool" "$destination.next"
        chmod 755 "$destination.next"
        mv "$destination.next" "$destination"
        cp "$staging/README.md" "$staging/LICENSE" "$(dirname "$destination")/"
        cp -R "$staging/third-party-licenses" "$(dirname "$destination")/"
      else pkexec /usr/bin/install -m 755 "$staging/EditorTool" "$destination"; fi
      ;;
   esac
   ;;
esac
printf 'ok\n' > "$result"
trap - EXIT
case "$mode" in
 flatpak-user) /usr/bin/flatpak run --user com.github.OnlySecci.EditorTool >/dev/null 2>&1 & ;;
    flatpak-system) /usr/bin/flatpak run --system com.github.OnlySecci.EditorTool >/dev/null 2>&1 & ;;
 *) "$destination" >/dev/null 2>&1 & ;;
esac
"#).map_err(|e|e.to_string())?;
        let mode = match download.kind {
            Kind::Deb => "deb",
            Kind::Rpm => "rpm",
            Kind::FlatpakUser => "flatpak-user",
            Kind::FlatpakSystem => "flatpak-system",
            _ => "portable",
        };
        let flatpak = matches!(download.kind, Kind::FlatpakUser | Kind::FlatpakSystem);
        let mut command = if flatpak {
            let mut cmd = Command::new("flatpak-spawn");
            // Detach the host helper so flatpak-spawn itself exits. Otherwise
            // it would keep the old sandbox alive while the helper waits for it.
            cmd.arg("--host")
                .arg("/bin/sh")
                .arg("-c")
                .arg("nohup /bin/sh \"$@\" </dev/null >/dev/null 2>&1 &")
                .arg("editortool-updater");
            cmd
        } else {
            Command::new("/bin/sh")
        };
        command
            .arg(helper)
            .arg(&download.payload)
            .arg(exe)
            .arg(pid)
            .arg(mode)
            .arg(result);
        if flatpak {
            if !command.status().map_err(|e| e.to_string())?.success() {
                return Err("Der Flatpak-Host konnte die Aktualisierung nicht starten.".into());
            }
        } else {
            command.spawn().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
pub fn previous_error(directory: &Path) -> Option<String> {
    let path = directory.join("last-result.txt");
    let text = fs::read_to_string(&path).ok()?;
    if text.trim_start_matches('\u{feff}').trim() == "ok" {
        let _ = fs::remove_file(path);
        return None;
    }
    if text.starts_with("pending") {
        let _ = fs::remove_file(path);
        return Some("Ein vorheriges Update wurde unterbrochen. Du kannst über Hilfe → Nach Updates suchen erneut aktualisieren.".into());
    }
    let _ = fs::remove_file(path);
    Some(format!(
        "{}\nProtokoll: {}",
        text,
        directory.join("last-result.txt.log").display()
    ))
}
