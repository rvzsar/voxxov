//! Маппинг нашего `ProxyConfig` в `yt_dlp::client::proxy::ProxyConfig`.
//!
//! Сам proxy теперь устанавливается на уровне `DownloaderBuilder` через
//! `.with_proxy(...)` (см. `ytdlp.rs`). Этот модуль остаётся только как
//! тонкий конвертер.

use crate::config::{ProxyConfig, ProxyKind};
use url::Url;

pub fn to_ytdlp_proxy(cfg: &ProxyConfig) -> Option<yt_dlp::client::proxy::ProxyConfig> {
    use yt_dlp::client::proxy::{ProxyConfig as YtProxy, ProxyType};

    let kind = match cfg.kind {
        ProxyKind::None => return None,
        ProxyKind::Http => ProxyType::Http,
        ProxyKind::Https => ProxyType::Https,
        ProxyKind::Socks5 => ProxyType::Socks5,
    };

    let host = cfg.host.as_deref()?.trim();
    if host.is_empty() {
        return None;
    }
    let port = cfg.port.unwrap_or(match cfg.kind {
        ProxyKind::Http | ProxyKind::Https => 8080,
        ProxyKind::Socks5 => 1080,
        ProxyKind::None => 0,
    });

    // Сборка URL через `url::Url` — percent-encode для user/password.
    let mut url = match Url::parse(&format!(
        "{}://{}:{}",
        match cfg.kind {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5",
            ProxyKind::None => unreachable!(),
        },
        host,
        port
    )) {
        Ok(u) => u,
        Err(_) => return None,
    };
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        if !u.is_empty() {
            let _ = url.set_username(Some(u));
            let _ = url.set_password(Some(p));
        }
    }

    let mut yp = YtProxy::new(kind, url.as_str());
    if let Some(np) = &cfg.no_proxy {
        if !np.is_empty() {
            let list: Vec<String> =
                np.split(',').map(|s| s.trim().to_string()).collect();
            yp = yp.with_no_proxy(list);
        }
    }
    Some(yp)
}
