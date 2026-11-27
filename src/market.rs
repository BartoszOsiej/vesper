//! Market P2S: lokalny plik rynku + handshake.
//!
//! Zasada z projektu: klient startuje z lokalną kopią rynku i manifestem assetów.
//! Przy starcie liczymy hash każdego assetu i porównujemy z manifestem. Niezgodność
//! = rynek się nie otwiera (klient nie wchodzi do gry), bo oznacza to podmieniony
//! plik modelu albo inny build gry.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MarketItem {
    pub id: String,
    pub name: String,
    /// Cena kupna i sprzedaży w kredytach.
    pub buy: u64,
    pub sell: u64,
}

#[derive(Debug, Clone)]
pub struct Market {
    pub station: String,
    pub version: u32,
    pub items: Vec<MarketItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MarketError {
    Io(String),
    Malformed(String),
    /// Hash pliku nie zgadza się z manifestem — klient nie może wystartować.
    AssetMismatch { path: String, expected: String, got: String },
    MissingAsset(String),
}

impl std::fmt::Display for MarketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketError::Io(e) => write!(f, "błąd wejścia/wyjścia: {e}"),
            MarketError::Malformed(e) => write!(f, "zły format rynku: {e}"),
            MarketError::AssetMismatch { path, expected, got } => write!(
                f,
                "asset {path} niezgodny z manifestem (oczekiwano {expected}, jest {got})"
            ),
            MarketError::MissingAsset(p) => write!(f, "brak assetu z manifestu: {p}"),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Prosty parser formatu:
///
/// ```text
/// station VESPER-01
/// version 1
/// item ore_raw "Ruda" 12 8
/// asset ships/frigate.obj <sha256>
/// ```
pub fn parse(text: &str) -> Result<(Market, BTreeMap<String, String>), MarketError> {
    let mut station = String::new();
    let mut version = 0u32;
    let mut items = Vec::new();
    let mut assets = BTreeMap::new();

    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next().unwrap_or("") {
            "station" => station = it.next().unwrap_or("").to_string(),
            "version" => {
                version = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| MarketError::Malformed(format!("wersja, linia {}", n + 1)))?;
            }
            "item" => {
                let id = it.next().ok_or_else(|| MarketError::Malformed("item: brak id".into()))?.to_string();
                let name = it
                    .next()
                    .ok_or_else(|| MarketError::Malformed("item: brak nazwy".into()))?
                    .trim_matches('"')
                    .to_string();
                let buy: u64 = it.next().and_then(|v| v.parse().ok()).ok_or_else(|| {
                    MarketError::Malformed(format!("item {id}: zła cena kupna"))
                })?;
                let sell: u64 = it.next().and_then(|v| v.parse().ok()).ok_or_else(|| {
                    MarketError::Malformed(format!("item {id}: zła cena sprzedaży"))
                })?;
                items.push(MarketItem { id, name, buy, sell });
            }
            "asset" => {
                let p = it.next().ok_or_else(|| MarketError::Malformed("asset: brak ścieżki".into()))?;
                let h = it.next().ok_or_else(|| MarketError::Malformed("asset: brak hasha".into()))?;
                assets.insert(p.to_string(), h.to_string());
            }
            other => return Err(MarketError::Malformed(format!("nieznany klucz: {other}"))),
        }
    }
    if station.is_empty() {
        return Err(MarketError::Malformed("brak stacji".into()));
    }
    Ok((Market { station, version, items }, assets))
}

/// Handshake: rynek + weryfikacja assetów względem manifestu.
pub fn handshake(root: &Path, market_file: &Path) -> Result<Market, MarketError> {
    let text = std::fs::read_to_string(market_file).map_err(|e| MarketError::Io(e.to_string()))?;
    let (market, assets) = parse(&text)?;
    for (rel, expected) in assets {
        let path: PathBuf = root.join(&rel);
        let bytes = std::fs::read(&path).map_err(|_| MarketError::MissingAsset(rel.clone()))?;
        let got = sha256_hex(&bytes);
        if got != expected {
            return Err(MarketError::AssetMismatch { path: rel, expected, got });
        }
    }
    Ok(market)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn market_text(asset: &str, hash: &str) -> String {
        format!(
            "station VESPER-01\nversion 1\nitem ore_raw \"Ruda\" 12 8\nasset {asset} {hash}\n"
        )
    }

    #[test]
    fn parses_market_and_assets() {
        let (m, a) = parse(&market_text("ship.obj", "abc")).unwrap();
        assert_eq!(m.station, "VESPER-01");
        assert_eq!(m.version, 1);
        assert_eq!(m.items.len(), 1);
        assert_eq!(m.items[0].name, "Ruda");
        assert_eq!(m.items[0].buy, 12);
        assert_eq!(a["ship.obj"], "abc");
    }

    #[test]
    fn rejects_unknown_key() {
        let err = parse("station X\nversion 1\nwynalazek 1\n").unwrap_err();
        assert!(matches!(err, MarketError::Malformed(_)));
    }

    #[test]
    fn handshake_accepts_matching_asset() {
        let dir = std::env::temp_dir().join("vesper-handshake-ok");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ship.obj"), b"v 0 0 0\n").unwrap();
        let hash = sha256_hex(b"v 0 0 0\n");
        let mfile = dir.join("market.txt");
        std::fs::write(&mfile, market_text("ship.obj", &hash)).unwrap();
        let m = handshake(&dir, &mfile).unwrap();
        assert_eq!(m.items[0].id, "ore_raw");
    }

    #[test]
    fn handshake_refuses_tampered_asset() {
        let dir = std::env::temp_dir().join("vesper-handshake-bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ship.obj"), b"v 9 9 9\n").unwrap();
        let mfile = dir.join("market.txt");
        std::fs::write(&mfile, market_text("ship.obj", &sha256_hex(b"v 0 0 0\n"))).unwrap();
        match handshake(&dir, &mfile) {
            Err(MarketError::AssetMismatch { .. }) => {}
            other => panic!("oczekiwano AssetMismatch, jest {other:?}"),
        }
    }
}
