use crate::data::{itsf::PlayerCategory, Player, PlayerImage};

use super::download;
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum PlayerRef {
    Code(String),
}

impl PlayerRef {
    pub fn key(&self) -> String {
        match self {
            Self::Code(code) => code.clone(),
        }
    }
}

#[derive(Serialize)]
struct PlayerRequest<'a> {
    player: &'a str,
}

#[derive(Serialize)]
struct PlayerSearchRequest<'a> {
    player: &'a str,
}

#[derive(Deserialize)]
struct PlayerResponse {
    player: ApiPlayer,
}

#[derive(Deserialize)]
struct PlayerSearchResult {
    code: String,
}

#[derive(Deserialize)]
struct ApiPlayer {
    id: i32,
    first_name: String,
    last_name: String,
    image: Option<String>,
    country: Option<String>,
    international_license: Option<String>,
    categories: Vec<String>,
}

async fn request_player(code: &str) -> Result<ApiPlayer, String> {
    let response: PlayerResponse = download::post_json(
        "https://api.tablesoccer.org/cms.player",
        &[("X-Organization", "ITSF")],
        &PlayerRequest { player: code },
    )
    .await?;
    Ok(response.player)
}

fn parse_player_category(categories: &[String]) -> PlayerCategory {
    let categories = categories.join(" ").to_lowercase();
    let is_woman = categories.contains("women") || categories.contains("female");
    if categories.contains("junior") {
        if is_woman {
            PlayerCategory::JuniorFemale
        } else {
            PlayerCategory::JuniorMale
        }
    } else if categories.contains("senior") {
        if is_woman {
            PlayerCategory::SeniorFemale
        } else {
            PlayerCategory::SeniorMale
        }
    } else if is_woman {
        PlayerCategory::Women
    } else {
        PlayerCategory::Men
    }
}

fn parse_player_id(player: &ApiPlayer) -> i32 {
    player
        .international_license
        .as_deref()
        .and_then(|license| license.parse::<i32>().ok())
        .unwrap_or(player.id)
}

pub async fn download_player_info_by_code(code: &str) -> Result<Player, String> {
    let player = request_player(code)
        .await
        .map_err(|msg| format!("Player[{}]: {}", code, msg))?;

    Ok(Player {
        itsf_id: parse_player_id(&player),
        first_name: player.first_name,
        last_name: player.last_name,
        birth_year: 0,
        country_code: player.country,
        category: parse_player_category(&player.categories),
        itsf_rankings: Vec::new(),
        dtfb_id: None,
        dtfb_championship_results: Vec::new(),
        dtfb_national_rankings: Vec::new(),
        dtfb_league_teams: Vec::new(),
        comments: Vec::new(),
    })
}

async fn search_player_codes(query: &str) -> Result<Vec<String>, String> {
    let response: Vec<PlayerSearchResult> = download::post_json(
        "https://api.tablesoccer.org/cms.player_search",
        &[("X-Organization", "ITSF")],
        &PlayerSearchRequest { player: query },
    )
    .await?;
    Ok(response.into_iter().map(|player| player.code).collect())
}

pub async fn find_player_code_by_license(
    first_name: &str,
    last_name: &str,
    international_license: i32,
) -> Result<Option<String>, String> {
    let full_name = format!("{} {}", first_name, last_name);
    let mut candidates = search_player_codes(&full_name)
        .await
        .map_err(|msg| format!("Player search[{}]: {}", full_name, msg))?;
    if candidates.is_empty() {
        candidates = search_player_codes(last_name)
            .await
            .map_err(|msg| format!("Player search[{}]: {}", last_name, msg))?;
    }

    for code in candidates {
        let player = request_player(&code)
            .await
            .map_err(|msg| format!("Player[{}]: {}", code, msg))?;
        if parse_player_id(&player) == international_license {
            return Ok(Some(code));
        }
    }

    Ok(None)
}

fn image_format_from_content_type(content_type: Option<&str>) -> Option<&'static str> {
    match content_type {
        Some(content_type) if content_type.starts_with("image/jpeg") => Some("jpg"),
        Some(content_type) if content_type.starts_with("image/png") => Some("png"),
        Some(content_type) if content_type.starts_with("image/svg+xml") => Some("svg"),
        _ => None,
    }
}

fn image_format_from_bytes(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpg"
    } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
        "svg"
    } else {
        "bin"
    }
}

async fn download_image_from_url(itsf_id: i32, url: &str) -> Result<Option<PlayerImage>, String> {
    let response = match reqwest::get(url).await {
        Ok(response) => {
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                return Err(format!("{} returned HTTP {}", url, response.status()));
            }
            response
        }
        Err(err) => {
            if let Some(status) = err.status() {
                if status == StatusCode::NOT_FOUND {
                    return Ok(None);
                }
            }
            return Err(err.to_string());
        }
    };

    let image_format = image_format_from_content_type(
        response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
    );
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    let image_format = image_format.unwrap_or_else(|| image_format_from_bytes(&bytes));
    Ok(Some(PlayerImage {
        itsf_id,
        image_data: bytes.to_vec(),
        image_format: image_format.to_string(),
    }))
}

pub async fn download_player_image_by_code(code: &str, itsf_id: i32) -> Result<Option<PlayerImage>, String> {
    let player = request_player(code)
        .await
        .map_err(|msg| format!("Player image[{}]: {}", code, msg))?;

    match player.image {
        Some(url) => download_image_from_url(itsf_id, &url).await,
        None => Ok(None),
    }
}
