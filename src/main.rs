#![cfg_attr(not(test), windows_subsystem = "windows")]

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod gui;

const HOSTS_BEGIN: &str = "# >>> BLOCKSITE BEGIN >>>";
const HOSTS_END: &str = "# <<< BLOCKSITE END <<<";
const INSTALL_GRANT_SECONDS: u64 = 30 * 60;
const BUILTIN_ADULT_DOMAINS: &[&str] = &[
    "beeg.com",
    "bongacams.com",
    "cam4.com",
    "chaturbate.com",
    "eporner.com",
    "hentaihaven.xxx",
    "livejasmin.com",
    "nhentai.net",
    "onlyfans.com",
    "porn.com",
    "pornhub.com",
    "redtube.com",
    "spankbang.com",
    "stripchat.com",
    "tube8.com",
    "xhamster.com",
    "xhamsterlive.com",
    "xnxx.com",
    "xvideos.com",
    "youporn.com",
];
const BLOCK_PAGE_HTML: &str = include_str!("block_page.html");
const BLOCK_PAGE_ADDRESS: &str = "127.0.0.1:80";
const BLOCK_PAGE_HEALTH_TOKEN: &str = "blocksite-local-block-page";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum LockMode {
    Allow,
    #[default]
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    password_hash: Option<String>,
    lock_mode: LockMode,
    allowed_sites: Vec<String>,
    denied_sites: Vec<String>,
    #[serde(default = "default_adult_sites_blocked")]
    adult_sites_blocked: bool,
    #[serde(default = "default_site_blocking_enabled")]
    site_blocking_enabled: bool,
    wallpaper_path: Option<String>,
    wallpaper_locked: bool,
    installation_locked: bool,
    settings_locked: bool,
    install_grant_until: Option<u64>,
    settings_grant_until: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            password_hash: None,
            lock_mode: LockMode::Deny,
            allowed_sites: Vec::new(),
            denied_sites: Vec::new(),
            adult_sites_blocked: default_adult_sites_blocked(),
            site_blocking_enabled: default_site_blocking_enabled(),
            wallpaper_path: None,
            wallpaper_locked: false,
            installation_locked: false,
            settings_locked: false,
            install_grant_until: None,
            settings_grant_until: None,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        gui::show_error(&error);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return if gui::is_elevated()? {
            gui::run()
        } else {
            gui::run_as_administrator()
        };
    };
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "init" => initialize(),
        "mode" => set_mode(&rest),
        "sites" => set_site_blocking(&rest),
        "apply-sites" => apply_sites_command(),
        "apply" => apply(),
        "wallpaper" => set_wallpaper(&rest),
        "unlock-wallpaper" => change_wallpaper_lock(false),
        "expiry-monitor" => expiry_monitor(),
        "block-page-server" => block_page_server(),
        "gui-elevated" => {
            if !gui::is_elevated()? {
                return Err("O Blocksite precisa ser iniciado como administrador.".into());
            }
            gui::run()
        }
        "unlock-install" => unlock_install(),
        "lock-install" => lock_install(),
        "unlock-settings" => change_settings_lock(false),
        "lock-settings" => change_settings_lock(true),
        "set-password" => change_password(),
        "verify-password" => verify_password(&load_config()?),
        "status" => status(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(format!("comando desconhecido: {command}"))
        }
    }
}

fn print_help() {
    println!(
        "Blocksite - controle local de sites e do ambiente Windows\n\
         \n\
         Comandos:\n\
         \tinit\n\
         \tmode allow <dominios-separados-por-virgula>\n\
         \tmode deny <dominios-separados-por-virgula>\n\
         \tsites enable|disable\n\
         \tapply-sites\n\
         \tapply\n\
         \twallpaper <caminho-da-imagem>\n\
         \tunlock-install\n\
         \tlock-install\n\
         \tunlock-settings\n\
         \tlock-settings\n\
         \tset-password\n\
         \tstatus\n\
         \thelp\n\
         \n\
         Senhas são solicitadas interativamente e não devem ser passadas como argumentos."
    );
}

fn initialize() -> Result<(), String> {
    let mut config = load_config()?;
    if config.password_hash.is_some() {
        return Err("A senha já foi configurada. Use set-password para alterá-la.".into());
    }

    let password = read_password("Crie uma senha: ")?;
    if password.trim().len() < 8 {
        return Err("Use uma senha com pelo menos 8 caracteres.".into());
    }
    let confirmation = read_password("Confirme a senha: ")?;
    if password != confirmation {
        return Err("As senhas não coincidem.".into());
    }

    config.password_hash = Some(hash_password(&password)?);
    config.installation_locked = true;
    config.settings_locked = true;
    config.adult_sites_blocked = true;
    config.site_blocking_enabled = true;
    save_config(&config)?;
    println!(
        "Senha configurada. Execute `blocksite apply` como administrador para aplicar as políticas."
    );
    Ok(())
}

fn set_mode(args: &[String]) -> Result<(), String> {
    let mode = args.first().ok_or("Informe allow ou deny.")?;
    let sites = args.get(1).map(String::as_str).unwrap_or("");
    let sites = parse_domains(sites)?;
    let adult_sites_blocked = args
        .get(2)
        .map(|value| match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err("A opção de bloqueio adulto deve ser true ou false."),
        })
        .transpose()?;
    let mut config = load_config()?;
    verify_password(&config)?;
    if let Some(adult_sites_blocked) = adult_sites_blocked {
        config.adult_sites_blocked = adult_sites_blocked;
    }

    match mode.as_str() {
        "allow" => {
            if sites.is_empty() {
                return Err("Informe ao menos um domínio permitido.".into());
            }
            config.lock_mode = LockMode::Allow;
            config.allowed_sites = sites;
        }
        "deny" => {
            config.lock_mode = LockMode::Deny;
            config.denied_sites = sites;
        }
        _ => return Err("Modo inválido. Use allow ou deny.".into()),
    }

    save_config(&config)?;
    println!("Modo salvo. Execute `blocksite apply` para aplicar as regras.");
    Ok(())
}

fn default_adult_sites_blocked() -> bool {
    true
}

fn default_site_blocking_enabled() -> bool {
    true
}

fn set_site_blocking(args: &[String]) -> Result<(), String> {
    let enabled = match args.first().map(String::as_str) {
        Some("enable") => true,
        Some("disable") => false,
        _ => return Err("Use `sites enable` ou `sites disable`.".into()),
    };
    let mut config = load_config()?;
    ensure_initialized(&config)?;
    verify_password(&config)?;
    let previous_config = config.clone();
    config.site_blocking_enabled = enabled;
    if enabled {
        config.lock_mode = LockMode::Deny;
    }

    save_config(&config)?;
    let apply_result = (|| {
        if enabled && !denied_domains(&config).is_empty() {
            ensure_block_page_server()?;
        }
        apply_hosts(&config)
    })();

    if let Err(error) = apply_result {
        let config_rollback = save_config(&previous_config);
        let hosts_rollback = apply_hosts(&previous_config);
        let mut rollback_errors = Vec::new();
        if let Err(rollback_error) = config_rollback {
            rollback_errors.push(format!("configuração: {rollback_error}"));
        }
        if let Err(rollback_error) = hosts_rollback {
            rollback_errors.push(format!("regras do arquivo hosts: {rollback_error}"));
        }
        return if rollback_errors.is_empty() {
            Err(error)
        } else {
            Err(format!(
                "{error}. Falha ao restaurar {}.",
                rollback_errors.join("; ")
            ))
        };
    }

    println!(
        "Bloqueio de sites {}.",
        if enabled {
            "ativado"
        } else {
            "liberado até ser ativado novamente"
        }
    );
    Ok(())
}

fn parse_domains(input: &str) -> Result<Vec<String>, String> {
    let mut domains = Vec::new();
    for value in input
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value = value.to_ascii_lowercase();
        let domain = value
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string();
        if !valid_domain(&domain) {
            return Err(format!("Domínio inválido: {value}"));
        }
        if !domains.contains(&domain) {
            domains.push(domain);
        }
    }
    Ok(domains)
}

fn valid_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 || domain.contains(['/', ':', '@', ' ']) {
        return false;
    }

    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn apply() -> Result<(), String> {
    let config = load_config()?;
    ensure_initialized(&config)?;
    verify_password(&config)?;
    apply_sites(&config)?;
    apply_wallpaper(&config)?;
    apply_installation_lock(&config)?;
    apply_settings_lock(&config)?;
    println!(
        "Políticas aplicadas. A página Blocksite é exibida em conexões HTTP; conexões HTTPS não podem ser redirecionadas sem um certificado confiável."
    );
    Ok(())
}

fn apply_sites_command() -> Result<(), String> {
    let config = load_config()?;
    ensure_initialized(&config)?;
    verify_password(&config)?;
    apply_sites(&config)
}

fn apply_sites(config: &Config) -> Result<(), String> {
    if config.site_blocking_enabled && !denied_domains(&config).is_empty() {
        ensure_block_page_server()?;
    }
    apply_hosts(&config)?;
    println!(
        "Bloqueio de sites {}.",
        if config.site_blocking_enabled {
            "aplicado"
        } else {
            "liberado"
        }
    );
    Ok(())
}

fn set_wallpaper(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("Informe o caminho da imagem.")?;
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!(
            "Arquivo de imagem não encontrado: {}",
            path.display()
        ));
    }
    let mut config = load_config()?;
    ensure_initialized(&config)?;
    verify_password(&config)?;
    config.wallpaper_path = Some(path.to_string_lossy().into_owned());
    config.wallpaper_locked = true;
    save_config(&config)?;
    apply_wallpaper(&config)?;
    println!("Wallpaper aplicado e a alteração pelo usuário foi desabilitada.");
    Ok(())
}

fn change_wallpaper_lock(locked: bool) -> Result<(), String> {
    let mut config = load_config()?;
    verify_password(&config)?;
    config.wallpaper_locked = locked;
    save_config(&config)?;
    apply_wallpaper(&config)?;
    println!(
        "Alteração do wallpaper {}.",
        if locked { "bloqueada" } else { "liberada" }
    );
    Ok(())
}

fn unlock_install() -> Result<(), String> {
    let mut config = load_config()?;
    verify_password(&config)?;
    let previous_config = config.clone();
    let expiry = current_epoch()? + INSTALL_GRANT_SECONDS;
    config.installation_locked = false;
    config.settings_locked = false;
    config.install_grant_until = Some(expiry);
    config.settings_grant_until = Some(expiry);
    save_config(&config)?;
    if let Err(error) = launch_expiry_monitor() {
        config = previous_config;
        save_config(&config)?;
        apply_installation_lock(&config)?;
        apply_settings_lock(&config)?;
        return Err(error);
    }
    apply_installation_lock(&config)?;
    apply_settings_lock(&config)?;
    println!("Instalação MSI liberada por 30 minutos.");
    Ok(())
}

fn lock_install() -> Result<(), String> {
    let mut config = load_config()?;
    verify_password(&config)?;
    config.installation_locked = true;
    config.install_grant_until = None;
    save_config(&config)?;
    apply_installation_lock(&config)?;
    println!("Bloqueio de instalação MSI reativado.");
    Ok(())
}

fn change_settings_lock(locked: bool) -> Result<(), String> {
    let mut config = load_config()?;
    verify_password(&config)?;
    if locked {
        config.settings_locked = true;
        config.settings_grant_until = None;
        save_config(&config)?;
        apply_settings_lock(&config)?;
    } else {
        let previous_config = config.clone();
        config.settings_locked = false;
        config.settings_grant_until = Some(current_epoch()? + INSTALL_GRANT_SECONDS);
        save_config(&config)?;
        if let Err(error) = launch_expiry_monitor() {
            config = previous_config;
            save_config(&config)?;
            apply_settings_lock(&config)?;
            return Err(error);
        }
        apply_settings_lock(&config)?;
    }
    println!(
        "Acesso às Configurações do Windows {}.",
        if locked {
            "bloqueado"
        } else {
            "liberado por 30 minutos"
        }
    );
    Ok(())
}

fn change_password() -> Result<(), String> {
    let mut config = load_config()?;
    verify_password(&config)?;
    let password = read_password("Nova senha (mínimo 8 caracteres): ")?;
    if password.trim().len() < 8 {
        return Err("Use uma senha com pelo menos 8 caracteres.".into());
    }
    let confirmation = read_password("Confirme a nova senha: ")?;
    if password != confirmation {
        return Err("As senhas não coincidem.".into());
    }
    config.password_hash = Some(hash_password(&password)?);
    save_config(&config)
}

fn status() -> Result<(), String> {
    let config = load_config()?;
    println!("Senha configurada: {}", config.password_hash.is_some());
    println!("Modo de sites: {:?}", config.lock_mode);
    println!("Sites permitidos: {}", config.allowed_sites.join(", "));
    println!("Sites bloqueados: {}", config.denied_sites.join(", "));
    println!("Bloqueio de sites adultos: {}", config.adult_sites_blocked);
    println!("Bloqueio de sites ativo: {}", config.site_blocking_enabled);
    println!("Wallpaper bloqueado: {}", config.wallpaper_locked);
    println!("Instalação MSI bloqueada: {}", config.installation_locked);
    println!("Configurações bloqueadas: {}", config.settings_locked);
    if let Some(expiry) = config.install_grant_until {
        println!("Liberação MSI até (Unix): {expiry}");
    }
    if let Some(expiry) = config.settings_grant_until {
        println!("Configurações liberadas até (Unix): {expiry}");
    }
    Ok(())
}

fn ensure_initialized(config: &Config) -> Result<(), String> {
    if config.password_hash.is_none() {
        Err("Execute `blocksite init` para configurar a senha primeiro.".into())
    } else {
        Ok(())
    }
}

fn verify_password(config: &Config) -> Result<(), String> {
    let password = read_password("Senha: ")?;
    verify_password_with(config, &password)
}

fn read_password(prompt: &str) -> Result<String, String> {
    if io::stdin().is_terminal() {
        return prompt_password(prompt).map_err(|error| error.to_string());
    }

    eprint!("{prompt}");
    let mut password = String::new();
    io::stdin()
        .read_line(&mut password)
        .map_err(|error| format!("Não foi possível ler a senha: {error}"))?;
    Ok(password.trim_end_matches(['\r', '\n']).to_string())
}

fn verify_password_with(config: &Config, password: &str) -> Result<(), String> {
    let hash = config
        .password_hash
        .as_deref()
        .ok_or("Execute `blocksite init` para configurar a senha primeiro.")?;
    let parsed = PasswordHash::new(hash).map_err(|e| format!("Hash de senha inválido: {e}"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| "Senha incorreta.".to_string())
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("Não foi possível proteger a senha: {e}"))
}

fn config_path() -> Result<PathBuf, String> {
    if let Some(directory) = env::var_os("BLOCKSITE_CONFIG_DIR") {
        return Ok(Path::new(&directory).join("config.json"));
    }
    let appdata = env::var_os("APPDATA").ok_or("A variável APPDATA não está definida.")?;
    Ok(Path::new(&appdata).join("Blocksite").join("config.json"))
}

fn load_config() -> Result<Config, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Não foi possível ler {}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("Configuração inválida: {e}"))
}

fn save_config(config: &Config) -> Result<(), String> {
    let path = config_path()?;
    let parent = path.parent().ok_or("Caminho de configuração inválido.")?;
    fs::create_dir_all(parent)
        .map_err(|e| format!("Não foi possível criar a configuração: {e}"))?;
    let content = serde_json::to_vec_pretty(config)
        .map_err(|e| format!("Não foi possível serializar a configuração: {e}"))?;
    fs::write(&path, content).map_err(|e| format!("Não foi possível salvar a configuração: {e}"))
}

fn apply_hosts(config: &Config) -> Result<(), String> {
    if config.site_blocking_enabled && config.lock_mode == LockMode::Allow {
        return Err(
            "O modo allowlist não pode ser aplicado com segurança pelo arquivo hosts do Windows: \
             ele não oferece bloqueio geral nem curingas. Use denylist ou implemente um proxy/filtro de rede."
                .into(),
        );
    }

    let windows = env::var_os("WINDIR").ok_or("A variável WINDIR não está definida.")?;
    let hosts_path = Path::new(&windows)
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts");
    let original = fs::read_to_string(&hosts_path)
        .map_err(|e| format!("Não foi possível ler {}: {e}", hosts_path.display()))?;
    let mut lines = strip_managed_hosts(&original)?;
    lines.push(HOSTS_BEGIN.to_string());
    for domain in denied_domains(config) {
        if !valid_domain(&domain) {
            return Err(format!("Domínio inválido na configuração: {domain}"));
        }
        lines.push(format!("127.0.0.1 {domain}"));
        lines.push(format!("127.0.0.1 www.{domain}"));
    }
    lines.push(HOSTS_END.to_string());
    let content = format!("{}\r\n", lines.join("\r\n"));
    fs::write(&hosts_path, content).map_err(|e| {
        format!(
            "Não foi possível atualizar {}. Execute como administrador. Erro: {e}",
            hosts_path.display()
        )
    })?;
    run_command("ipconfig", &["/flushdns"])?;
    Ok(())
}

fn denied_domains(config: &Config) -> Vec<String> {
    if !config.site_blocking_enabled {
        return Vec::new();
    }
    let mut domains = config.denied_sites.clone();
    if config.adult_sites_blocked {
        for domain in BUILTIN_ADULT_DOMAINS {
            if !domains.iter().any(|existing| existing == domain) {
                domains.push((*domain).to_string());
            }
        }
    }
    domains
}

fn strip_managed_hosts(input: &str) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let mut inside = false;
    let mut found_begin = false;
    let mut found_end = false;

    for line in input.lines() {
        match line.trim() {
            HOSTS_BEGIN if !inside => {
                inside = true;
                found_begin = true;
            }
            HOSTS_END if inside => {
                inside = false;
                found_end = true;
            }
            HOSTS_BEGIN | HOSTS_END => {
                return Err(
                    "Marcadores de bloqueio Blocksite inconsistentes no arquivo hosts.".into(),
                );
            }
            _ if !inside => lines.push(line.to_string()),
            _ => {}
        }
    }

    if inside || found_begin != found_end {
        return Err(
            "Bloco Blocksite incompleto no arquivo hosts; nenhuma alteração foi feita.".into(),
        );
    }
    Ok(lines)
}

fn apply_wallpaper(config: &Config) -> Result<(), String> {
    if !config.wallpaper_locked {
        return registry_add(
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\ActiveDesktop",
            "NoChangingWallPaper",
            "REG_DWORD",
            "0",
        );
    }
    let path = config
        .wallpaper_path
        .as_deref()
        .ok_or("Wallpaper marcado como bloqueado, mas nenhum caminho foi configurado.")?;
    if !Path::new(path).is_file() {
        return Err(format!("Arquivo de wallpaper não encontrado: {path}"));
    }

    registry_add("HKCU\\Control Panel\\Desktop", "Wallpaper", "REG_SZ", path)?;
    registry_add(
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\ActiveDesktop",
        "NoChangingWallPaper",
        "REG_DWORD",
        "1",
    )?;
    run_command(
        "Rundll32.exe",
        &["User32.dll,UpdatePerUserSystemParameters"],
    )?;
    Ok(())
}

fn apply_installation_lock(config: &Config) -> Result<(), String> {
    let now = current_epoch()?;
    let locked = config.installation_locked
        || config
            .install_grant_until
            .is_some_and(|expiry| expiry <= now);
    registry_add(
        "HKLM\\Software\\Policies\\Microsoft\\Windows\\Installer",
        "DisableMSI",
        "REG_DWORD",
        if locked { "2" } else { "0" },
    )
}

fn apply_settings_lock(config: &Config) -> Result<(), String> {
    let now = current_epoch()?;
    let locked = config.settings_locked
        || config
            .settings_grant_until
            .is_some_and(|expiry| expiry <= now);
    registry_add(
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer",
        "NoControlPanel",
        "REG_DWORD",
        if locked { "1" } else { "0" },
    )
}

fn registry_add(key: &str, name: &str, kind: &str, value: &str) -> Result<(), String> {
    run_command(
        "reg",
        &["add", key, "/v", name, "/t", kind, "/d", value, "/f"],
    )
}

fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não foi possível executar {program}: {e}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Não foi possível capturar a saída de {program}."))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Não foi possível capturar os erros de {program}."))?;
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stderr.read_to_end(&mut output).map(|_| output)
    });

    let deadline = std::time::Instant::now() + COMMAND_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                child.kill().map_err(|e| {
                    format!("Não foi possível encerrar {program} após timeout: {e}")
                })?;
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "O comando {program} excedeu o limite de 30 segundos. A operação foi interrompida."
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("Não foi possível aguardar {program}: {error}"));
            }
        }
    };
    let _stdout = stdout_reader
        .join()
        .map_err(|_| format!("Falha ao ler a saída de {program}."))?
        .map_err(|e| format!("Não foi possível ler a saída de {program}: {e}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("Falha ao ler os erros de {program}."))?
        .map_err(|e| format!("Não foi possível ler os erros de {program}: {e}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!(
        "Comando {program} falhou: {}",
        String::from_utf8_lossy(&stderr).trim()
    ))
}

fn ensure_block_page_server() -> Result<(), String> {
    if block_page_server_is_running() {
        return Ok(());
    }

    let executable = env::current_exe()
        .map_err(|error| format!("Não foi possível localizar o Blocksite: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("block-page-server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map_err(|error| format!("Não foi possível iniciar a página local de bloqueio: {error}"))?;

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if block_page_server_is_running() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(
        "Não foi possível iniciar o servidor da página de bloqueio em 127.0.0.1:80. \
         Verifique se outro programa já está usando a porta 80."
            .into(),
    )
}

fn block_page_server_is_running() -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], 80));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(200)) else {
        return false;
    };

    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    if stream
        .write_all(
            b"GET /__blocksite_health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .is_err()
    {
        return false;
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).is_ok() && response.contains(BLOCK_PAGE_HEALTH_TOKEN)
}

fn block_page_server() -> Result<(), String> {
    let listener = TcpListener::bind(BLOCK_PAGE_ADDRESS)
        .map_err(|error| format!("Não foi possível escutar em {BLOCK_PAGE_ADDRESS}: {error}"))?;
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if let Err(error) = serve_block_page(stream) {
                    eprintln!("Erro ao responder página de bloqueio: {error}");
                }
            }
            Err(error) => eprintln!("Erro ao receber conexão da página de bloqueio: {error}"),
        }
    }
    Ok(())
}

fn serve_block_page(mut stream: TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| format!("Não foi possível configurar timeout HTTP: {error}"))?;
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];

    while request.len() < 16 * 1024 {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("Não foi possível ler requisição HTTP: {error}"))?;
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    let request = String::from_utf8_lossy(&request);
    let first_line = request.lines().next().unwrap_or_default();
    let path = first_line.split_whitespace().nth(1).unwrap_or("/");
    let response = if path == "/__blocksite_health" {
        http_response(
            "200 OK",
            "text/plain; charset=utf-8",
            BLOCK_PAGE_HEALTH_TOKEN.to_string(),
        )
    } else {
        let host = request
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("host").then_some(value.trim())
            })
            .filter(|host| !host.is_empty())
            .unwrap_or("site bloqueado");
        let html = BLOCK_PAGE_HTML.replace("{{BLOCKED_SITE}}", &escape_html(host));
        http_response("200 OK", "text/html; charset=utf-8", html)
    };

    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("Não foi possível enviar página de bloqueio: {error}"))
}

fn http_response(status: &str, content_type: &str, body: String) -> String {
    format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn launch_expiry_monitor() -> Result<(), String> {
    let executable =
        env::current_exe().map_err(|e| format!("Não foi possível localizar o app: {e}"))?;
    let mut command = Command::new(executable);
    command
        .arg("expiry-monitor")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Não foi possível iniciar o monitor de expiração: {e}"))
}

fn expiry_monitor() -> Result<(), String> {
    loop {
        let mut config = load_config()?;
        let now = current_epoch()?;

        let install_expired = config
            .install_grant_until
            .is_some_and(|expiry| expiry <= now);
        let settings_expired = config
            .settings_grant_until
            .is_some_and(|expiry| expiry <= now);
        if install_expired || settings_expired {
            if install_expired {
                config.installation_locked = true;
                config.install_grant_until = None;
            }
            if settings_expired {
                config.settings_locked = true;
                config.settings_grant_until = None;
            }
            save_config(&config)?;
            if install_expired {
                apply_installation_lock(&config)?;
            }
            if settings_expired {
                apply_settings_lock(&config)?;
            }
        }

        let next_expiry = [config.install_grant_until, config.settings_grant_until]
            .into_iter()
            .flatten()
            .min();
        let Some(expiry) = next_expiry else {
            return Ok(());
        };
        if expiry <= now {
            continue;
        }
        thread::sleep(Duration::from_secs((expiry - now).min(10)));
    }
}

fn current_epoch() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|e| format!("Relógio do sistema inválido: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_deduplicates_domains() {
        assert_eq!(
            parse_domains("HTTPS://Example.com/,example.com").unwrap(),
            vec!["example.com"]
        );
    }

    #[test]
    fn rejects_invalid_domains() {
        assert!(parse_domains("example.com,not a domain").is_err());
        assert!(parse_domains("example.com;127.0.0.1").is_err());
    }

    #[test]
    fn adult_blocklist_is_enabled_by_default_and_preserves_custom_sites() {
        let config: Config = serde_json::from_str(r#"{"denied_sites":["custom.test"]}"#).unwrap();
        let domains = denied_domains(&config);

        assert!(config.adult_sites_blocked);
        assert!(config.site_blocking_enabled);
        assert!(domains.contains(&"custom.test".to_string()));
        assert!(domains.contains(&"pornhub.com".to_string()));
        assert_eq!(
            domains
                .iter()
                .filter(|domain| *domain == "custom.test")
                .count(),
            1
        );
    }

    #[test]
    fn adult_blocklist_can_be_disabled_without_removing_custom_sites() {
        let config = Config {
            denied_sites: vec!["custom.test".to_string()],
            adult_sites_blocked: false,
            ..Config::default()
        };
        let domains = denied_domains(&config);

        assert_eq!(domains, vec!["custom.test"]);
        assert!(!domains.contains(&"pornhub.com".to_string()));
    }

    #[test]
    fn disabled_site_blocking_produces_no_hosts_entries() {
        let config = Config {
            denied_sites: vec!["custom.test".to_string()],
            site_blocking_enabled: false,
            ..Config::default()
        };

        assert!(denied_domains(&config).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn drains_large_command_output_without_blocking() {
        run_command(
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::Out.Write('x' * 100000)",
            ],
        )
        .unwrap();
    }

    #[test]
    fn refuses_unsafe_allowlist_application() {
        let config = Config {
            lock_mode: LockMode::Allow,
            ..Config::default()
        };
        assert!(apply_hosts(&config).is_err());
    }

    #[test]
    fn strips_only_a_complete_managed_hosts_block() {
        let input =
            format!("before\r\n{HOSTS_BEGIN}\r\n127.0.0.1 bad.test\r\n{HOSTS_END}\r\nafter");
        assert_eq!(
            strip_managed_hosts(&input).unwrap(),
            vec!["before".to_string(), "after".to_string()]
        );
        assert!(
            strip_managed_hosts(&format!("before\n{HOSTS_BEGIN}\n127.0.0.1 bad.test")).is_err()
        );
    }

    #[test]
    fn block_page_escapes_the_requested_host() {
        let host = escape_html("<script>alert('x')</script>");
        let page = BLOCK_PAGE_HTML.replace("{{BLOCKED_SITE}}", &host);
        assert!(page.contains("&lt;script&gt;"));
        assert!(!page.contains("<script>alert"));
    }

    #[test]
    fn block_page_health_response_has_a_stable_marker() {
        let response = http_response(
            "200 OK",
            "text/plain; charset=utf-8",
            BLOCK_PAGE_HEALTH_TOKEN.to_string(),
        );
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains(BLOCK_PAGE_HEALTH_TOKEN));
    }

    #[test]
    fn serves_block_page_for_http_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            serve_block_page(stream).unwrap();
        });

        let mut client = TcpStream::connect(address).unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: blocked.example\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        server.join().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Este site está bloqueado"));
        assert!(response.contains("blocked.example"));
    }
}
