//! Fail-fast checks before `asteroids::run` so a bogus `SDXR_PROTECTED_PANEL` never
//! connects to a live Stardust server (no hang, no session touch).

use std::path::{Path, PathBuf};

/// Why XR accept must not call `Client::connect` / `run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightError {
    MissingEnv,
    MalformedEnv(String),
    ProviderMissing(PathBuf),
    ServerMissing(PathBuf),
}

impl std::fmt::Display for PreflightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnv => write!(f, "SDXR_PROTECTED_PANEL unset"),
            Self::MalformedEnv(s) => write!(f, "SDXR_PROTECTED_PANEL malformed: {s}"),
            Self::ProviderMissing(p) => write!(f, "protected-panel provider socket missing: {}", p.display()),
            Self::ServerMissing(p) => write!(f, "stardust-server socket missing: {}", p.display()),
        }
    }
}

impl std::error::Error for PreflightError {}

/// Parse `path:{name}:{secret}` and return `(name, secret)`.
pub fn parse_protected_panel_env(raw: &str) -> Result<(&str, &str), PreflightError> {
    let mut parts = raw.split(':');
    let proto = parts.next().unwrap_or("");
    if proto != "path" {
        return Err(PreflightError::MalformedEnv(format!(
            "expected path:name:secret, got protocol {proto:?}"
        )));
    }
    let name = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| {
        PreflightError::MalformedEnv("missing provider name".into())
    })?;
    let secret = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| {
        PreflightError::MalformedEnv("missing secret".into())
    })?;
    // reject trailing junk that would confuse the provider client
    if parts.next().is_some() {
        return Err(PreflightError::MalformedEnv(
            "extra ':' segments (expected path:name:secret)".into(),
        ));
    }
    Ok((name, secret))
}

/// `$XDG_RUNTIME_DIR/stardust-protected-panel-provider/{name}` (or `runtime_dir` override).
pub fn provider_socket_path(runtime_dir: &Path, name: &str) -> PathBuf {
    runtime_dir
        .join("stardust-protected-panel-provider")
        .join(name)
}

/// First server socket path the fusion client would try (`stardust-server/stardust-0`).
pub fn server_socket_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("stardust-server").join("stardust-0")
}

/// Ready to call `run` only when env parses and both provider + server sockets exist.
/// Existence only — we do not connect (connecting would consume the oneshot / join the session).
pub fn preflight_xr(env: Option<&str>, runtime_dir: &Path) -> Result<PathBuf, PreflightError> {
    let raw = env.ok_or(PreflightError::MissingEnv)?;
    let (name, _secret) = parse_protected_panel_env(raw)?;
    let provider = provider_socket_path(runtime_dir, name);
    if !provider.exists() {
        return Err(PreflightError::ProviderMissing(provider));
    }
    let server = server_socket_path(runtime_dir);
    if !server.exists() {
        return Err(PreflightError::ServerMissing(server));
    }
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_path_name_secret() {
        assert_eq!(
            parse_protected_panel_env("path:abc:sekrit").unwrap(),
            ("abc", "sekrit")
        );
        assert!(matches!(
            parse_protected_panel_env("nope"),
            Err(PreflightError::MalformedEnv(_))
        ));
        assert!(matches!(
            parse_protected_panel_env("path:only"),
            Err(PreflightError::MalformedEnv(_))
        ));
        assert!(matches!(
            parse_protected_panel_env("path:a:b:c"),
            Err(PreflightError::MalformedEnv(_))
        ));
    }

    #[test]
    fn preflight_fails_fast_when_provider_absent() {
        let dir = tempfile::tempdir().unwrap();
        let err = preflight_xr(Some("path:ghost:secret"), dir.path()).unwrap_err();
        assert!(matches!(err, PreflightError::ProviderMissing(_)), "{err}");
    }

    #[test]
    fn preflight_ok_when_sockets_exist() {
        let dir = tempfile::tempdir().unwrap();
        let provider_dir = dir.path().join("stardust-protected-panel-provider");
        fs::create_dir_all(&provider_dir).unwrap();
        fs::write(provider_dir.join("live"), b"").unwrap();
        let server_dir = dir.path().join("stardust-server");
        fs::create_dir_all(&server_dir).unwrap();
        fs::write(server_dir.join("stardust-0"), b"").unwrap();
        let p = preflight_xr(Some("path:live:sekrit"), dir.path()).unwrap();
        assert_eq!(p, provider_dir.join("live"));
    }
}
