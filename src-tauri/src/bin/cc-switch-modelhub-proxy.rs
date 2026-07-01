use anyhow::{bail, Context, Result};
use cc_switch_lib::{
    set_current_provider, AppState, AppType, Database, Provider, ProviderMeta, ProxyConfig,
};
use serde_json::json;
use std::{env, fs, path::PathBuf, sync::Arc, time::Duration};

const DEFAULT_LISTEN: &str = "127.0.0.1:15721";
const DEFAULT_MODELHUB_ROOT: &str = "https://aidp.bytedance.net/api/modelhub/online";
const MODEL_PLACEHOLDER: &str = "modelhub-dynamic";
const MODEL_PROVIDER_ID: &str = "modelhub";
const PROVIDER_ID: &str = "modelhub-azure-chat";
const PROVIDER_NAME: &str = "ModelHub Codex";
const PROVIDER_TYPE: &str = "modelhub_codex";

#[derive(Debug)]
enum Command {
    SetupProvider(SetupProviderOptions),
    Serve(ServeOptions),
}

#[derive(Debug)]
struct SetupProviderOptions {
    state_home: Option<PathBuf>,
    listen_host: String,
    listen_port: u16,
    modelhub_root_url: String,
}

#[derive(Debug)]
struct ServeOptions {
    state_home: Option<PathBuf>,
}

fn parse_command() -> Result<Command> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        return Ok(Command::Serve(ServeOptions { state_home: None }));
    }

    match args[0].as_str() {
        "--help" | "-h" | "help" => {
            print_usage();
            std::process::exit(0);
        }
        "setup-provider" => parse_setup_provider_args(&args[1..]).map(Command::SetupProvider),
        "serve" => parse_serve_args(&args[1..]).map(Command::Serve),
        arg if arg.starts_with('-') => parse_serve_args(&args).map(Command::Serve),
        arg => bail!("unknown subcommand: {arg}"),
    }
}

fn parse_setup_provider_args(args: &[String]) -> Result<SetupProviderOptions> {
    let mut index = 0;
    let mut listen = DEFAULT_LISTEN.to_string();
    let mut modelhub_root_url = DEFAULT_MODELHUB_ROOT.to_string();
    let mut state_home: Option<PathBuf> = None;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--help" | "-h" => {
                print_setup_provider_usage();
                std::process::exit(0);
            }
            "--base-url" | "--modelhub-root" => {
                modelhub_root_url = take_value(args, &mut index, arg)?;
            }
            "--listen" => {
                listen = take_value(args, &mut index, arg)?;
            }
            "--state-home" => {
                state_home = Some(expand_path(take_value(args, &mut index, arg)?));
            }
            _ if arg.starts_with("--base-url=") => {
                modelhub_root_url = value_after_equals(arg);
            }
            _ if arg.starts_with("--modelhub-root=") => {
                modelhub_root_url = value_after_equals(arg);
            }
            _ if arg.starts_with("--listen=") => {
                listen = value_after_equals(arg);
            }
            _ if arg.starts_with("--state-home=") => {
                state_home = Some(expand_path(value_after_equals(arg)));
            }
            _ => bail!("unknown setup-provider argument: {arg}"),
        }
        index += 1;
    }

    let (listen_host, listen_port) = parse_listen(&listen)?;
    Ok(SetupProviderOptions {
        state_home,
        listen_host,
        listen_port,
        modelhub_root_url: modelhub_root_url.trim_end_matches('/').to_string(),
    })
}

fn parse_serve_args(args: &[String]) -> Result<ServeOptions> {
    let mut index = 0;
    let mut state_home: Option<PathBuf> = None;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--help" | "-h" => {
                print_serve_usage();
                std::process::exit(0);
            }
            "--state-home" => {
                state_home = Some(expand_path(take_value(args, &mut index, arg)?));
            }
            _ if arg.starts_with("--state-home=") => {
                state_home = Some(expand_path(value_after_equals(arg)));
            }
            _ => bail!("unknown serve argument: {arg}"),
        }
        index += 1;
    }

    Ok(ServeOptions { state_home })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .with_context(|| format!("missing value for {flag}"))
}

fn value_after_equals(arg: &str) -> String {
    arg.split_once('=')
        .map(|(_, value)| value.to_string())
        .unwrap_or_default()
}

fn parse_listen(raw: &str) -> Result<(String, u16)> {
    let (host, port) = raw
        .rsplit_once(':')
        .with_context(|| format!("--listen must be HOST:PORT, got {raw}"))?;
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        bail!("--listen host cannot be empty");
    }
    let port = port
        .parse::<u16>()
        .with_context(|| format!("invalid --listen port in {raw}"))?;
    Ok((host.to_string(), port))
}

fn expand_path(raw: String) -> PathBuf {
    expand_pathbuf(PathBuf::from(raw))
}

fn expand_pathbuf(path: PathBuf) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path;
    };
    if raw == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path
}

fn print_usage() {
    eprintln!(
        "Usage:\n\
  cc-switch-modelhub-proxy setup-provider [options]\n\
  cc-switch-modelhub-proxy serve [options]\n\
\n\
Commands:\n\
  setup-provider  Seed the fixed cc-switch ModelHub Codex provider and proxy listen config\n\
  serve           Start the local proxy from cc-switch state\n\
\n\
Codex config.toml is intentionally managed outside this tool.\n\
\n\
Run `cc-switch-modelhub-proxy <command> --help` for command options."
    );
}

fn print_setup_provider_usage() {
    eprintln!(
        "Usage: cc-switch-modelhub-proxy setup-provider [options]\n\
\n\
Options:\n\
  --base-url URL        ModelHub root URL (default: {DEFAULT_MODELHUB_ROOT})\n\
  --modelhub-root URL   Alias for --base-url\n\
  --listen HOST:PORT    Local proxy listen address (default: {DEFAULT_LISTEN})\n\
  --state-home PATH     Isolated cc-switch state home for settings/db\n\
\n\
This command writes only cc-switch provider/runtime state. It does not modify Codex config.toml."
    );
}

fn print_serve_usage() {
    eprintln!(
        "Usage: cc-switch-modelhub-proxy serve [options]\n\
\n\
Options:\n\
  --state-home PATH     Isolated cc-switch state home for settings/db\n\
\n\
Run setup-provider once before serve. Codex config.toml must already point to the local /v1 proxy."
    );
}

fn apply_state_home(state_home: Option<&PathBuf>) -> Result<()> {
    if let Some(state_home) = state_home {
        fs::create_dir_all(state_home)
            .with_context(|| format!("create state home {}", state_home.display()))?;
        env::set_var("CC_SWITCH_TEST_HOME", state_home);
    }
    Ok(())
}

fn connect_host_for_url(host: &str) -> String {
    let connect_host = match host {
        "0.0.0.0" => "127.0.0.1",
        "::" => "::1",
        other => other,
    };
    if connect_host.contains(':') && !connect_host.starts_with('[') {
        format!("[{connect_host}]")
    } else {
        connect_host.to_string()
    }
}

fn codex_base_url(host: &str, port: u16) -> String {
    format!("http://{}:{port}/v1", connect_host_for_url(host))
}

fn provider_config(modelhub_root_url: &str) -> String {
    format!(
        r#"model = "{MODEL_PLACEHOLDER}"
model_provider = "{MODEL_PROVIDER_ID}"
sandbox_mode = "danger-full-access"
approval_policy = "on-request"
model_reasoning_effort = "xhigh"
model_max_output_tokens = 64000

[model_providers.{MODEL_PROVIDER_ID}]
name = "ModelHub"
base_url = "{modelhub_root_url}"
wire_api = "responses"
request_max_retries = 50
retry_429 = true
stream_max_retries = 50
"#
    )
}

fn seed_provider(db: &Database, modelhub_root_url: &str) -> Result<()> {
    let mut provider = Provider::with_id(
        PROVIDER_ID.to_string(),
        PROVIDER_NAME.to_string(),
        json!({
            "auth": {
                "OPENAI_API_KEY": ""
            },
            "base_url": modelhub_root_url,
            "modelhubRootUrl": modelhub_root_url,
            "model": MODEL_PLACEHOLDER,
            "config": provider_config(modelhub_root_url),
            "modelCatalog": {
                "models": [{
                    "model": MODEL_PLACEHOLDER,
                    "name": MODEL_PLACEHOLDER,
                    "displayName": "ModelHub dynamic request model"
                }]
            }
        }),
        None,
    );
    provider.category = Some("third-party".to_string());
    provider.icon = Some("openai".to_string());
    provider.meta = Some(ProviderMeta {
        provider_type: Some(PROVIDER_TYPE.to_string()),
        codex_chat_reasoning: Some(serde_json::from_value(json!({
            "supportsThinking": false,
            "supportsEffort": false,
            "thinkingParam": "none",
            "effortParam": "none"
        }))?),
        ..ProviderMeta::default()
    });

    db.save_provider("codex", &provider)
        .context("save modelhub provider")?;
    db.set_current_provider("codex", PROVIDER_ID)
        .context("set current Codex provider in database")?;
    set_current_provider(&AppType::Codex, Some(PROVIDER_ID))
        .context("set current Codex provider in settings")?;
    Ok(())
}

async fn update_proxy_config(db: &Database, options: &SetupProviderOptions) -> Result<()> {
    let mut config: ProxyConfig = db.get_proxy_config().await.context("get proxy config")?;
    config.listen_address = options.listen_host.clone();
    config.listen_port = options.listen_port;
    db.update_proxy_config(config)
        .await
        .context("update proxy config")
}

async fn setup_provider(options: SetupProviderOptions) -> Result<()> {
    apply_state_home(options.state_home.as_ref())?;
    let db = Arc::new(Database::init().context("database init")?);
    update_proxy_config(&db, &options).await?;
    seed_provider(&db, &options.modelhub_root_url)?;

    println!("cc-switch ModelHub provider configured");
    println!("provider_id={PROVIDER_ID}");
    println!("provider_type={PROVIDER_TYPE}");
    println!("modelhub_root={}", options.modelhub_root_url);
    println!(
        "codex_base_url={}",
        codex_base_url(&options.listen_host, options.listen_port)
    );
    println!("non_gpt_endpoint={}/v2/crawl", options.modelhub_root_url);
    println!("gpt_endpoint={}/responses", options.modelhub_root_url);
    println!("codex_config=external");
    Ok(())
}

fn ensure_provider_ready(db: &Database) -> Result<()> {
    let provider = db
        .get_provider_by_id(PROVIDER_ID, "codex")
        .context("read modelhub provider")?;
    if provider.is_none() {
        bail!("ModelHub provider is not configured. Run `cc-switch-modelhub-proxy setup-provider` first.");
    }

    let current = db
        .get_current_provider("codex")
        .context("read current Codex provider")?;
    if current.as_deref() != Some(PROVIDER_ID) {
        bail!("Current Codex provider is not {PROVIDER_ID}. Run `cc-switch-modelhub-proxy setup-provider` first.");
    }

    Ok(())
}

async fn serve(options: ServeOptions) -> Result<()> {
    apply_state_home(options.state_home.as_ref())?;
    let _ = rustls::crypto::ring::default_provider().install_default();

    let db = Arc::new(Database::init().context("database init")?);
    ensure_provider_ready(&db)?;

    let state = AppState::new(db);
    let info = state
        .proxy_service
        .start()
        .await
        .map_err(anyhow::Error::msg)?;
    let status = state
        .proxy_service
        .get_status()
        .await
        .map_err(anyhow::Error::msg)?;

    println!("cc-switch ModelHub proxy ready");
    println!("pid={}", std::process::id());
    println!("listen=http://{}:{}", info.address, info.port);
    println!(
        "codex_base_url=http://{}:{}/v1",
        status.address, status.port
    );
    println!("provider_id={PROVIDER_ID}");
    println!("provider_type={PROVIDER_TYPE}");
    println!("codex_config=external");

    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    match parse_command()? {
        Command::SetupProvider(options) => setup_provider(options).await,
        Command::Serve(options) => serve(options).await,
    }
}
