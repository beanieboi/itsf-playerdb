use crate::{models, schema};
use diesel::prelude::*;

no_arg_sql_function!(
    last_insert_rowid,
    diesel::sql_types::Integer,
    "Represents the SQL last_insert_row() function"
);

fn expect_result<T>(result: Result<T, diesel::result::Error>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("SQL Error: {:?}", err),
    }
}

pub fn get_player(conn: &SqliteConnection, itsf_lic: i32) -> Option<models::Player> {
    use schema::players::dsl::*;

    let player = players
        .filter(itsf_id.eq(itsf_lic))
        .first::<models::Player>(conn)
        .optional();

    expect_result(player)
}

pub fn add_player(conn: &SqliteConnection, new_player: models::Player) -> bool {
    use schema::players::dsl::*;

    let result = diesel::insert_or_ignore_into(players)
        .values(new_player)
        .execute(conn);

    match expect_result(result) {
        0 => false,
        1 => true,
        _ => panic!("invalid query result for player insert"),
    }
}

pub fn get_player_image(conn: &SqliteConnection, itsf_lic: i32) -> Option<models::PlayerImage> {
    use schema::player_images::dsl::*;

    let players = player_images
        .filter(itsf_id.eq(itsf_lic))
        .first::<models::PlayerImage>(conn)
        .optional();

    expect_result(players)
}

pub fn add_player_image(conn: &SqliteConnection, new_image: models::PlayerImage) -> bool {
    use schema::player_images::dsl::*;

    let result = diesel::insert_or_ignore_into(player_images)
        .values(new_image)
        .execute(conn);

    match expect_result(result) {
        0 => false,
        1 => true,
        _ => panic!("invalid query result for player image insert"),
    }
}

pub fn add_itsf_rankings(
    conn: &SqliteConnection,
    year: i32,
    queried_at: chrono::NaiveDateTime,
    category: models::ItsfRankingCategory,
    class: models::ItsfRankingClass,
    place_to_itsf_lic: &[(i32, i32)],
) -> bool {
    let result = conn.transaction::<bool, diesel::result::Error, _>(|| {
        let ranking = models::NewItsfRanking {
            year,
            queried_at,
            count: place_to_itsf_lic.len() as i32,
            category: category.into(),
            class: class.into(),
        };

        // add new itsf_rankings entry
        let result = diesel::insert_into(schema::itsf_rankings::dsl::itsf_rankings)
            .values(&ranking)
            .execute(conn)?;
        if result != 1 {
            log::error!("Failed to insert new ITSF ranking");
            return Ok(false);
        }

        // fetch last rowid (screw sqlite/diesen integration!), insert actual placements
        let last_rowid = diesel::select(last_insert_rowid).get_result::<i32>(conn)?;
        let rankings = place_to_itsf_lic
            .iter()
            .map(|place| models::ItsfRankingEntry {
                itsf_ranking_id: last_rowid,
                place: place.0,
                player_itsf_id: place.1,
            })
            .collect::<Vec<models::ItsfRankingEntry>>();

        let result = diesel::insert_into(schema::itsf_ranking_entries::dsl::itsf_ranking_entries)
            .values(&rankings)
            .execute(conn)?;

        if result != rankings.len() {
            log::error!(
                "Failed to insert ITSF ranking entries for id={}, count={}, inserted={}",
                last_rowid,
                rankings.len(),
                result
            );
            return Ok(false);
        }

        Ok(true)
    });

    expect_result(result)
}

pub struct PlayerItsfRanking {
    pub place: i32,
    pub year: i32,
    pub queried_at: chrono::NaiveDateTime,
    pub category: Option<String>,
}

impl PlayerItsfRanking {
    pub fn get(conn: &SqliteConnection, itsf_lic: i32) -> Option<Vec<Self>> {
        None
    }
}
