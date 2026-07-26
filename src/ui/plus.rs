use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::state::AppState;

pub fn draw(f: &mut ratatui::Frame, state: &mut AppState<'_>, area: Rect) {
    let is_chinese = sys_locale::get_locale().unwrap_or_default().contains("zh");

    let url_len = if state.plus_url_to_show.is_some() { 5 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(url_len),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(area);

    let is_plus = state.last_detected_level == geph5_broker_protocol::AccountLevel::Plus;
    let days_str = state
        .plus_expires_days
        .map(|d| format!(" ({:.0} days left)", d.ceil()))
        .unwrap_or_default();
    let banner = if is_plus {
        if is_chinese {
            format!("当前为 Plus 用户{}", days_str)
        } else {
            format!("Currently a Plus user{}", days_str)
        }
    } else if is_chinese {
        "当前为 Free 用户".to_string()
    } else {
        "Currently a Free user".to_string()
    };
    let banner_style = if is_plus {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Gray)
    };
    let banner_p = Paragraph::new(Line::from(banner))
        .style(banner_style)
        .block(Block::default().borders(Borders::ALL).title(
            if is_chinese {
                "Plus 状态"
            } else {
                "Current Plus Status"
            },
        ));
    f.render_widget(banner_p, chunks[0]);

    f.render_widget(&state.redeem_textarea, chunks[1]);
    f.render_widget(&state.promo_textarea, chunks[2]);

    let pkg_text = if state.price_points.is_empty() {
        if is_chinese {
            "加载套餐中…".to_string()
        } else {
            "Loading packages…".to_string()
        }
    } else {
        let idx = state.selected_price_idx.min(state.price_points.len() - 1);
        let (days, cents) = state.price_points[idx];
        let price = format!("${:.2}", cents as f64 / 100.0);
        let hint = if is_chinese {
            "(上下键选择)"
        } else {
            "(Up/Down to choose)"
        };
        format!("{} days — {}  {}", days, price, hint)
    };
    let pkg_p = Paragraph::new(pkg_text).block(Block::default().borders(Borders::ALL).title(
        if is_chinese {
            "套餐"
        } else {
            "Package"
        },
    ));
    f.render_widget(pkg_p, chunks[3]);

    let method_text = if state.payment_methods.is_empty() {
        if is_chinese {
            "加载中…".to_string()
        } else {
            "Loading…".to_string()
        }
    } else {
        let idx = state.selected_method_idx.min(state.payment_methods.len() - 1);
        let hint = if is_chinese {
            "(左右键选择)"
        } else {
            "(Left/Right to choose)"
        };
        format!("{}  {}", state.payment_methods[idx], hint)
    };
    let method_p = Paragraph::new(method_text).block(Block::default().borders(Borders::ALL).title(
        if is_chinese {
            "支付方式"
        } else {
            "Payment Method"
        },
    ));
    f.render_widget(method_p, chunks[4]);

    if let Some(url) = &state.plus_url_to_show {
        let url_text = if is_chinese {
            format!(
                "{}\n(请在浏览器中打开此链接完成支付。按 'c' 清除。)",
                url
            )
        } else {
            format!(
                "{}\n(Open this URL in any browser to complete payment. Press 'c' to clear.)",
                url
            )
        };
        let url_p = Paragraph::new(url_text)
            .style(Style::default().fg(Color::Cyan))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(
                if is_chinese {
                    "支付链接"
                } else {
                    "Payment URL — copy into a browser"
                },
            ));
        f.render_widget(url_p, chunks[5]);
    }

    let legend = if is_chinese {
        "v: 编辑兑换码   o: 编辑优惠码   上/下: 套餐   左/右: 方式   Enter: 兑换   b: 生成支付链接   c: 清除链接"
    } else {
        "v: edit redeem code   o: edit promo code   Up/Down: package   Left/Right: method   Enter: redeem   b: buy URL   c: clear URL"
    };
    let status_line = if state.plus_action_in_progress {
        let prefix = if is_chinese { "处理中…" } else { "Working…" };
        format!("{}\n{}\n{}", prefix, state.plus_action_status, legend)
    } else {
        format!("{}\n{}", state.plus_action_status, legend)
    };
    let status_style = if state.plus_action_in_progress {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let status_p = Paragraph::new(status_line)
        .style(status_style)
        .wrap(Wrap { trim: false });
    f.render_widget(status_p, chunks[6]);
}
