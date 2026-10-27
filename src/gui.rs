use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const WINDOWS_FORM: &str = include_str!("ui.ps1");
const ICON_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), r"\assets\blocksite.ico");

pub fn run() -> Result<(), String> {
    let app_dir = config_directory()?;
    fs::create_dir_all(&app_dir)
        .map_err(|error| format!("Não foi possível criar a pasta do Blocksite: {error}"))?;

    let script_path = app_dir.join("interface.ps1");
    let mut script = vec![0xEF, 0xBB, 0xBF];
    script.extend_from_slice(WINDOWS_FORM.as_bytes());
    fs::write(&script_path, script)
        .map_err(|error| format!("Não foi possível preparar a interface: {error}"))?;

    let executable = env::current_exe()
        .map_err(|error| format!("Não foi possível localizar o Blocksite: {error}"))?;
    let app_dir = app_dir.to_string_lossy().into_owned();
    let mut powershell = Command::new("powershell.exe");
    powershell
        .args(["-NoProfile", "-STA", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .arg("-Executable")
        .arg(executable)
        .arg("-ConfigDirectory")
        .arg(app_dir)
        .arg("-IconPath")
        .arg(ICON_PATH);
    hide_console_window(&mut powershell);
    let status = powershell
        .status()
        .map_err(|error| format!("Não foi possível abrir a interface do Windows: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("A interface encerrou com o código {}.", status))
    }
}

pub fn is_elevated() -> Result<bool, String> {
    let mut powershell = Command::new("powershell.exe");
    powershell
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent()); if ($p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { 'true' } else { 'false' }",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut powershell);
    let output = powershell.output().map_err(|error| {
        format!("Não foi possível verificar as permissões de administrador: {error}")
    })?;

    if !output.status.success() {
        return Err(format!(
            "Falha ao verificar permissões: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .eq_ignore_ascii_case("true"))
}

pub fn run_as_administrator() -> Result<(), String> {
    let executable = env::current_exe()
        .map_err(|error| format!("Não foi possível localizar o Blocksite: {error}"))?;
    let config_dir = config_directory()?;
    let executable = ps_quote(&executable.to_string_lossy());
    let config_dir = ps_quote(&config_dir.to_string_lossy());
    let script = format!(
        "$env:BLOCKSITE_CONFIG_DIR = '{config_dir}'; \
         $process = Start-Process -FilePath '{executable}' \
         -ArgumentList 'gui-elevated' -Verb RunAs -PassThru -Wait; \
         exit $process.ExitCode"
    );
    let encoded = encode_powershell(&script);
    let mut powershell = Command::new("powershell.exe");
    powershell.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
    hide_console_window(&mut powershell);
    let status = powershell
        .status()
        .map_err(|error| format!("Não foi possível solicitar acesso de administrador: {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("O Blocksite não foi iniciado. Aprove a solicitação de administrador do Windows e tente novamente.".into())
    }
}

pub fn show_error(message: &str) {
    let escaped = ps_quote(message);
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         [void][System.Windows.Forms.MessageBox]::Show('{escaped}', 'Blocksite', \
         [System.Windows.Forms.MessageBoxButtons]::OK, \
         [System.Windows.Forms.MessageBoxIcon]::Error)"
    );
    let encoded = encode_powershell(&script);
    let mut powershell = Command::new("powershell.exe");
    powershell.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
    hide_console_window(&mut powershell);
    let _ = powershell.status();
}

fn hide_console_window(command: &mut Command) {
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

fn config_directory() -> Result<std::path::PathBuf, String> {
    if let Some(directory) = env::var_os("BLOCKSITE_CONFIG_DIR") {
        return Ok(Path::new(&directory).to_path_buf());
    }
    let appdata = env::var_os("APPDATA").ok_or("A variável APPDATA não está definida.")?;
    Ok(Path::new(&appdata).join("Blocksite"))
}

fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn encode_powershell(script: &str) -> String {
    let bytes = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_powershell_paths() {
        assert_eq!(ps_quote("C:\\Users\\O'Connor"), "C:\\Users\\O''Connor");
    }

    #[test]
    fn base64_encodes_utf16le_powershell_text() {
        assert_eq!(encode_powershell("A"), "QQA=");
    }
}
