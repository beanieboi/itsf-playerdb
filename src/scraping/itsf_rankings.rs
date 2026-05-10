use super::download;
use crate::data::itsf::*;
use serde::{Deserialize, Serialize};

const WORLD_TOUR_ID: i32 = 1;
const OFFICIAL_RULE_ID: i32 = 1;

#[derive(Debug, Clone)]
pub struct RankingPlacement {
    pub place: i32,
    pub player_code: String,
}

#[derive(Serialize)]
struct RankingsRequest {
    tour: i32,
    fallback: &'static str,
    rule: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    season: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<usize>,
}

#[derive(Deserialize)]
struct RankingsResponse {
    pages: usize,
    standings: Vec<Standing>,
    seasons: Vec<Season>,
    rules: Vec<Rule>,
}

#[derive(Deserialize)]
struct Standing {
    rank: i32,
    team: Vec<StandingPlayer>,
}

#[derive(Deserialize)]
struct StandingPlayer {
    code: String,
}

#[derive(Deserialize)]
struct Season {
    id: i32,
    name: String,
}

#[derive(Deserialize)]
struct Rule {
    id: i32,
    categories: Vec<Category>,
}

#[derive(Deserialize)]
struct Category {
    id: i32,
    name: String,
}

async fn request_rankings(request: &RankingsRequest) -> Result<RankingsResponse, String> {
    download::post_json(
        "https://api.tablesoccer.org/cms.rankings",
        &[("X-Organization", "ITSF")],
        request,
    )
    .await
}

fn category_name(category: RankingCategory, class: RankingClass) -> &'static str {
    match (category, class) {
        (RankingCategory::Open, RankingClass::Singles) => "Open Singles",
        (RankingCategory::Open, RankingClass::Doubles) => "Open Doubles",
        (RankingCategory::Open, RankingClass::Combined) => "Open Combined",
        (RankingCategory::Women, RankingClass::Singles) => "Women Singles",
        (RankingCategory::Women, RankingClass::Doubles) => "Women Doubles",
        (RankingCategory::Women, RankingClass::Combined) => "Women Combined",
        (RankingCategory::Junior, RankingClass::Singles) => "Junior Under 19 Singles",
        (RankingCategory::Junior, RankingClass::Doubles) => "Junior Under 19 Doubles",
        (RankingCategory::Junior, RankingClass::Combined) => "Junior Under 19 Combined",
        (RankingCategory::Senior, RankingClass::Singles) => "Senior Over 50 Singles",
        (RankingCategory::Senior, RankingClass::Doubles) => "Senior Over 50 Doubles",
        (RankingCategory::Senior, RankingClass::Combined) => "Senior Over 50 Combined",
    }
}

pub async fn download(
    year: i32,
    category: RankingCategory,
    class: RankingClass,
    count: usize,
) -> Result<Vec<RankingPlacement>, String> {
    let initial = request_rankings(&RankingsRequest {
        tour: WORLD_TOUR_ID,
        fallback: "player",
        rule: OFFICIAL_RULE_ID,
        season: None,
        category: None,
        page: None,
    })
    .await?;
    let season = initial
        .seasons
        .iter()
        .find(|season| season.name == year.to_string())
        .ok_or(format!("can't find ITSF season {}", year))?;
    let category_name = category_name(category, class);
    let category = initial
        .rules
        .iter()
        .find(|rule| rule.id == OFFICIAL_RULE_ID)
        .and_then(|rule| rule.categories.iter().find(|category| category.name == category_name))
        .ok_or(format!("can't find ITSF ranking category {}", category_name))?;

    let mut ret = Vec::new();
    let mut page = 1;
    loop {
        let rankings = request_rankings(&RankingsRequest {
            tour: WORLD_TOUR_ID,
            fallback: "player",
            rule: OFFICIAL_RULE_ID,
            season: Some(season.id),
            category: Some(category.id),
            page: Some(page),
        })
        .await?;

        for standing in rankings.standings {
            if standing.rank as usize > count {
                return Ok(ret);
            }
            for player in standing.team {
                ret.push(RankingPlacement {
                    place: standing.rank,
                    player_code: player.code,
                });
            }
        }

        if page >= rankings.pages {
            return Ok(ret);
        }
        page += 1;
    }
}
