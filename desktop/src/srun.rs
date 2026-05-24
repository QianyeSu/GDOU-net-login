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
    error_msg: String,
    res: String,
    #[serde(default)]
    suc_msg: Option<String>,
}

impl SrunClient {
    pub fn new(config: AppConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .context("failed to build http client")?;
        let probe = reqwest::Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(6))
            .build()
            .context("failed to build probe client")?;
        Ok(Self {
            config,
            http,
            probe,
        })
    }

    pub async fn login(&self, password: &str) -> Result<String> {
        let state = self.get_login_state().await.ok();
        if matches!(state.as_ref().map(|s| s.error.as_str()), Some("ok")) {
            return Ok("already online".to_string());
        }

        let ip = self.local_ip()?;
        let ac_id = self.resolve_acid().await?;
        let token = self.get_challenge(&ip, ac_id).await?;
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
        let ts_str = now_millis().to_string();
        let params = [
            ("callback", "jsonp"),
            ("action", "login"),
            ("username", self.config.username.as_str()),
            ("password", password_encoded.as_str()),
            ("chksum", chksum.as_str()),
            ("info", info.as_str()),
            ("ac_id", ac_id_str.as_str()),
            ("ip", ip_str.as_str()),
            ("type", type_str.as_str()),
            ("n", n_str.as_str()),
            ("os", self.config.os_name.as_str()),
            ("name", self.config.device_name.as_str()),
            ("_", ts_str.as_str()),
        ];
        let url = format!(
            "{}/cgi-bin/srun_portal",
            self.config.portal_url.trim_end_matches('/')
        );
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
        if parsed.error != "ok" && !parsed.error_msg.is_empty() {
            return Ok(parsed.error_msg);
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
            self.local_ip()
                .unwrap_or(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))
        });
        let ac_id = self.resolve_acid().await?;
        let ip_str = ip.to_string();
        let ac_id_str = ac_id.to_string();
        let ts_str = now_millis().to_string();
        let params = [
            ("callback", "jsonp"),
            ("action", "logout"),
            ("username", self.config.username.as_str()),
            ("ac_id", ac_id_str.as_str()),
            ("ip", ip_str.as_str()),
            ("os", self.config.os_name.as_str()),
            ("name", self.config.device_name.as_str()),
            ("_", ts_str.as_str()),
        ];
        let url = format!(
            "{}/cgi-bin/srun_portal",
            self.config.portal_url.trim_end_matches('/')
        );
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
        let resp = self
            .probe
            .get("http://8.8.8.8")
            .send()
            .await
            .context("failed to probe portal")?;

        if let Some(location) = resp.headers().get(reqwest::header::LOCATION) {
            if let Ok(loc) = location.to_str() {
                if let Some(ac_id) = parse_acid_from_text(loc) {
                    return Ok(Some(ac_id));
                }
            }
        }

        let body = resp.text().await.unwrap_or_default();
        Ok(parse_acid_from_text(&body))
    }

    pub async fn get_login_state(&self) -> Result<LoginState> {
        let url = format!(
            "{}/cgi-bin/rad_user_info",
            self.config.portal_url.trim_end_matches('/')
        );
        let raw = self
            .http
            .get(url)
            .query(&[("callback", "jsonp")])
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
            match self.query_acid().await {
                Ok(Some(id)) => return Ok(id),
                Ok(None) => tracing::warn!("auto ac_id query returned no result, fallback to 17"),
                Err(err) => tracing::warn!("auto ac_id query failed, fallback to 17: {err:#}"),
            }
        }
        Ok(17)
    }

    async fn get_challenge(&self, ip: &IpAddr, ac_id: u32) -> Result<String> {
        let url = format!(
            "{}/cgi-bin/get_challenge",
            self.config.portal_url.trim_end_matches('/')
        );
        let ip_str = ip.to_string();
        let ac_id_str = ac_id.to_string();
        let raw = self
            .http
            .get(url)
            .query(&[
                ("callback", "jsonp"),
                ("username", self.config.username.as_str()),
                ("ip", ip_str.as_str()),
                ("ac_id", ac_id_str.as_str()),
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
}

fn parse_portal_response(text: &str) -> Result<PortalResponse> {
    let body = strip_jsonp(text);
    let parsed =
        serde_json::from_str::<PortalResponse>(body).context("failed to parse portal response")?;
    Ok(parsed)
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

fn debug_response(kind: &str, raw: &str) {
    tracing::debug!(target: "gdou-net-login", "{} response: {}", kind, raw);
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
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
