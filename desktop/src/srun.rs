use crate::config::AppConfig;
use anyhow::{anyhow, bail, Context, Result};
use base64::alphabet::Alphabet;
use base64::engine::{Engine, GeneralPurpose, GeneralPurposeConfig};
use hmac::{Hmac, Mac};
use md5::Md5;
use regex::Regex;
use reqwest::redirect::Policy;
use reqwest::Url;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
use std::process::Command;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

type HmacMd5 = Hmac<Md5>;

const BASE64_ALPHABET: &str = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub struct SrunClient {
    config: AppConfig,
    http: reqwest::Client,
    probe: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct LoginState {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    pub error: String,
    #[serde(default, deserialize_with = "deserialize_optional_ip")]
    pub online_ip: Option<IpAddr>,
    #[serde(default, deserialize_with = "deserialize_optional_lossy_string")]
    pub user_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_lossy_string")]
    pub error_msg: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_lossy_string")]
    pub res: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChallengeResponse {
    challenge: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PortalResponse {
    #[serde(deserialize_with = "deserialize_lossy_string")]
    error: String,
    #[serde(default, deserialize_with = "deserialize_lossy_string_default")]
    error_msg: String,
    #[serde(default, deserialize_with = "deserialize_lossy_string_default")]
    res: String,
    #[serde(default, deserialize_with = "deserialize_optional_lossy_string")]
    ecode: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_lossy_string")]
    suc_msg: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PortalProbe {
    pub portal_url: Option<String>,
    pub ac_id: Option<u32>,
    pub user_ip: Option<IpAddr>,
}

#[derive(Debug, Clone)]
pub struct PortalProbeTrace {
    pub target: String,
    pub status: Option<u16>,
    pub location: Option<String>,
    pub error: Option<String>,
    pub portal_url: Option<String>,
    pub ac_id: Option<u32>,
    pub user_ip: Option<IpAddr>,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkDiagnostics {
    pub system_proxy: Option<String>,
    pub default_route: Option<RouteInfo>,
    pub portal_route: Option<RouteInfo>,
    pub tun_detected: bool,
}

#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub interface: String,
    pub source: Option<IpAddr>,
    pub next_hop: Option<String>,
    pub virtual_route: bool,
}

impl SrunClient {
    pub fn new(config: AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .context("failed to build http client")?;
        let probe = reqwest::Client::builder()
            .redirect(Policy::none())
            .timeout(PROBE_TIMEOUT)
            .build()
            .context("failed to build probe client")?;
        Ok(Self {
            config,
            http,
            probe,
        })
    }

    pub async fn login(&self, password: &str) -> Result<String> {
        let detected = self.probe_portal_if_needed().await.unwrap_or_default();
        let state = self.get_login_state_with_probe(&detected).await.ok();
        if matches!(state.as_ref().map(|s| s.error.as_str()), Some("ok")) {
            return Ok("already online".to_string());
        }
        if detected.portal_url.is_none()
            && self.config.portal_url.trim().is_empty()
            && self.probe_internet_online().await
        {
            return Ok(
                "already online; portal will be detected when authentication page is reachable"
                    .to_string(),
            );
        }

        let (portal_url, ac_id, ip) = self.resolve_login_context(&detected).await?;
        let token = self.get_challenge_at(&portal_url, &ip, ac_id).await?;
        let hmd5 = hmac_md5_hex(password, &token)?;
        let info = format!(
            "{{SRBX1}}{}",
            fkbase64(xencode(
                &serde_json::json!({
                    "username": self.config.username,
                    "password": password,
                    "ip": ip.to_string(),
                    "acid": ac_id.to_string(),
                    "enc_ver": "srun_bx1",
                })
                .to_string(),
                &token
            ))
        );
        let chksum = sha1_hex(&format!(
            "{token}{username}{token}{hmd5}{token}{ac_id}{token}{ip}{token}{n}{token}{ty}{token}{info}",
            token = token,
            username = self.config.username,
            hmd5 = hmd5,
            ac_id = ac_id,
            ip = ip,
            n = self.config.n,
            ty = self.config.login_type,
            info = info
        ));
        let password_encoded = format!("{{MD5}}{}", hmd5);
        let ac_id_str = ac_id.to_string();
        let ip_str = ip.to_string();
        let n_str = self.config.n.to_string();
        let type_str = self.config.login_type.to_string();
        let ts = now_millis();
        let ts_str = ts.to_string();
        let callback = callback_name(ts);
        let params = [
            ("callback", callback.as_str()),
            ("action", "login"),
            ("username", self.config.username.as_str()),
            ("password", password_encoded.as_str()),
            ("os", self.config.os_name.as_str()),
            ("name", self.config.device_name.as_str()),
            ("double_stack", "0"),
            ("chksum", chksum.as_str()),
            ("info", info.as_str()),
            ("ac_id", ac_id_str.as_str()),
            ("ip", ip_str.as_str()),
            ("n", n_str.as_str()),
            ("type", type_str.as_str()),
            ("_", ts_str.as_str()),
        ];
        let url = format!("{}/cgi-bin/srun_portal", portal_url.trim_end_matches('/'));
        let raw = self
            .http
            .get(url)
            .query(&params)
            .send()
            .await
            .context("failed to send login request")?
            .text()
            .await
            .context("failed to read login response")?;

        debug_response("login", &raw);
        let parsed = parse_portal_response(&raw)?;
        if parsed.error != "ok" {
            bail!(format_portal_error(&parsed));
        }
        if !parsed.res.is_empty() {
            return Ok(parsed.res);
        }
        if let Some(msg) = parsed.suc_msg {
            return Ok(msg);
        }
        Ok(raw)
    }

    pub async fn logout(&self, _password: &str) -> Result<String> {
        let state = self.get_login_state().await.ok();
        if matches!(state.as_ref().map(|s| s.error.as_str()), Some("not_online")) {
            return Ok("already offline".to_string());
        }

        let ip = state.and_then(|state| state.online_ip).unwrap_or_else(|| {
            self.resolve_user_ip()
                .unwrap_or(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))
        });
        let ac_id = self.resolve_acid().await?;
        let ip_str = ip.to_string();
        let ac_id_str = ac_id.to_string();
        let ts = now_millis();
        let ts_str = ts.to_string();
        let callback = callback_name(ts);
        let params = [
            ("callback", callback.as_str()),
            ("action", "logout"),
            ("username", self.config.username.as_str()),
            ("ac_id", ac_id_str.as_str()),
            ("ip", ip_str.as_str()),
            ("os", self.config.os_name.as_str()),
            ("name", self.config.device_name.as_str()),
            ("_", ts_str.as_str()),
        ];
        let portal_url = self.resolve_portal_url().await?;
        let url = format!("{}/cgi-bin/srun_portal", portal_url.trim_end_matches('/'));
        let raw = self
            .http
            .get(url)
            .query(&params)
            .send()
            .await
            .context("failed to send logout request")?
            .text()
            .await
            .context("failed to read logout response")?;

        debug_response("logout", &raw);
        let parsed = parse_portal_response(&raw)?;
        if parsed.error != "ok" {
            bail!(format_portal_error(&parsed));
        }
        if parsed.error_msg == "0" || parsed.res == "0" {
            return Ok("logout ok".to_string());
        }
        if !parsed.error_msg.is_empty() {
            return Ok(parsed.error_msg);
        }
        if !parsed.res.is_empty() {
            return Ok(parsed.res);
        }
        Ok(raw)
    }

    pub async fn probe_online(&self) -> Result<bool> {
        match self.get_login_state().await {
            Ok(state) => Ok(state.error == "ok"),
            Err(_) => Ok(self.probe_internet_online().await),
        }
    }

    pub async fn probe_internet_online(&self) -> bool {
        for target in self.online_probe_targets() {
            let (probe, trace) = self.probe_target(&target).await;
            if probe.portal_url.is_none()
                && trace.error.is_none()
                && trace
                    .status
                    .map(|status| (200..400).contains(&status))
                    .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    pub async fn query_acid(&self) -> Result<Option<u32>> {
        if let Some(ac_id) = self.config.ac_id {
            return Ok(Some(ac_id));
        }
        Ok(self.probe_portal().await?.ac_id)
    }

    pub async fn diagnose_challenge(&self) -> Result<String> {
        let detected = self.probe_portal_if_needed().await.unwrap_or_default();
        let (portal_url, ac_id, ip) = self.resolve_login_context(&detected).await?;
        let token = self.get_challenge_at(&portal_url, &ip, ac_id).await?;
        Ok(format!(
            "challenge ok: portal={portal_url}, ac_id={ac_id}, ip={ip}, token_len={}",
            token.len()
        ))
    }

    pub fn network_diagnostics(&self) -> NetworkDiagnostics {
        NetworkDiagnostics {
            system_proxy: system_proxy_status(),
            default_route: route_to("8.8.8.8"),
            portal_route: self.portal_host().and_then(|host| route_to(&host)),
            tun_detected: tun_detected(),
        }
    }

    pub async fn probe_portal(&self) -> Result<PortalProbe> {
        self.probe_portal_fast().await
    }

    pub async fn probe_portal_detailed(&self) -> Result<(PortalProbe, Vec<PortalProbeTrace>)> {
        self.probe_portal_fast_detailed().await
    }

    async fn probe_portal_fast(&self) -> Result<PortalProbe> {
        let (detected, _) = self.probe_portal_fast_detailed().await?;
        Ok(detected)
    }

    async fn probe_portal_fast_detailed(&self) -> Result<(PortalProbe, Vec<PortalProbeTrace>)> {
        let mut detected = PortalProbe::default();
        let srun_targets = self.srun_probe_targets();
        let srun_probes = srun_targets
            .iter()
            .map(|target| self.probe_srun_target(target));
        let srun_results = futures::future::join_all(srun_probes).await;
        let mut traces = Vec::with_capacity(srun_results.len());
        for (probe, trace) in srun_results {
            traces.push(trace);
            merge_probe(&mut detected, probe);
            if probe_has_identity(&detected) {
                return Ok((detected, traces));
            }
        }

        let targets = self.portal_probe_targets();
        for target in targets {
            let (probe, trace) = self.probe_target(&target).await;
            traces.push(trace);
            merge_probe(&mut detected, probe);
            if probe_has_identity(&detected) {
                return Ok((detected, traces));
            }
        }

        Ok((detected, traces))
    }

    fn online_probe_targets(&self) -> Vec<String> {
        let mut targets = Vec::new();
        push_unique_target(&mut targets, self.config.probe_url.trim());
        push_unique_target(
            &mut targets,
            "http://www.msftconnecttest.com/connecttest.txt",
        );
        targets
    }

    fn portal_probe_targets(&self) -> Vec<String> {
        let mut targets = Vec::new();
        push_unique_target(&mut targets, self.config.probe_url.trim());
        for target in [
            "http://192.168.0.1/",
            "http://www.msftconnecttest.com/connecttest.txt",
            "http://neverssl.com/",
        ] {
            push_unique_target(&mut targets, target);
        }
        targets
    }

    fn srun_probe_targets(&self) -> Vec<String> {
        let mut origins = Vec::new();
        if let Some(origin) = configured_origin(&self.config.portal_url) {
            push_unique_target(&mut origins, &origin);
        }
        if let Some(IpAddr::V4(ip)) = self.config.user_ip {
            for origin in candidate_origins_from_ip(ip) {
                push_unique_target(&mut origins, &origin);
            }
        }
        if let Ok(IpAddr::V4(ip)) = self.local_ip() {
            for origin in candidate_origins_from_ip(ip) {
                push_unique_target(&mut origins, &origin);
            }
        }

        let mut targets = Vec::new();
        for origin in origins {
            let origin = origin.trim_end_matches('/');
            push_unique_target(&mut targets, &format!("{origin}/cgi-bin/rad_user_info"));
            push_unique_target(&mut targets, &format!("{origin}/index_17.html"));
            push_unique_target(&mut targets, &format!("{origin}/index_17"));
            push_unique_target(
                &mut targets,
                &format!("{origin}/srun_portal_success?ac_id=17&theme=pro"),
            );
        }
        targets
    }

    async fn probe_target(&self, target: &str) -> (PortalProbe, PortalProbeTrace) {
        let mut trace = PortalProbeTrace {
            target: target.to_string(),
            status: None,
            location: None,
            error: None,
            portal_url: None,
            ac_id: None,
            user_ip: None,
        };
        if let Err(err) = validate_request_url(target, UrlPurpose::Probe) {
            trace.error = Some(format!("{err:#}"));
            return (PortalProbe::default(), trace);
        }
        let resp = match self.probe.get(target).send().await {
            Ok(resp) => resp,
            Err(err) => {
                tracing::debug!("portal probe failed for {target}: {err:#}");
                trace.error = Some(format!("{err:#}"));
                return (PortalProbe::default(), trace);
            }
        };

        trace.status = Some(resp.status().as_u16());
        let response_url = resp.url().clone();
        let mut detected = PortalProbe::default();
        if let Some(location) = resp.headers().get(reqwest::header::LOCATION) {
            if let Ok(loc) = location.to_str() {
                let resolved = response_url
                    .join(loc)
                    .map(|url| url.to_string())
                    .unwrap_or_else(|_| loc.to_string());
                trace.location = Some(resolved.clone());
                merge_probe_text(&mut detected, &resolved);
            }
        }

        let body = resp.text().await.unwrap_or_default();
        merge_probe_text(&mut detected, &body);
        if detected.portal_url.is_none()
            && (detected.ac_id.is_some()
                || detected.user_ip.is_some()
                || looks_like_srun_portal(&body))
        {
            detected.portal_url = origin_from_url(&response_url);
        }
        trace.portal_url = detected.portal_url.clone();
        trace.ac_id = detected.ac_id;
        trace.user_ip = detected.user_ip;
        (detected, trace)
    }

    async fn probe_srun_target(&self, target: &str) -> (PortalProbe, PortalProbeTrace) {
        let mut trace = PortalProbeTrace {
            target: target.to_string(),
            status: None,
            location: None,
            error: None,
            portal_url: None,
            ac_id: None,
            user_ip: None,
        };
        if let Err(err) = validate_request_url(target, UrlPurpose::Portal) {
            trace.error = Some(format!("{err:#}"));
            return (PortalProbe::default(), trace);
        }
        let resp = match self.probe.get(target).send().await {
            Ok(resp) => resp,
            Err(err) => {
                trace.error = Some(format!("{err:#}"));
                return (PortalProbe::default(), trace);
            }
        };

        trace.status = Some(resp.status().as_u16());
        let response_url = resp.url().clone();
        let body = resp.text().await.unwrap_or_default();
        let mut detected = PortalProbe::default();
        merge_probe_text(&mut detected, response_url.as_str());
        merge_probe_text(&mut detected, &body);
        if looks_like_srun_portal(response_url.as_str())
            || looks_like_srun_portal(&body)
            || detected.ac_id.is_some()
            || detected.user_ip.is_some()
            || response_is_jsonp(&body)
        {
            detected.portal_url = origin_from_url(&response_url);
        }
        if detected.user_ip.is_none() {
            detected.user_ip = self.local_ip().ok();
        }
        trace.portal_url = detected.portal_url.clone();
        trace.ac_id = detected.ac_id;
        trace.user_ip = detected.user_ip;
        (detected, trace)
    }

    pub async fn get_login_state(&self) -> Result<LoginState> {
        let probe = self.probe_portal_if_needed().await.unwrap_or_default();
        self.get_login_state_with_probe(&probe).await
    }

    async fn get_login_state_with_probe(&self, probe: &PortalProbe) -> Result<LoginState> {
        let portal_url = self.resolve_portal_url_with_probe(probe).await?;
        let url = format!("{}/cgi-bin/rad_user_info", portal_url.trim_end_matches('/'));
        let ts = now_millis();
        let ts_str = ts.to_string();
        let callback = callback_name(ts);
        let raw = self
            .http
            .get(url)
            .query(&[("callback", callback.as_str()), ("_", ts_str.as_str())])
            .send()
            .await
            .context("failed to send login state request")?
            .text()
            .await
            .context("failed to read login state response")?;

        debug_response("state", &raw);
        parse_login_state(&raw)
    }

    async fn resolve_acid(&self) -> Result<u32> {
        if let Some(id) = self.config.ac_id {
            return Ok(id);
        }
        if self.config.auto_query_acid {
            match self.probe_portal().await {
                Ok(probe) => {
                    if let Some(id) = probe.ac_id {
                        return Ok(id);
                    }
                    bail!("failed to auto detect ac_id; paste the current portal URL or fill ac_id manually");
                }
                Err(err) => bail!("failed to auto detect ac_id: {err:#}"),
            }
        }
        bail!("ac_id is required when auto detection is disabled")
    }

    async fn get_challenge_at(&self, portal_url: &str, ip: &IpAddr, ac_id: u32) -> Result<String> {
        let url = format!("{}/cgi-bin/get_challenge", portal_url.trim_end_matches('/'));
        let ip_str = ip.to_string();
        let ac_id_str = ac_id.to_string();
        let ts = now_millis();
        let ts_str = ts.to_string();
        let callback = callback_name(ts);
        let raw = self
            .http
            .get(url)
            .query(&[
                ("callback", callback.as_str()),
                ("username", self.config.username.as_str()),
                ("ip", ip_str.as_str()),
                ("ac_id", ac_id_str.as_str()),
                ("_", ts_str.as_str()),
            ])
            .send()
            .await
            .context("failed to send challenge request")?
            .text()
            .await
            .context("failed to read challenge response")?;

        debug_response("challenge", &raw);
        let body = strip_jsonp(&raw);
        let parsed: ChallengeResponse =
            serde_json::from_str(body).context("failed to parse challenge response")?;
        if parsed.challenge.is_empty() {
            bail!("challenge token missing");
        }
        Ok(parsed.challenge)
    }

    pub fn local_ip(&self) -> Result<IpAddr> {
        #[cfg(target_os = "windows")]
        if let Some(ip) = windows_private_ipv4() {
            return Ok(IpAddr::V4(ip));
        }

        let sock = UdpSocket::bind("0.0.0.0:0").context("failed to bind udp socket")?;
        sock.connect("8.8.8.8:80")
            .context("failed to infer outbound ip")?;
        let addr = sock.local_addr().context("failed to read local addr")?;
        match addr {
            SocketAddr::V4(v4) => Ok(IpAddr::V4(*v4.ip())),
            SocketAddr::V6(_) => Err(anyhow!("ipv6 is not supported for this portal")),
        }
    }

    pub fn effective_user_ip(&self) -> Result<IpAddr> {
        self.local_ip()
            .or_else(|_| self.config.user_ip.context("client ip is required"))
    }

    fn resolve_user_ip(&self) -> Result<IpAddr> {
        self.effective_user_ip()
    }

    async fn resolve_login_context(&self, probe: &PortalProbe) -> Result<(String, u32, IpAddr)> {
        let portal_url = self.resolve_portal_url_with_probe(probe).await?;

        let ac_id = self.config.ac_id.or(probe.ac_id).context(
            "failed to auto detect ac_id; paste the current portal URL or fill ac_id manually",
        )?;
        let user_ip = self
            .local_ip()
            .or_else(|_| probe.user_ip.context("probe missing client ip"))
            .or_else(|_| self.config.user_ip.context("client ip is required"))?;

        Ok((portal_url, ac_id, user_ip))
    }

    async fn resolve_portal_url(&self) -> Result<String> {
        let probe = self.probe_portal_if_needed().await.unwrap_or_default();
        self.resolve_portal_url_with_probe(&probe).await
    }

    async fn resolve_portal_url_with_probe(&self, probe: &PortalProbe) -> Result<String> {
        let configured = self.config.portal_url.trim();
        if !configured.is_empty() {
            validate_request_url(configured, UrlPurpose::Portal)?;
            return Ok(configured.to_string());
        }

        if let Some(portal_url) = probe.portal_url.clone() {
            let parsed = reqwest::Url::parse(&portal_url).context("invalid detected portal url")?;
            let host = parsed
                .host_str()
                .context("detected portal url missing host")?;
            let mut base = format!("{}://{}", parsed.scheme(), host);
            if let Some(port) = parsed.port() {
                base.push(':');
                base.push_str(&port.to_string());
            }
            validate_request_url(&base, UrlPurpose::Portal)?;
            return Ok(base);
        }

        bail!(
            "failed to auto detect portal url; paste the current portal URL in advanced settings"
        );
    }

    pub async fn probe_portal_if_needed(&self) -> Result<PortalProbe> {
        if self.has_login_context() {
            return Ok(PortalProbe::default());
        }
        self.probe_portal_fast().await
    }

    fn has_login_context(&self) -> bool {
        !self.config.portal_url.trim().is_empty()
            && self.config.ac_id.is_some()
            && self.config.user_ip.is_some()
    }

    fn portal_host(&self) -> Option<String> {
        configured_origin(&self.config.portal_url).and_then(|origin| {
            Url::parse(&origin)
                .ok()
                .and_then(|url| url.host_str().map(ToString::to_string))
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum UrlPurpose {
    Portal,
    Probe,
}

pub fn validate_request_url(input: &str, purpose: UrlPurpose) -> Result<()> {
    let parsed = Url::parse(input.trim()).context("invalid url")?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => bail!("only http and https urls are allowed"),
    }

    let host = parsed.host_str().context("url missing host")?;
    if is_blocked_host(host, purpose) {
        bail!("unsafe local or link-local address is not allowed");
    }
    Ok(())
}

fn deserialize_lossy_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value_to_string(value).unwrap_or_default())
}

fn deserialize_lossy_string_default<'de, D>(
    deserializer: D,
) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_lossy_string(deserializer)
}

fn deserialize_optional_lossy_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(value_to_string).filter(|s| !s.is_empty()))
}

fn deserialize_optional_ip<'de, D>(deserializer: D) -> std::result::Result<Option<IpAddr>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value
        .and_then(value_to_string)
        .and_then(|text| text.parse::<IpAddr>().ok()))
}

fn value_to_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn parse_portal_response(text: &str) -> Result<PortalResponse> {
    let body = strip_jsonp(text);
    let parsed =
        serde_json::from_str::<PortalResponse>(body).context("failed to parse portal response")?;
    Ok(parsed)
}

fn parse_login_state(text: &str) -> Result<LoginState> {
    let body = strip_jsonp(text);
    let parsed = serde_json::from_str::<LoginState>(body).context("failed to parse login state")?;
    Ok(parsed)
}

fn format_portal_error(parsed: &PortalResponse) -> String {
    let mut parts = vec![format!("login failed: {}", parsed.error)];
    if let Some(ecode) = &parsed.ecode {
        if !ecode.is_empty() && ecode != &parsed.error {
            parts.push(format!("ecode={ecode}"));
        }
    }
    if !parsed.error_msg.is_empty() {
        parts.push(parsed.error_msg.clone());
    }
    if !parsed.res.is_empty() && parsed.res != parsed.error_msg {
        parts.push(parsed.res.clone());
    }
    parts.join("; ")
}

fn strip_jsonp(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(open) = trimmed.find('(') {
        if let Some(close) = trimmed.rfind(')') {
            if close > open {
                return &trimmed[open + 1..close];
            }
        }
    }
    trimmed
}

fn parse_acid_from_text(text: &str) -> Option<u32> {
    let re = Regex::new(r"(?:ac_id=|/index_)(\d+)").ok()?;
    let caps = re.captures(text)?;
    caps.get(1)?.as_str().parse().ok()
}

fn parse_user_ip_from_text(text: &str) -> Option<IpAddr> {
    let re = Regex::new(r"(?:wlanuserip=|user_ip=|userip=)([0-9a-fA-F:\.]+)").ok()?;
    let caps = re.captures(text)?;
    caps.get(1)?.as_str().parse().ok()
}

fn parse_portal_url_from_text(text: &str) -> Option<String> {
    let endpoint_re =
        Regex::new(r#"https?://[^\s'"<>]+(?:srun_portal_success|srun_portal|cgi-bin|get_challenge|rad_user_info|index_\d+)[^\s'"<>]*"#)
            .ok()?;
    if let Some(matched) = endpoint_re.find(text) {
        return Some(matched.as_str().replace("&amp;", "&"));
    }

    let query_re =
        Regex::new(r#"https?://[^\s'"<>]+(?:[?&](?:ac_id|wlanuserip)=)[^\s'"<>]*"#).ok()?;
    query_re
        .find(text)
        .map(|m| m.as_str().replace("&amp;", "&"))
}

fn merge_probe_text(probe: &mut PortalProbe, text: &str) {
    if probe.ac_id.is_none() {
        probe.ac_id = parse_acid_from_text(text);
    }
    if probe.user_ip.is_none() {
        probe.user_ip = parse_user_ip_from_text(text);
    }
    if probe.portal_url.is_none() {
        probe.portal_url = parse_portal_url_from_text(text);
    }
}

fn merge_probe(target: &mut PortalProbe, source: PortalProbe) {
    if target.ac_id.is_none() {
        target.ac_id = source.ac_id;
    }
    if target.user_ip.is_none() {
        target.user_ip = source.user_ip;
    }
    if target.portal_url.is_none() {
        target.portal_url = source.portal_url;
    }
}

fn probe_has_identity(probe: &PortalProbe) -> bool {
    probe.portal_url.is_some() && probe.ac_id.is_some() && probe.user_ip.is_some()
}

fn looks_like_srun_portal(text: &str) -> bool {
    text.contains("srun_portal")
        || text.contains("get_challenge")
        || text.contains("rad_user_info")
        || text.contains("srun_bx1")
        || text.contains("ac_id")
}

fn response_is_jsonp(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("jQuery") && trimmed.contains('(') && trimmed.ends_with(')')
}

fn configured_origin(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Url::parse(trimmed)
        .ok()
        .and_then(|url| origin_from_url(&url))
}

fn is_blocked_host(host: &str, purpose: UrlPurpose) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "0.0.0.0" | "::" | "::1") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(ip) => is_blocked_ipv4(ip, purpose),
            IpAddr::V6(ip) => ip.is_loopback() || ip.is_unspecified(),
        };
    }
    false
}

fn is_blocked_ipv4(ip: Ipv4Addr, purpose: UrlPurpose) -> bool {
    let [a, b, _, _] = ip.octets();
    if a == 127 || a == 0 || a == 169 && b == 254 {
        return true;
    }
    matches!(purpose, UrlPurpose::Portal) && a == 198 && (b == 18 || b == 19)
}

fn candidate_origins_from_ip(ip: Ipv4Addr) -> Vec<String> {
    let [a, b, c, _] = ip.octets();
    let mut origins = Vec::new();
    for candidate in [
        Ipv4Addr::new(a, b, c, 1),
        Ipv4Addr::new(a, b, 0, 1),
        Ipv4Addr::new(a, 129, 1, 1),
        Ipv4Addr::new(a, 129, 0, 1),
        Ipv4Addr::new(a, 0, 0, 1),
    ] {
        push_unique_target(&mut origins, &format!("http://{candidate}"));
    }
    origins
}

fn origin_from_url(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        origin.push(':');
        origin.push_str(&port.to_string());
    }
    Some(origin)
}

#[cfg(target_os = "windows")]
fn windows_private_ipv4() -> Option<Ipv4Addr> {
    let output = Command::new("ipconfig")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"(?i)(?:IPv4[^:\r\n]*:\s*)(\d+\.\d+\.\d+\.\d+)").ok()?;
    let mut addresses = Vec::new();
    for caps in re.captures_iter(&text) {
        if let Some(ip) = caps
            .get(1)
            .and_then(|m| m.as_str().parse::<Ipv4Addr>().ok())
        {
            if is_usable_private_ipv4(ip) {
                addresses.push(ip);
            }
        }
    }
    addresses
        .iter()
        .copied()
        .find(|ip| ip.octets()[0] == 10)
        .or_else(|| addresses.first().copied())
}

#[cfg(target_os = "windows")]
fn is_usable_private_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, _, _] = ip.octets();
    if a == 127 || a == 169 && b == 254 || a == 198 && (b == 18 || b == 19) {
        return false;
    }
    a == 10 || a == 172 && (16..=31).contains(&b) || a == 192 && b == 168
}

fn push_unique_target(targets: &mut Vec<String>, target: &str) {
    let target = target.trim();
    if target.is_empty() {
        return;
    }
    if !targets.iter().any(|existing| existing == target) {
        targets.push(target.to_string());
    }
}

fn debug_response(kind: &str, raw: &str) {
    tracing::debug!(
        target: "gdou-net-login",
        "{} response: {}",
        kind,
        summarize_response(raw)
    );
}

fn summarize_response(raw: &str) -> String {
    let body = strip_jsonp(raw);
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(map)) => {
            let mut parts = Vec::new();
            for key in ["error", "ecode", "res", "error_msg", "suc_msg"] {
                if let Some(value) = map
                    .get(key)
                    .and_then(|value| value_to_string(value.clone()))
                {
                    parts.push(format!("{key}={}", shorten_text(&value, 48)));
                }
            }
            if parts.is_empty() {
                format!("json object keys={}", map.len())
            } else {
                parts.join("; ")
            }
        }
        _ => format!("{} bytes", raw.len()),
    }
}

fn shorten_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

#[cfg(target_os = "windows")]
fn system_proxy_status() -> Option<String> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyServer",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let server = text
        .lines()
        .find(|line| line.contains("ProxyServer"))?
        .split_whitespace()
        .last()?
        .to_string();

    let enabled = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .map(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.contains("0x1")
        })
        .unwrap_or(false);

    enabled.then_some(server)
}

#[cfg(not(target_os = "windows"))]
fn system_proxy_status() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn route_to(target: &str) -> Option<RouteInfo> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Find-NetRoute -RemoteIPAddress '{}' | Select-Object -First 1 IPAddress,InterfaceAlias,NextHop | ConvertTo-Json -Compress",
                target.replace('\'', "''")
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_str(text.trim()).ok()?;
    let interface = value.get("InterfaceAlias")?.as_str()?.to_string();
    let source = value
        .get("IPAddress")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<IpAddr>().ok());
    let next_hop = value
        .get("NextHop")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let virtual_route = is_virtual_route(&interface, source.as_ref(), next_hop.as_deref());
    Some(RouteInfo {
        interface,
        source,
        next_hop,
        virtual_route,
    })
}

#[cfg(not(target_os = "windows"))]
fn route_to(_target: &str) -> Option<RouteInfo> {
    None
}

#[cfg(target_os = "windows")]
fn tun_detected() -> bool {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty Name",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    [
        "tun", "tap", "wintun", "meta", "mihomo", "clash", "sing", "v2ray", "nekoray",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

#[cfg(not(target_os = "windows"))]
fn tun_detected() -> bool {
    false
}

fn is_virtual_route(interface: &str, source: Option<&IpAddr>, next_hop: Option<&str>) -> bool {
    let interface = interface.to_ascii_lowercase();
    let name_matches = [
        "tun", "tap", "wintun", "meta", "mihomo", "clash", "sing", "v2ray", "nekoray",
    ]
    .iter()
    .any(|needle| interface.contains(needle));
    let source_matches = source.is_some_and(is_proxy_reserved_ip);
    let next_hop_matches = next_hop
        .and_then(|value| value.parse::<IpAddr>().ok())
        .as_ref()
        .is_some_and(is_proxy_reserved_ip);
    name_matches || source_matches || next_hop_matches
}

fn is_proxy_reserved_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            a == 198 && (b == 18 || b == 19)
        }
        IpAddr::V6(_) => false,
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn callback_name(timestamp: u128) -> String {
    format!("jQuery1124_{timestamp}")
}

fn hmac_md5_hex(password: &str, token: &str) -> Result<String> {
    let mut mac = HmacMd5::new_from_slice(token.as_bytes()).context("failed to initialize hmac")?;
    mac.update(password.as_bytes());
    Ok(format!("{:x}", mac.finalize().into_bytes()))
}

fn sha1_hex(input: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn xencode(msg: &str, key: &str) -> Vec<u8> {
    if msg.is_empty() {
        return vec![];
    }
    let mut msg = mix(msg.as_bytes(), true);
    let key = mix(key.as_bytes(), false);
    let len = msg.len();
    let last = len - 1;
    let mut right = msg[last];
    let c: u32 = 0x9e37_79b9;
    let mut d: u32 = 0;
    let count = 6 + 52 / len;
    for _ in 0..count {
        d = d.wrapping_add(c);
        let e = d >> 2 & 3;
        for p in 0..=last {
            let left = msg[(p + 1) % len];
            right = ((right >> 5) ^ (left << 2))
                .wrapping_add((left >> 3 ^ right << 4) ^ (d ^ left))
                .wrapping_add(key[(p & 3) ^ e as usize] ^ right)
                .wrapping_add(msg[p]);
            msg[p] = right;
        }
    }
    split(msg, false)
}

fn mix(buffer: &[u8], append_size: bool) -> Vec<u32> {
    let mut res: Vec<u32> = buffer
        .chunks(4)
        .map(|chunk| {
            u32::from_le_bytes(chunk.try_into().unwrap_or_else(|_| {
                let mut last_chunk = [0u8, 0, 0, 0];
                last_chunk[..chunk.len()].copy_from_slice(chunk);
                last_chunk
            }))
        })
        .collect();
    if append_size {
        res.push(buffer.len() as u32);
    }
    res
}

fn split(buffer: Vec<u32>, include_size: bool) -> Vec<u8> {
    let len = buffer.len();
    let size_record = buffer[len - 1];
    if include_size {
        let size = ((len - 1) * 4) as u32;
        if size_record < size.saturating_sub(3) || size_record > size {
            return vec![];
        }
    }

    let mut bytes: Vec<u8> = buffer.iter().flat_map(|i| i.to_le_bytes()).collect();
    if include_size {
        bytes.truncate(size_record as usize);
    }
    bytes
}

fn fkbase64(payload: Vec<u8>) -> String {
    let alphabet = Alphabet::new(BASE64_ALPHABET).expect("invalid base64 alphabet");
    let engine = GeneralPurpose::new(&alphabet, GeneralPurposeConfig::new());
    engine.encode(payload)
}

#[cfg(test)]
mod tests {
    use super::{parse_login_state, parse_portal_response};

    #[test]
    fn parses_numeric_portal_response_fields() {
        let parsed =
            parse_portal_response(r#"callback({"error":"ok","res":0,"error_msg":0})"#).unwrap();

        assert_eq!(parsed.error, "ok");
        assert_eq!(parsed.res, "0");
        assert_eq!(parsed.error_msg, "0");
    }

    #[test]
    fn parses_numeric_login_state_fields() {
        let parsed =
            parse_login_state(r#"callback({"error":"ok","online_ip":"10.0.0.8","res":0})"#)
                .unwrap();

        assert_eq!(parsed.error, "ok");
        assert_eq!(parsed.online_ip.unwrap().to_string(), "10.0.0.8");
        assert_eq!(parsed.res.as_deref(), Some("0"));
    }
}
