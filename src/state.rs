use geph5_misc_rpc::client_control::{ConnInfo, NewsItem};
use isocountry::CountryCode;
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::{Arc, Mutex};
use tui_textarea::TextArea;

#[derive(Clone)]
pub struct LogWriter {
    pub logs: Arc<Mutex<Vec<String>>>,
}

impl io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        let mut logs = self.logs.lock().unwrap();
        logs.extend(s.lines().map(|l| l.to_string()));
        if logs.len() > 1000 {
            let overflow = logs.len() - 1000;
            logs.drain(0..overflow);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[derive(PartialEq)]
pub enum TabIdx {
    Status,
    Nodes,
    Config,
    Debug,
    Plus,
}

#[derive(PartialEq)]
pub enum Focus {
    None,
    Secret,
    SocksPort,
    HttpPort,
    RedeemCode,
    PromoCode,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuiPrefs {
    pub secret: String,
    pub socks_port: String,
    pub http_port: String,
    pub listen_all: bool,
    pub enable_debug_log: bool,
    pub allow_direct: bool,
    pub selected_country: Option<String>,
    pub last_known_level: Option<String>,
    pub last_connected_secret: Option<String>,
}

impl TuiPrefs {
    fn path() -> std::path::PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("geph5_tui_prefs.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if let Ok(b) = std::fs::read(&path) {
            if let Ok(p) = serde_json::from_slice(&b) {
                return p;
            }
        }
        Self {
            socks_port: "9909".into(),
            http_port: "9910".into(),
            ..Default::default()
        }
    }

    pub fn save(&self) {
        let _ = std::fs::write(
            Self::path(),
            serde_json::to_string(self).unwrap_or_default(),
        );
    }
}

pub struct AppState<'a> {
    pub tab: TabIdx,
    pub is_running: bool,
    pub conn_info: ConnInfo,

    // Config
    pub secret_textarea: TextArea<'a>,
    pub socks_textarea: TextArea<'a>,
    pub http_textarea: TextArea<'a>,
    pub listen_all: bool,
    pub allow_direct: bool,

    // Nodes
    pub countries: Vec<CountryCode>,
    pub node_list_state: ListState,
    pub selected_country: Option<String>,
    pub switch_in_progress: bool,
    pub last_switch_target: Option<String>,
    pub switch_started_at: Option<std::time::Instant>,

    pub level_notice: Option<String>,
    pub news_items: Vec<NewsItem>,
    pub needs_cache_clear: bool,
    pub last_detected_level: geph5_broker_protocol::AccountLevel,
    pub plus_expires_days: Option<f64>,
    pub last_connected_secret: Option<String>,

    pub focus: Focus,
    pub registration_status: String,
    pub registration_idx: Option<usize>,

    pub debug_logs: Arc<Mutex<Vec<String>>>,
    pub debug_scroll: u16,
    pub debug_auto_scroll: bool,
    pub enable_debug_log: bool,

    pub status_scroll: u16,

    pub update_info: Option<(String, std::path::PathBuf)>,

    // Plus tab (session-only, not persisted)
    pub redeem_textarea: TextArea<'a>,
    pub promo_textarea: TextArea<'a>,
    pub plus_action_status: String,
    pub plus_action_in_progress: bool,
    pub plus_url_to_show: Option<String>,
    pub selected_price_idx: usize,
    pub selected_method_idx: usize,
    pub price_points: Vec<(u32, u32)>, // (days, cents)
    pub payment_methods: Vec<String>,
    pub poll_plus_until: Option<std::time::Instant>,
    pub poll_plus_prev_expires: Option<u64>, // previous plus_expires_unix to detect advance
}

impl<'a> AppState<'a> {
    pub fn new(debug_logs: Arc<Mutex<Vec<String>>>) -> Self {
        let prefs = TuiPrefs::load();

        let mut secret = TextArea::default();
        secret.insert_str(&prefs.secret);

        let mut socks = TextArea::default();
        socks.insert_str(&prefs.socks_port);

        let mut http = TextArea::default();
        http.insert_str(&prefs.http_port);

        Self {
            tab: TabIdx::Status,
            is_running: false,
            conn_info: ConnInfo::Disconnected,

            secret_textarea: secret,
            socks_textarea: socks,
            http_textarea: http,
            listen_all: prefs.listen_all,
            allow_direct: prefs.allow_direct,

            countries: vec![],
            node_list_state: ListState::default(),
            selected_country: prefs.selected_country.clone(),
            switch_in_progress: false,
            last_switch_target: prefs.selected_country.clone(),
            switch_started_at: None,

            level_notice: None,
            news_items: vec![],
            needs_cache_clear: false,
            last_detected_level: match prefs.last_known_level.as_deref() {
                Some("Plus") => geph5_broker_protocol::AccountLevel::Plus,
                _ => geph5_broker_protocol::AccountLevel::Free,
            },
            plus_expires_days: None,
            last_connected_secret: prefs.last_connected_secret.clone(),

            focus: Focus::None,
            registration_status: String::new(),
            registration_idx: None,

            debug_logs,
            debug_scroll: 0,
            debug_auto_scroll: true,
            enable_debug_log: prefs.enable_debug_log,

            status_scroll: 0,

            update_info: None,

            redeem_textarea: TextArea::default(),
            promo_textarea: TextArea::default(),
            plus_action_status: String::new(),
            plus_action_in_progress: false,
            plus_url_to_show: None,
            selected_price_idx: 0,
            selected_method_idx: 0,
            price_points: Vec::new(),
            payment_methods: Vec::new(),
            poll_plus_until: None,
            poll_plus_prev_expires: None,
        }
    }

    pub fn to_prefs(&self) -> TuiPrefs {
        TuiPrefs {
            secret: self.secret_textarea.lines().join(""),
            socks_port: self.socks_textarea.lines().join(""),
            http_port: self.http_textarea.lines().join(""),
            listen_all: self.listen_all,
            enable_debug_log: self.enable_debug_log,
            allow_direct: self.allow_direct,
            selected_country: self.selected_country.clone(),
            last_known_level: Some(match self.last_detected_level {
                geph5_broker_protocol::AccountLevel::Plus => "Plus".to_string(),
                geph5_broker_protocol::AccountLevel::Free => "Free".to_string(),
            }),
            last_connected_secret: self.last_connected_secret.clone(),
        }
    }

    pub fn sync_prefs(&self) {
        self.to_prefs().save();
    }
}
