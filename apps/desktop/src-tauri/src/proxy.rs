//! CLI-args для yt-dlp из `ProxyConfig`.
//!
//! Зовём `yt-dlp.exe` напрямую, поэтому прокси передаётся через `--proxy`.
//! User/password percent-кодирует `reqwest::Url`.

use crate::config::{ProxyConfig, ProxyKind};

/// Args для командной строки yt-dlp.
pub fn to_args(cfg: &ProxyConfig) -> Vec<String> {
    let mut args = Vec::new();

    if !matches!(cfg.kind, ProxyKind::None) {
        if let Some(url) = proxy_url(cfg) {
            args.push("--proxy".to_string());
            args.push(url);
        }
    }

    args
}

/// URL с percent-encoded user/password, или `None` если прокси не настроен.
fn proxy_url(cfg: &ProxyConfig) -> Option<String> {
    use reqwest::Url;

    let host = cfg.host.as_deref()?.trim();
    if host.is_empty() {
        return None;
    }
    let port = cfg.port.unwrap_or(match cfg.kind {
        ProxyKind::Http | ProxyKind::Https => 8080,
        ProxyKind::Socks5 => 1080,
        ProxyKind::None => 0,
    });
    let scheme = match cfg.kind {
        ProxyKind::Http => "http",
        ProxyKind::Https => "https",
        ProxyKind::Socks5 => "socks5",
        ProxyKind::None => unreachable!(),
    };

    let mut url = Url::parse(&format!("{scheme}://{host}:{port}")).ok()?;
    if let (Some(u), Some(p)) = (cfg.username.as_deref(), cfg.password.as_deref()) {
        if !u.is_empty() {
            // set_username/set_password сами percent-кодируют.
            let _ = url.set_username(u);
            let _ = url.set_password(Some(p));
        }
    }
    Some(url.as_str().to_string())
}
