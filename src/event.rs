use crossterm::event::{KeyCode, KeyEvent};
use geph5_misc_rpc::client_control::ControlClient;
use ratatui::style::{Color, Style};

use crate::daemon::{self, DaemonRpcTransport};
use crate::state::{AppState, Focus, TabIdx};

pub fn handle_focused_input<'a>(state: &mut AppState<'a>, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => {
            state.focus = Focus::None;
            state.secret_textarea.set_style(Style::default());
            state.socks_textarea.set_style(Style::default());
            state.http_textarea.set_style(Style::default());
            state.redeem_textarea.set_style(Style::default());
            state.promo_textarea.set_style(Style::default());
        }
        _ => match state.focus {
            Focus::Secret => {
                state.secret_textarea.input(key);
            }
            Focus::SocksPort => {
                state.socks_textarea.input(key);
            }
            Focus::HttpPort => {
                state.http_textarea.input(key);
            }
            Focus::RedeemCode => {
                state.redeem_textarea.input(key);
            }
            Focus::PromoCode => {
                state.promo_textarea.input(key);
            }
            _ => {}
        },
    }
}

pub async fn handle_global_key<'a>(state: &mut AppState<'a>, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => {
            return true;
        }
        KeyCode::Char('1') => state.tab = TabIdx::Status,
        KeyCode::Char('2') => state.tab = TabIdx::Nodes,
        KeyCode::Char('3') => state.tab = TabIdx::Config,
        KeyCode::Char('4') => state.tab = TabIdx::Debug,
        KeyCode::Char('5') => state.tab = TabIdx::Plus,

        KeyCode::Char('s') => {
            if !state.is_running {
                let current_secret = state.secret_textarea.lines().join("");
                let secret_changed =
                    state.last_connected_secret.as_deref() != Some(current_secret.as_str());
                if secret_changed || state.needs_cache_clear {
                    daemon::clear_conn_token_cache();
                    state.needs_cache_clear = false;
                    state.level_notice = None;
                }
                state.last_connected_secret = Some(current_secret);
                let prefs = state.to_prefs();
                prefs.save();
                let _ = daemon::start_daemon(&prefs).await;
            }
        }
        KeyCode::Char('x') => {
            if state.is_running {
                let _ = daemon::stop_daemon().await;
            }
        }
        KeyCode::Char('e') if state.tab == TabIdx::Config => {
            state.focus = Focus::Secret;
            state
                .secret_textarea
                .set_style(Style::default().fg(Color::Yellow));
        }
        KeyCode::Char('p') if state.tab == TabIdx::Config => {
            state.focus = Focus::SocksPort;
            state
                .socks_textarea
                .set_style(Style::default().fg(Color::Yellow));
        }
        KeyCode::Char('h') if state.tab == TabIdx::Config => {
            state.focus = Focus::HttpPort;
            state
                .http_textarea
                .set_style(Style::default().fg(Color::Yellow));
        }
        KeyCode::Char('r') if state.tab == TabIdx::Config => {
            if !state.is_running {
                let prefs = state.to_prefs();
                match daemon::start_daemon(&prefs).await {
                    Ok(()) => {
                        state.is_running = daemon::daemon_running().await;
                    }
                    Err(err) => {
                        state.registration_status =
                            format!("Daemon failed to start: {err:#}");
                        return false;
                    }
                }
            }
            match ControlClient(DaemonRpcTransport).start_registration().await {
                Ok(Ok(idx)) => {
                    state.registration_idx = Some(idx);
                    state.registration_status = "Registration started...".into();
                }
                Ok(Err(msg)) => {
                    state.registration_status = format!("Failed to start: {}", msg);
                }
                Err(err) => {
                    state.registration_status = format!("RPC error: {err:#}");
                }
            }
        }

        KeyCode::Char('l') if state.tab == TabIdx::Config => {
            state.listen_all = !state.listen_all;
        }
        KeyCode::Char('b') if state.tab == TabIdx::Config => {
            state.allow_direct = !state.allow_direct;
        }
        KeyCode::Down if state.tab == TabIdx::Nodes => {
            let i = match state.node_list_state.selected() {
                Some(i) => {
                    if i >= state.countries.len().saturating_sub(1) {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            state.node_list_state.select(Some(i));
        }
        KeyCode::Up if state.tab == TabIdx::Nodes => {
            let i = match state.node_list_state.selected() {
                Some(i) => {
                    if i == 0 {
                        state.countries.len().saturating_sub(1)
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            state.node_list_state.select(Some(i));
        }
        KeyCode::Enter if state.tab == TabIdx::Nodes => {
            if let Some(i) = state.node_list_state.selected() {
                if i < state.countries.len() {
                    let new_country = state.countries[i].alpha2().to_string();
                    if state.selected_country.as_deref() != Some(new_country.as_str()) {
                        state.selected_country = Some(new_country.clone());
                        state.last_switch_target = Some(new_country.clone());
                        state.switch_in_progress = true;
                        state.switch_started_at = Some(std::time::Instant::now());
                        state.sync_prefs();
                        smolscale::spawn(async move {
                            match daemon::switch_exit(Some(new_country)).await {
                                Ok(()) => tracing::info!("switch_exit completed"),
                                Err(e) => tracing::warn!(err = %e, "switch_exit failed"),
                            }
                        })
                        .detach();
                    }
                }
            }
        }
        KeyCode::Char('a') if state.tab == TabIdx::Nodes => {
            if state.selected_country.is_some() {
                state.selected_country = None;
                state.last_switch_target = None;
                state.switch_in_progress = true;
                state.switch_started_at = Some(std::time::Instant::now());
                state.sync_prefs();
                smolscale::spawn(async move {
                    match daemon::switch_exit(None).await {
                        Ok(()) => tracing::info!("switch_exit to Auto completed"),
                        Err(e) => tracing::warn!(err = %e, "switch_exit failed"),
                    }
                })
                .detach();
            }
        }
        KeyCode::Char('j') if state.tab == TabIdx::Status => {
            state.status_scroll = state.status_scroll.saturating_add(1);
        }
        KeyCode::Char('k') if state.tab == TabIdx::Status => {
            state.status_scroll = state.status_scroll.saturating_sub(1);
        }
        KeyCode::Up if state.tab == TabIdx::Debug => {
            state.debug_scroll = state.debug_scroll.saturating_sub(1);
            state.debug_auto_scroll = false;
        }
        KeyCode::Down if state.tab == TabIdx::Debug => {
            let max_scroll = state.debug_logs.lock().unwrap().len().saturating_sub(1) as u16;
            state.debug_scroll = std::cmp::min(state.debug_scroll + 1, max_scroll);
            if state.debug_scroll >= max_scroll {
                state.debug_auto_scroll = true;
            }
        }
        KeyCode::Char('d') if state.tab == TabIdx::Debug => {
            state.enable_debug_log = !state.enable_debug_log;
        }

        KeyCode::Char('v') if state.tab == TabIdx::Plus => {
            state.focus = Focus::RedeemCode;
            state
                .redeem_textarea
                .set_style(Style::default().fg(Color::Yellow));
        }
        KeyCode::Char('o') if state.tab == TabIdx::Plus => {
            state.focus = Focus::PromoCode;
            state
                .promo_textarea
                .set_style(Style::default().fg(Color::Yellow));
        }
        KeyCode::Char('c') if state.tab == TabIdx::Plus => {
            state.plus_url_to_show = None;
            state.plus_action_status = "URL cleared.".into();
        }
        KeyCode::Up if state.tab == TabIdx::Plus => {
            if !state.price_points.is_empty() {
                let len = state.price_points.len();
                state.selected_price_idx = (state.selected_price_idx + len - 1) % len;
            }
        }
        KeyCode::Down if state.tab == TabIdx::Plus => {
            if !state.price_points.is_empty() {
                let len = state.price_points.len();
                state.selected_price_idx = (state.selected_price_idx + 1) % len;
            }
        }
        KeyCode::Left if state.tab == TabIdx::Plus => {
            if !state.payment_methods.is_empty() {
                let len = state.payment_methods.len();
                state.selected_method_idx = (state.selected_method_idx + len - 1) % len;
            }
        }
        KeyCode::Right if state.tab == TabIdx::Plus => {
            if !state.payment_methods.is_empty() {
                let len = state.payment_methods.len();
                state.selected_method_idx = (state.selected_method_idx + 1) % len;
            }
        }
        KeyCode::Enter
            if state.tab == TabIdx::Plus
                && state.focus == Focus::None
                && !state.plus_action_in_progress =>
        {
            let secret = state.secret_textarea.lines().join("");
            let code = state.redeem_textarea.lines().join("");
            if secret.is_empty() {
                state.plus_action_status = "Login first.".into();
                return false;
            }
            if code.is_empty() {
                state.plus_action_status = "Enter a redeem code first.".into();
                return false;
            }
            state.plus_action_in_progress = true;
            state.plus_action_status = "Redeeming…".into();
            match ControlClient(DaemonRpcTransport)
                .broker_rpc(
                    "redeem_voucher".into(),
                    vec![serde_json::json!(secret), serde_json::json!(code)],
                )
                .await
            {
                Ok(Ok(val)) => {
                    let days: i32 = serde_json::from_value(val).unwrap_or(0);
                    state.plus_action_status =
                        format!("Redeemed! {} days added. Refreshing…", days);
                    state.plus_url_to_show = None;
                    state.poll_plus_prev_expires = state
                        .plus_expires_days
                        .map(|d| (d.round() as i64).max(0) as u64);
                    state.poll_plus_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
                    state.plus_action_in_progress = false;
                }
                Ok(Err(msg)) => {
                    state.plus_action_status = format!("Redeem failed: {}", msg);
                    state.plus_action_in_progress = false;
                }
                Err(e) => {
                    state.plus_action_status = format!("RPC error: {e:#}");
                    state.plus_action_in_progress = false;
                }
            }
        }
        KeyCode::Char('b')
            if state.tab == TabIdx::Plus
                && state.focus == Focus::None
                && !state.plus_action_in_progress =>
        {
            let secret = state.secret_textarea.lines().join("");
            if secret.is_empty() {
                state.plus_action_status = "Login first.".into();
                return false;
            }
            if state.price_points.is_empty() || state.payment_methods.is_empty() {
                state.plus_action_status = "Loading prices, try again…".into();
                return false;
            }
            let days = state.price_points[state.selected_price_idx].0;
            let method = state.payment_methods[state.selected_method_idx].clone();
            let promo = state.promo_textarea.lines().join("");
            let method_arg = if promo.is_empty() {
                method
            } else {
                format!("{}+++{}", method, promo)
            };
            state.plus_action_in_progress = true;
            state.plus_action_status = "Creating payment URL…".into();
            match ControlClient(DaemonRpcTransport)
                .broker_rpc(
                    "create_payment".into(),
                    vec![
                        serde_json::json!(secret),
                        serde_json::json!(days),
                        serde_json::json!(method_arg),
                    ],
                )
                .await
            {
                Ok(Ok(val)) => {
                    let url: String = serde_json::from_value(val).unwrap_or_default();
                    if url.is_empty() {
                        state.plus_action_status = "Empty URL returned.".into();
                    } else {
                        state.plus_url_to_show = Some(url);
                        state.plus_action_status =
                            "URL ready below — copy into a browser.".into();
                        state.poll_plus_prev_expires = state
                            .plus_expires_days
                            .map(|d| (d.round() as i64).max(0) as u64);
                        state.poll_plus_until = Some(
                            std::time::Instant::now() + std::time::Duration::from_secs(300),
                        );
                    }
                    state.plus_action_in_progress = false;
                }
                Ok(Err(msg)) => {
                    state.plus_action_status = format!("Failed: {}", msg);
                    state.plus_action_in_progress = false;
                }
                Err(e) => {
                    state.plus_action_status = format!("RPC error: {e:#}");
                    state.plus_action_in_progress = false;
                }
            }
        }
        _ => {}
    }
    false
}
