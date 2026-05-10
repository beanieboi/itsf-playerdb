#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i8)]
pub enum PlayerCategory {
    Men,
    Women,
    JuniorMale,
    JuniorFemale,
    SeniorMale,
    SeniorFemale,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(i8)]
pub enum RankingCategory {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "women")]
    Women,
    #[serde(rename = "junior")]
    Junior,
    #[serde(rename = "senior")]
    Senior,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(i8)]
pub enum RankingClass {
    #[serde(rename = "singles")]
    Singles,
    #[serde(rename = "doubles")]
    Doubles,
    #[serde(rename = "combined")]
    Combined,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Ranking {
    pub year: i32,
    pub place: i32,
    pub category: RankingCategory,
    pub class: RankingClass,
}

impl Ranking {
    pub fn matches(&self, other_ranking: &Self) -> bool {
        self.year == other_ranking.year && self.category == other_ranking.category && self.class == other_ranking.class
    }
}
