//! Преобразование `ProxyConfig` в yt-dlp CLI-аргументы + env-vars.

use crate::config::{ProxyConfig, ProxyKind};

#[derive(Debug, Clone, Default)]
pub struct ProxyEnv {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn build(cfg: &ProxyConfig) -> ProxyEnv {
    let mut out = ProxyEnv::default();
    if matches!(cfg.kind, ProxyKind::None) {
        if let Some(np) = &cfg.no_proxy {
            if !np.is_empty() {
                out.env.push(("NO_PROXY".to_string(), np.clone()));
                out.env.push(("no_proxy".to_string(), np.clone()));
            }
        }
        return out;
    }

    let host = match &cfg.host {
        Some(h) if !h.is_empty() => h.clone(),
        _ => return out,
    };
    let port = cfg.port.unwrap_or(match cfg.kind {
        ProxyKind::Http => 8080,
        ProxyKind::Https => 8080,
        ProxyKind::Socks5 => 1080,
        ProxyKind::None => 0,
    });
    let mut userinfo = String::new();
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        if !u.is_empty() {
            userinfo = format!("{u}:{p}@");
        }
    }
    let url = format!("{scheme}://{userinfo}{host}:{port}",
                      scheme = match cfg.kind {
                          ProxyKind::Http => "http",
                          ProxyKind::Https => "https",
                          ProxyKind::Socks5 => "socks5",
                          ProxyKind::None => unreachable!(),
                      });

    out.args.push("--proxy".to_string());
    out.args.push(url.clone());

    if let Some(np) = &cfg.no_proxy {
        if !np.is_empty() {
            out.env.push(("NO_PROXY".to_string(), np.clone()));
            out.env.push(("no_proxy".to_string(), np.clone()));
        }
    }
    out
}
