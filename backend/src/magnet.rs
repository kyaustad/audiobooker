use regex::Regex;

const BASE32: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

fn base32_to_hex(input: &str) -> Option<String> {
    let mut bits = String::new();
    for c in input.chars() {
        let lower = c.to_ascii_lowercase() as u8;
        let idx = BASE32.iter().position(|&b| b == lower)?;
        bits.push_str(&format!("{:05b}", idx));
    }
    let mut hex = String::new();
    for chunk in bits.as_bytes().chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let nibble = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 2).ok()?;
        hex.push_str(&format!("{nibble:x}"));
    }
    if hex.len() == 40 {
        Some(hex)
    } else {
        None
    }
}

pub fn normalize_info_hash(input: &str) -> Option<String> {
    let compact: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    if Regex::new(r"(?i)^[a-f0-9]{40}$").ok()?.is_match(&compact) {
        return Some(compact.to_ascii_lowercase());
    }
    if Regex::new(r"(?i)^[a-z2-7]{32}$").ok()?.is_match(&compact) {
        return base32_to_hex(&compact);
    }
    None
}

pub fn parse_magnet_hash(magnet: &str) -> Option<String> {
    let re = Regex::new(r"(?i)xt=urn:btih:([a-f0-9]{40}|[a-z2-7]{32})").ok()?;
    let caps = re.captures(magnet)?;
    let hash = caps.get(1)?.as_str();
    if hash.len() == 40 {
        Some(hash.to_ascii_lowercase())
    } else {
        base32_to_hex(hash)
    }
}

pub fn parse_magnet_name(magnet: &str) -> Option<String> {
    let re = Regex::new(r"(?i)dn=([^&]+)").ok()?;
    let caps = re.captures(magnet)?;
    let raw = caps.get(1)?.as_str().replace('+', " ");
    Some(urlencoding::decode(&raw).map(|s| s.into_owned()).unwrap_or(raw))
}

pub fn build_magnet(info_hash: &str, name: Option<&str>) -> String {
    let mut uri = format!("magnet:?xt=urn:btih:{info_hash}");
    if let Some(n) = name.filter(|s| !s.trim().is_empty()) {
        uri.push_str("&dn=");
        uri.push_str(&urlencoding::encode(n.trim()));
    }
    uri
}

#[derive(Debug, Clone)]
pub struct ParsedDownloadInput {
    pub magnet_uri: String,
    pub info_hash: String,
    pub name: Option<String>,
}

pub fn parse_download_input(input: &str, name: Option<&str>) -> Option<ParsedDownloadInput> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("magnet:?") {
        let info_hash = parse_magnet_hash(trimmed)?;
        let display = parse_magnet_name(trimmed).or_else(|| name.map(|s| s.trim().to_string()));
        return Some(ParsedDownloadInput {
            magnet_uri: trimmed.to_string(),
            info_hash,
            name: display.filter(|s| !s.is_empty()),
        });
    }

    let info_hash = normalize_info_hash(trimmed)?;
    let display = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(ParsedDownloadInput {
        magnet_uri: build_magnet(&info_hash, display.as_deref()),
        info_hash,
        name: display,
    })
}
