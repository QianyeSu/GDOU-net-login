use crate::config::AppConfig;
use anyhow::{anyhow, bail, Context, Result};
use base64::alphabet::Alphabet;
use base64::engine::{Engine, GeneralPurpose, GeneralPurposeConfig};
use hmac::{Hmac, Mac};
use md5::Md5;
use regex::Regex;
use reqwest::redirect::Policy;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacMd5 = Hmac<Md5>;

const BASE64_ALPHABET: &str = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";

#[derive(Debug, Clone)]
pub struct SrunClient {
    config: AppConfig,
    http: reqwest::Client,
    probe: reqwest::Client,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct LoginState {
    pub error: String,
    pub online_ip: Option<IpAddr>,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub error_msg: Option<String>,
    #[serde(default)]
    pub res: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChallengeResponse {
    challenge: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PortalResponse {
    error: String,
    #[serde(default)]
    error_msg: String,
    #[serde(default)]
    res: String,
    #[serde(default)]
    ecode: Option<String>,
    #[serde(default)]
    suc_msg: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PortalProbe {
    pub portal_url: Option<String>,
    pub ac_id: Option<u32>,
    pub user_ip: Option<IpAddr>,
}

impl SrunClient {
    pub fn new(config: AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .context("failed to build http client")?;
        let probe = reqwest::Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_millis(1800))
            .build()
            .context("failed to build probe client")?;
        Ok(Self {
            config,
            http,
            probe,
        })
    }

    pub async fn login(&self, password: &str) -> Result<String> {
        let detected = self.probe_portal_fast().await.unwrap_or_default();
        let state = self.get_login_state_with_probe(&detected).await.ok();
        if matches!(state.as_ref().map(|s| s.error.as_str()), Some("ok")) {
            return Ok("already online".to_string());
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
        let state = self.get_login_state().await?;
        if state.error != "ok" {
            return Ok("already offline".to_string());
        }

        let ip = state.online_ip.unwrap_or_else(|| {
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
            Err(_) => {
                let resp = self.probe.get(&self.config.probe_url).send().await;
                match resp {
                    Ok(r) => Ok(matches!(r.status().as_u16(), 200 | 204)),
                    Err(_) => Ok(false),
                }
            }
        }
    }

    pub async fn query_acid(&self) -> Result<Option<u32>> {
        Ok(self.probe_portal().await?.ac_id)
    }

    pub async fn probe_portal(&self) -> Result<PortalProbe> {
        self.probe_portal_fast().await
    }

    async fn probe_portal_fast(&self) -> Result<PortalProbe> {
        let mut detected = PortalProbe::default();

        let targets = [
            "http://connectivitycheck.gstatic.com/generate_204",
            "http://neverssl.com/",
            "http://8.8.8.8/",
        ];
        let probes = targets.map(|target| self.probe_target(target));
        let results = futures::future::join_all(probes).await;

        for probe in results.into_iter().flatten() {
            merge_probe(&mut detected, probe);
            if probe_has_identity(&detected) {
                return Ok(detected);
            }
        }

        Ok(detected)
    }

    async fn probe_target(&self, target: &str) -> Option<PortalProbe> {
        let resp = match self.probe.get(target).send().await {
            Ok(resp) => resp,
            Err(err) => {
                tracing::debug!("portal probe failed for {target}: {err:#}");
                return None;
            }
        };

        let mut detected = PortalProbe::default();
        if let Some(location) = resp.headers().get(reqwest::header::LOCATION) {
            if let Ok(loc) = location.to_str() {
                merge_probe_text(&mut detected, loc);
            }
        }

        let body = resp.text().await.unwrap_or_default();
        merge_probe_text(&mut detected, &body);
        Some(detected)
    }

    pub async fn get_login_state(&self) -> Result<LoginState> {
        let probe = self.probe_portal_fast().await.unwrap_or_default();
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
        let body = strip_jsonp(&raw);
        let parsed: LoginState =
            serde_json::from_str(body).context("failed to parse login state")?;
        Ok(parsed)
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
        let sock = UdpSocket::bind("0.0.0.0:0").context("failed to bind udp socket")?;
        sock.connect("8.8.8.8:80")
            .context("failed to infer outbound ip")?;
        let addr = sock.local_addr().context("failed to read local addr")?;
        match addr {
            SocketAddr::V4(v4) => Ok(IpAddr::V4(*v4.ip())),
            SocketAddr::V6(_) => Err(anyhow!("ipv6 is not supported for this portal")),
        }
    }

    fn resolve_user_ip(&self) -> Result<IpAddr> {
        match self.config.user_ip {
            Some(ip) => Ok(ip),
            None => self.local_ip(),
        }
    }

    async fn resolve_login_context(&self, probe: &PortalProbe) -> Result<(String, u32, IpAddr)> {
        let portal_url = self.resolve_portal_url_with_probe(probe).await?;

        let ac_id = self
            .config
            .ac_id
            .or(probe.ac_id)
            .context("failed to auto detect ac_id; paste the current portal URL or fill ac_id manually")?;
        let user_ip = match self.config.user_ip.or(probe.user_ip) {
            Some(ip) => ip,
            None => self.local_ip()?,
        };

        Ok((portal_url, ac_id, user_ip))
    }

    async fn resolve_portal_url(&self) -> Result<String> {
        let probe = self.probe_portal_fast().await.unwrap_or_default();
        self.resolve_portal_url_with_probe(&probe).await
    }

    async fn resolve_portal_url_with_probe(&self, probe: &PortalProbe) -> Result<String> {
        let configured = self.config.portal_url.trim();
        if !configured.is_empty() {
            return Ok(configured.to_string());
        }

        if let Some(portal_url) = probe.portal_url.clone() {
            let parsed = reqwest::Url::parse(&portal_url).context("invalid detected portal url")?;
            let host = parsed.host_str().context("detected portal url missing host")?;
            let mut base = format!("{}://{}", parsed.scheme(), host);
            if let Some(port) = parsed.port() {
                base.push(':');
                base.push_str(&port.to_string());
            }
            return Ok(base);
        }

        bail!("failed to auto detect portal url; paste the current portal URL in advanced settings");
    }
}

fn parse_portal_response(text: &str) -> Result<PortalResponse> {
    let body = strip_jsonp(text);
    let parsed =
        serde_json::from_str::<PortalResponse>(body).context("failed to parse portal response")?;
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
    let re = Regex::new(r#"https?://[^\s'"<>]+srun_portal_success[^\s'"<>]*"#).ok()?;
    re.find(text).map(|m| m.as_str().replace("&amp;", "&"))
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
    probe.ac_id.is_some() && probe.user_ip.is_some()
}

fn debug_response(kind: &str, raw: &str) {
    tracing::debug!(target: "gdou-net-login", "{} response: {}", kind, raw);
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
