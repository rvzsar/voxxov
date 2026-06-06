//! Конвертация нашего `ProxyConfig` в CLI-аргументы `yt-dlp`.
//!
//! Используем `args.push("--proxy", url)` + `args.push("--no-proxy", list)`,
//! т.к. `DownloaderBuilder::with_proxy` есть в крейте `yt-dlp`, но
//! чтобы не зависеть от его точной сигнатуры — просто эмулируем через
//! args. URL percent-encode делает `url::Url` (см. `ProxyConfig::to_ytdlp_arg`).

use crate::config::{ProxyConfig, ProxyKind};

/// CLI-аргументы, которые нужно прокинуть в `Downloader::append_args`.
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

/// Возвращает URL с percent-encoded user/password, или `None` если
/// прокси не настроен / хост пустой.
fn proxy_url(cfg: &ProxyConfig) -> Option<String> {
    use url::Url;

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
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        if !u.is_empty() {
            // set_username/set_password сами percent-кодируют.
            let _ = url.set_username(Some(u));
            let _ = url.set_password(Some(p));
        }
    }
    Some(url.as_str().to_string())
}

fn no_proxy_list(_cfg: &ProxyConfig) -> Option<Vec<String>> {
    // TODO: yt-dlp CLI не имеет `--no-proxy` флага. NO_PROXY env
    // обрабатывается libcurl/reqwest. Пока — no-op; cfg.no_proxy
    // применяется на уровне reqwest через крейт `yt-dlp` (если
    // DownloaderBuilder::with_proxy поддерживает no_proxy list в
    // будущих версиях).
    None
}
