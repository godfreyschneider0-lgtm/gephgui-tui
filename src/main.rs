use crossterm::{
    event::Event,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    widgets::{Block, Borders},
    Terminal,
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod autoupdate;
mod daemon;
mod event;
mod state;
mod ui;

use daemon::{daemon_running, start_daemon, stop_daemon, DaemonRpcTransport};
use geph5_misc_rpc::client_control::{ConnInfo, ControlClient};
use state::{AppState, LogWriter, TuiPrefs};

fn main() -> anyhow::Result<()> {
    unsafe {
        std::env::remove_var("http_proxy");
        std::env::remove_var("https_proxy");
        std::env::remove_var("HTTP_PROXY");
        std::env::remove_var("HTTPS_PROXY");
    }

    let args = std::env::args().collect::<Vec<_>>();
    match args.get(1).map(|s| s.as_str()) {
        Some("-h") | Some("--help") => {
            println!(
                "geph-tui {} — lightweight Geph5 client (TUI + headless daemon)\n",
                env!("CARGO_PKG_VERSION")
            );
            println!("USAGE:");
            println!("    geph-tui [MODE] [ARGS]\n");
            println!("MODES:");
            println!("    (none)            Interactive TUI (default). Configure account, region,");
            println!("                      ports; press 's' to connect, 'q' to quit.");
            println!("    --ctl <cmd>       Control the daemon without UI.");
            println!("                      Commands:");
            println!("                    start, stop, status   — basic daemon control");
            println!("                    switch [<cc|auto>] [--immediate]  — hot-swap exit (--immediate skips drain)");
            println!("                    exit                  — show current exit constraint");
            println!("                    exits                 — list available exits");
            println!("                    sessions              — show active+draining sessions");
            println!("                    logs [N]              — last N log lines (default 20)");
            println!("                    account               — show account info");
            println!("                    redeem <code>         — redeem a voucher code to extend Plus");
            println!("    -h, --help        Show this help.\n");
            println!("TUI KEYS:");
            println!("    1-5   tabs (Status / Regions / Config / Debug / Plus)");
            println!("    s/x   start / stop connection");
            println!("    e     edit Account ID      p  edit SOCKS5 port      h  edit HTTP port");
            println!(
                "    l     toggle listen-all      b  toggle direct/bridged"
            );
            println!("    r     register a new account");
            println!("    q     quit");
            return Ok(());
        }
        _ => {}
    }
    if let Some("--ctl") = args.get(1).map(|s| s.as_str()) {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .try_init();

        let subcmd = args.get(2).map(|s| s.as_str()).unwrap_or("status");
        smol::block_on(async {
            match subcmd {
                "start" => ctl_start().await,
                "stop" => ctl_stop().await,
                "status" => ctl_status().await,
                "switch" => {
                    let country = args.get(3).filter(|s| *s != "--immediate").map(|s| s.as_str());
                    let immediate = args.iter().skip(3).any(|s| s == "--immediate");
                    ctl_switch(country, immediate).await
                }
                "exit" => ctl_exit_constraint().await,
                "exits" => ctl_exits().await,
                "sessions" => ctl_sessions().await,
                "logs" => ctl_logs(args.get(3).map(|s| s.as_str())).await,
                "account" => ctl_account().await,
                "redeem" => ctl_redeem(args.get(3).map(|s| s.as_str())).await,
                _ => {
                    eprintln!("Usage: geph-tui --ctl <command> [args]");
                    eprintln!();
                    eprintln!("Commands:");
                    eprintln!("    start, stop, status   basic daemon control");
                    eprintln!("    switch [<cc|auto>]    hot-swap exit (existing TCP drains)");
                    eprintln!("    exit                  show current exit constraint");
                    eprintln!("    exits                 list available exits");
                    eprintln!("    sessions              show active+draining sessions");
                    eprintln!("    logs [N]              last N log lines (default 20)");
                    eprintln!("    account               show account info");
                    eprintln!("    redeem <code>         redeem a voucher code to extend Plus");
                    std::process::exit(1);
                }
            }
        })?;
        return Ok(());
    }

    // DO NOT run the autoupdate logic on flatpak, but otherwise it's good
    if std::env::var("FLATPAK_ID").is_err() {
        smolscale::spawn(autoupdate::download_update_loop()).detach();
    }

    let debug_logs = Arc::new(Mutex::new(Vec::new()));
    let writer = LogWriter {
        logs: debug_logs.clone(),
    };

    // Ensure terminal output is completely clean
    let _ = tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false) // Disable ANSI color codes which might mess up TUI if any leak
        .try_init();

    smolscale::block_on(async {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Clear screen explicitly before running
        terminal.clear()?;

        let res = run_app(&mut terminal, debug_logs).await;

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            println!("{:?}", err)
        }

        anyhow::Ok(())
    })?;

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    debug_logs: Arc<Mutex<Vec<String>>>,
) -> anyhow::Result<()> {
    let mut state = AppState::new(debug_logs);
    state
        .secret_textarea
        .set_block(Block::default().borders(Borders::ALL).title("Login Secret"));
    state
        .socks_textarea
        .set_block(Block::default().borders(Borders::ALL).title("SOCKS5 Port"));
    state.http_textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("HTTP Proxy Port"),
    );
    state
        .redeem_textarea
        .set_block(Block::default().borders(Borders::ALL).title("Redeem Code"));
    state.promo_textarea.set_block(
        Block::default()
            .borders(Borders::ALL)
            .title("Promo Code (optional)"),
    );

    let prefs = state.to_prefs();
    if !prefs.secret.is_empty() && !daemon_running().await {
        let _ = start_daemon(&prefs).await;
    }

    loop {
        state.update_info = autoupdate::get_cached_update();

        // Fetch status
        state.is_running = daemon_running().await;
        if state.is_running {
            if let Ok(info) = ControlClient(DaemonRpcTransport).conn_info().await {
                state.conn_info = info;
            }
        } else {
            state.conn_info = ConnInfo::Disconnected;
        }

        if state.switch_in_progress {
            if let Some(started) = state.switch_started_at {
                if started.elapsed() >= Duration::from_secs(3) {
                    state.switch_in_progress = false;
                }
            } else {
                state.switch_in_progress = false;
            }
        }

        let mut user_level = state.last_detected_level;
        state.poll_plus_prev_expires = state
            .plus_expires_days
            .map(|d| (d.round() as i64).max(0) as u64);
        if state.is_running {
            let secret = state.secret_textarea.lines().join("");
            if !secret.is_empty() {
                let cred = geph5_broker_protocol::Credential::Secret(secret);
                let cred_val = serde_json::to_value(&cred).unwrap_or(serde_json::Value::Null);
                if let Ok(Ok(ui_val)) = ControlClient(DaemonRpcTransport)
                    .broker_rpc("get_user_info_by_cred".into(), vec![cred_val])
                    .await
                {
                    if let Ok(Some(ui)) =
                        serde_json::from_value::<Option<geph5_broker_protocol::UserInfo>>(ui_val)
                    {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        state.plus_expires_days = ui.plus_expires_unix.map(|exp| {
                            let secs = exp.saturating_sub(now);
                            secs as f64 / 86400.0
                        });
                        if ui.plus_expires_unix.unwrap_or(0) > now {
                            user_level = geph5_broker_protocol::AccountLevel::Plus;
                        } else {
                            user_level = geph5_broker_protocol::AccountLevel::Free;
                        }
                    }
                }
            }

            if user_level != state.last_detected_level {
                state.needs_cache_clear = true;
                state.level_notice = Some(match user_level {
                    geph5_broker_protocol::AccountLevel::Plus => {
                        "VIP detected! Press x then s to reconnect with VIP access.".into()
                    }
                    geph5_broker_protocol::AccountLevel::Free => {
                        "VIP expired. Press x then s to reconnect.".into()
                    }
                });
                state.last_detected_level = user_level;
            }
        }

        if let Some(deadline) = state.poll_plus_until {
            if deadline.elapsed() < Duration::from_secs(300) {
                let new_days = state
                    .plus_expires_days
                    .map(|d| (d.round() as i64).max(0) as u64)
                    .unwrap_or(0);
                if new_days > state.poll_plus_prev_expires.unwrap_or(0) {
                    state.plus_action_status = "Plus subscription extended! \u{2713}".into();
                    state.poll_plus_until = None;
                }
            } else {
                state.poll_plus_until = None;
            }
        }

        if state.tab == state::TabIdx::Plus && state.is_running {
            if state.price_points.is_empty() {
                if let Ok(Ok(val)) = ControlClient(DaemonRpcTransport)
                    .broker_rpc("raw_price_points".into(), vec![])
                    .await
                {
                    if let Ok(pp) = serde_json::from_value::<Vec<(u32, u32)>>(val) {
                        state.price_points = pp;
                    }
                }
            }
            if state.payment_methods.is_empty() {
                if let Ok(Ok(val)) = ControlClient(DaemonRpcTransport)
                    .broker_rpc("payment_methods".into(), vec![])
                    .await
                {
                    if let Ok(pm) = serde_json::from_value::<Vec<String>>(val) {
                        state.payment_methods = pm;
                    }
                }
            }
        }

        if let Ok(Ok(ns)) = ControlClient(DaemonRpcTransport).net_status().await {
            let mut seen = std::collections::HashSet::new();
            state.countries = ns
                .exits
                .into_iter()
                .filter_map(|(_, (_, desc, meta))| {
                    if !meta.allowed_levels.contains(&user_level) {
                        return None;
                    }
                    let cc = desc.country;
                    if seen.insert(cc.alpha2().to_string()) {
                        Some(cc)
                    } else {
                        None
                    }
                })
                .collect();
            state.countries.sort_by_key(|c| c.alpha2().to_string());
        }

        if state.news_items.is_empty() && state.is_running {
            let lang = if sys_locale::get_locale()
                .unwrap_or_default()
                .contains("zh")
            {
                "zh"
            } else {
                "en"
            };
            if let Ok(Ok(news)) =
                ControlClient(DaemonRpcTransport).latest_news(lang.to_string()).await
            {
                state.news_items = news;
            }
        }

        // Poll registration
        if let Some(idx) = state.registration_idx {
            match ControlClient(DaemonRpcTransport)
                .poll_registration(idx)
                .await
            {
                Ok(Ok(prog)) => {
                    if let Some(sec) = prog.secret {
                        state.secret_textarea = tui_textarea::TextArea::default();
                        state.secret_textarea.insert_str(&sec);
                        state.secret_textarea.set_block(
                            Block::default().borders(Borders::ALL).title("Login Secret"),
                        );
                        state.registration_status = "Registration complete!".into();
                        state.registration_idx = None;
                        state.needs_cache_clear = true;
                        state.last_detected_level = geph5_broker_protocol::AccountLevel::Free;
                        state.plus_expires_days = None;
                        state.level_notice = if state.is_running {
                            Some("New account registered. Press 'x' then 's' to reconnect.".into())
                        } else {
                            None
                        };
                        state.sync_prefs();
                    } else {
                        state.registration_status =
                            format!("Registering... {:.1}%", prog.progress * 100.0);
                    }
                }
                Ok(Err(msg)) => {
                    state.registration_status = format!("Registration failed: {}", msg);
                    state.registration_idx = None;
                }
                Err(_) => {
                    if !state.is_running {
                        state.registration_status = "Daemon stopped during registration.".into();
                        state.registration_idx = None;
                    }
                }
            }
        }

        terminal.draw(|f| ui::draw_ui(f, &mut state))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            let ev = crossterm::event::read()?;

            if state.focus != state::Focus::None {
                if let Event::Key(key) = ev {
                    event::handle_focused_input(&mut state, key);
                }
                continue;
            }

            if let Event::Key(key) = ev {
                if event::handle_global_key(&mut state, key).await {
                    return Ok(());
                }
            }
            state.sync_prefs();
        }
    }
}

async fn ctl_start() -> anyhow::Result<()> {
    if daemon_running().await {
        println!("already running");
        return Ok(());
    }
    let prefs = TuiPrefs::load();
    if prefs.secret.is_empty() {
        eprintln!("no account configured");
        std::process::exit(1);
    }
    if prefs.last_connected_secret.as_deref() != Some(prefs.secret.as_str()) {
        daemon::clear_conn_token_cache();
        let mut saved = prefs.clone();
        saved.last_connected_secret = Some(prefs.secret.clone());
        saved.save();
    }
    start_daemon(&prefs).await?;
    println!("started");
    Ok(())
}

async fn ctl_stop() -> anyhow::Result<()> {
    if !daemon_running().await {
        println!("not running");
        return Ok(());
    }
    stop_daemon().await?;
    println!("stopped");
    Ok(())
}

async fn ctl_status() -> anyhow::Result<()> {
    let prefs = TuiPrefs::load();
    let socks5 = prefs.socks_port;
    let http = prefs.http_port;
    let listen_addr = if prefs.listen_all {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    if !daemon_running().await {
        println!("running=no");
        println!("socks5_port={socks5}");
        println!("http_port={http}");
        println!("listen_addr={listen_addr}");
        return Ok(());
    }

    let conn = ControlClient(DaemonRpcTransport)
        .conn_info()
        .await
        .unwrap_or(ConnInfo::Disconnected);

    let state = match &conn {
        ConnInfo::Connected { .. } => "connected",
        ConnInfo::Connecting => "connecting",
        ConnInfo::Disconnected => "disconnected",
    };

    println!("running=yes");
    println!("conn={state}");
    println!("socks5_port={socks5}");
    println!("http_port={http}");
    println!("listen_addr={listen_addr}");

    if let ConnInfo::Connected { sessions } = &conn {
        if let Some(s) = sessions.first() {
            let cc = &s.exit.country;
            println!("exit={} ({})", cc.name(), cc.alpha2());
        }
    }

    Ok(())
}

async fn require_daemon() -> anyhow::Result<()> {
    if !daemon_running().await {
        anyhow::bail!("daemon not running; start it with `geph-tui --ctl start`");
    }
    Ok(())
}

fn fmt_constraint(c: &geph5_broker_protocol::ExitConstraint) -> String {
    use geph5_broker_protocol::ExitConstraint;
    match c {
        ExitConstraint::Auto => "auto".to_string(),
        ExitConstraint::Country(cc) => cc.alpha2().to_lowercase(),
        ExitConstraint::CountryCity(cc, city) => {
            format!("{}-{}", cc.alpha2().to_lowercase(), city)
        }
        ExitConstraint::Direct(s) => s.clone(),
        ExitConstraint::Hostname(s) => s.clone(),
    }
}

async fn ctl_switch(arg: Option<&str>, immediate: bool) -> anyhow::Result<()> {
    require_daemon().await?;
    let cc = arg.unwrap_or("auto");
    let constraint = if cc.eq_ignore_ascii_case("auto") {
        geph5_broker_protocol::ExitConstraint::Auto
    } else {
        geph5_broker_protocol::ExitConstraint::Country(
            isocountry::CountryCode::for_alpha2_caseless(cc)
                .map_err(|e| anyhow::anyhow!("bad country code {cc}: {e:?}"))?,
        )
    };
    let human = fmt_constraint(&constraint);
    ControlClient(DaemonRpcTransport)
        .set_exit_constraint(constraint)
        .await
        .map_err(|e| anyhow::anyhow!("set_exit_constraint RPC transport error: {e:?}"))?
        .map_err(|e| anyhow::anyhow!("set_exit_constraint rejected by daemon: {e}"))?;
    if immediate {
        let killed = ControlClient(DaemonRpcTransport)
            .kill_stale_sessions()
            .await
            .map_err(|e| anyhow::anyhow!("kill_stale_sessions RPC transport error: {e:?}"))?
            .map_err(|e| anyhow::anyhow!("kill_stale_sessions rejected by daemon: {e}"))?;
        println!("Switched to {human}. Killed {killed} stale sessions. No drain period.");
    } else {
        println!("Switched to {human}. Existing TCP connections will drain.");
    }
    Ok(())
}

async fn ctl_exit_constraint() -> anyhow::Result<()> {
    require_daemon().await?;
    let constraint = ControlClient(DaemonRpcTransport)
        .current_exit_constraint()
        .await
        .map_err(|e| anyhow::anyhow!("current_exit_constraint RPC error: {e:?}"))?;
    println!("{}", fmt_constraint(&constraint));
    Ok(())
}

async fn ctl_exits() -> anyhow::Result<()> {
    require_daemon().await?;
    let ns = ControlClient(DaemonRpcTransport)
        .net_status()
        .await
        .map_err(|e| anyhow::anyhow!("net_status RPC transport error: {e:?}"))?
        .map_err(|e| anyhow::anyhow!("net_status rejected by daemon: {e}"))?;
    let mut rows: Vec<(geph5_broker_protocol::ExitDescriptor, geph5_broker_protocol::ExitMetadata)> =
        ns.exits.into_values().map(|(_, desc, meta)| (desc, meta)).collect();
    rows.sort_by(|a, b| {
        (a.0.country.alpha2(), &a.0.city).cmp(&(b.0.country.alpha2(), &b.0.city))
    });
    let plus = geph5_broker_protocol::AccountLevel::Plus;
    for (desc, meta) in rows {
        let tier = if meta.allowed_levels.contains(&plus) {
            "plus"
        } else {
            "free"
        };
        println!(
            "{}  {:<18} {:>5.0}%  {}",
            desc.country.alpha2().to_uppercase(),
            desc.city,
            desc.load * 100.0,
            tier,
        );
    }
    Ok(())
}

async fn ctl_sessions() -> anyhow::Result<()> {
    require_daemon().await?;
    let conn = ControlClient(DaemonRpcTransport)
        .conn_info()
        .await
        .map_err(|e| anyhow::anyhow!("conn_info RPC error: {e:?}"))?;
    match conn {
        ConnInfo::Disconnected => println!("Disconnected"),
        ConnInfo::Connecting => println!("Connecting"),
        ConnInfo::Connected { sessions } => {
            let mut groups: std::collections::BTreeMap<String, Vec<&geph5_misc_rpc::client_control::ConnectedInfo>> =
                std::collections::BTreeMap::new();
            for s in &sessions {
                groups
                    .entry(s.exit.country.alpha2().to_string())
                    .or_default()
                    .push(s);
            }
            for (cc, sess) in groups {
                println!("Exit {cc} ({} sessions):", sess.len());
                for s in sess {
                    let bridge = s
                        .bridge
                        .map(|b| b.to_string())
                        .unwrap_or_else(|| "none".to_string());
                    println!("  protocol={} city={} bridge={}", s.protocol, s.exit.city, bridge);
                }
            }
        }
    }
    Ok(())
}

async fn ctl_logs(arg: Option<&str>) -> anyhow::Result<()> {
    require_daemon().await?;
    let n = match arg {
        Some(a) => match a.parse::<usize>() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("Usage: geph-tui --ctl logs [N]    (N must be a positive integer)");
                std::process::exit(2);
            }
        },
        None => 20,
    };
    let logs = ControlClient(DaemonRpcTransport)
        .recent_logs()
        .await
        .map_err(|e| anyhow::anyhow!("recent_logs RPC error: {e:?}"))?;
    let start = logs.len().saturating_sub(n);
    for line in &logs[start..] {
        println!("{}", line);
    }
    Ok(())
}

async fn ctl_account() -> anyhow::Result<()> {
    require_daemon().await?;
    let prefs = TuiPrefs::load();
    if prefs.secret.is_empty() {
        anyhow::bail!("no account configured");
    }
    let cred = geph5_broker_protocol::Credential::Secret(prefs.secret.clone());
    let cred_val = serde_json::to_value(&cred).unwrap_or(serde_json::Value::Null);
    let ui_val = ControlClient(DaemonRpcTransport)
        .broker_rpc("get_user_info_by_cred".into(), vec![cred_val])
        .await
        .map_err(|e| anyhow::anyhow!("broker_rpc transport error: {e:?}"))?
        .map_err(|e| anyhow::anyhow!("broker_rpc rejected by daemon: {e}"))?;
    let ui = serde_json::from_value::<Option<geph5_broker_protocol::UserInfo>>(ui_val)
        .map_err(|e| anyhow::anyhow!("failed to deserialize UserInfo: {e}"))?;
    let ui = match ui {
        Some(ui) => ui,
        None => {
            println!("user_id=<none>");
            println!("plus=not plus");
            return Ok(());
        }
    };
    println!("user_id={}", ui.user_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match ui.plus_expires_unix {
        Some(exp) if exp > now => {
            let days = (exp - now) as f64 / 86400.0;
            println!("plus={} ({:.1} days remaining)", exp, days);
        }
        _ => println!("plus=not plus"),
    }
    if let Some(bw) = ui.bw_consumption {
        println!("bw_used_mb={}", bw.mb_used);
        println!("bw_limit_mb={}", bw.mb_limit);
    }
    Ok(())
}

async fn ctl_redeem(code_arg: Option<&str>) -> anyhow::Result<()> {
    require_daemon().await?;
    let code = match code_arg {
        Some(c) if !c.is_empty() => c.to_string(),
        _ => {
            eprintln!("Usage: geph-tui --ctl redeem <code>");
            std::process::exit(2);
        }
    };
    let prefs = TuiPrefs::load();
    if prefs.secret.is_empty() {
        anyhow::bail!("no account configured; start the TUI and log in first");
    }
    let url_val = ControlClient(DaemonRpcTransport)
        .broker_rpc(
            "redeem_voucher".into(),
            vec![serde_json::json!(prefs.secret), serde_json::json!(code)],
        )
        .await
        .map_err(|e| anyhow::anyhow!("broker_rpc transport error: {e:?}"))?
        .map_err(|e| anyhow::anyhow!("redeem rejected: {e}"))?;
    let days: i32 = serde_json::from_value(url_val)
        .map_err(|e| anyhow::anyhow!("failed to deserialize days (i32): {e}"))?;
    println!("Redeemed successfully. {} days added.", days);
    let cred = geph5_broker_protocol::Credential::Secret(prefs.secret.clone());
    let cred_val = serde_json::to_value(&cred).unwrap_or(serde_json::Value::Null);
    if let Ok(Ok(ui_val)) = ControlClient(DaemonRpcTransport)
        .broker_rpc("get_user_info_by_cred".into(), vec![cred_val])
        .await
    {
        if let Ok(Some(ui)) =
            serde_json::from_value::<Option<geph5_broker_protocol::UserInfo>>(ui_val)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            match ui.plus_expires_unix {
                Some(exp) if exp > now => {
                    let days_left = (exp - now) as f64 / 86400.0;
                    println!("Plus now expires in {:.1} days.", days_left);
                }
                _ => println!("Plus status: not plus"),
            }
        }
    }
    Ok(())
}
