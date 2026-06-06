//! Преобразование `ProxyConfig` в yt-dlp CLI-аргументы + env-vars.
//!
//! User/password percent-кодируются (через `url::Url`), чтобы пароль
//! с `@`/`:'/` не ломал URL. Предупреждение: пароль всё равно виден в
//! `ps`/`tasklist`, т.к. передаётся как CLI-аргумент `--proxy`; для
//! production стоит использовать `.netrc` или env-переменные.

use crate::config::{ProxyConfig, ProxyKind};

#[derive(Debug, Clone, Default)]
pub struct ProxyEnv {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn build(cfg: &ProxyConfig) -> ProxyEnv {
    let mut out = ProxyEnv::default();
    let scheme = match cfg.kind {
        ProxyKind::None => return apply_no_proxy(cfg, out),
        ProxyKind::Http => "http",
        ProxyKind::Https => "https",
        ProxyKind::Socks5 => "socks5",
    };

    let host = match &cfg.host {
        Some(h) if !h.is_empty() => h.clone(),
        _ => return apply_no_proxy(cfg, out),
    };
    let port = cfg.port.unwrap_or(match cfg.kind {
        ProxyKind::Http | ProxyKind::Https => 8080,
        ProxyKind::Socks5 => 1080,
        ProxyKind::None => 0,
    });

    // Собираем URL через `url::Url`, чтобы percent-кодировать user/password.
    let mut url = match url::Url::parse(&format!("{scheme}://{host}:{port}")) {
        Ok(u) => u,
        Err(_) => return out, // невалидный хост — отдаём без прокси
    };
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        if !u.is_empty() {
            // set_username/set_password сами percent-кодируют.
            let _ = url.set_username(Some(u));
            let _ = url.set_password(Some(p));
        }
    }
    out.args.push("--proxy".to_string());
    out.args.push(url.as_str().to_string());

    out = apply_no_proxy(cfg, out);
    out
}

fn apply_no_proxy(cfg: &ProxyConfig, mut out: ProxyEnv) -> ProxyEnv {
    if let Some(np) = &cfg.no_proxy {
        if !np.is_empty() {
            out.env.push(("NO_PROXY".to_string(), np.clone()));
            out.env.push(("no_proxy".to_string(), np.clone()));
        }
    }
    out
}
