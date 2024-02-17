use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{prelude::*, Insertable, Queryable};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::schema::*;

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = players)]
struct DbPlayer {
    itsf_id: i32,
    json_data: serde_json::Value,
}

pub struct DbConnection {
    pool: diesel::r2d2::Pool<ConnectionManager<PgConnection>>,
}

fn expect_result<T>(result: Result<T, diesel::result::Error>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("SQL Error: {:?}", err),
    }
}

impl DbConnection {
    pub fn open(database_url: &str) -> Self {
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        let pool = Pool::builder()
            .test_on_check_out(true)
            .build(manager)
            .expect("Could not build connection pool");

        Self { pool }
    }

    pub fn get_player_ids(&mut self) -> Vec<i32> {
        use crate::schema::players::dsl;
        let conn = &mut self.pool.get().unwrap();
        let ids = dsl::players.select(dsl::itsf_id).load(conn);
        expect_result(ids)
    }

    pub fn write_player_json<T: Serialize>(&mut self, itsf_id: i32, data: &T) {
        let json_data = serde_json::to_value(data).expect("JSON serialization failed");
        let player = DbPlayer { itsf_id, json_data };

        use crate::schema::players::dsl;
        let conn = &mut self.pool.get().unwrap();

        let result = diesel::insert_into(dsl::players)
            .values(&player)
            .on_conflict(dsl::itsf_id)
            .do_update()
            .set(&player)
            .execute(conn);

        let result = expect_result(result);
        if result != 1 {
            panic!("invalid query result for player insert: {}", result);
        }
    }

    pub fn read_player_json<T: DeserializeOwned>(&mut self, itsf_id: i32) -> Result<T, String> {
        use crate::schema::players::dsl;
        let conn = &mut self.pool.get().unwrap();

        let player = dsl::players
            .filter(dsl::itsf_id.eq(itsf_id))
            .first::<DbPlayer>(conn)
            .optional();

        match expect_result(player) {
            Some(player) => serde_json::from_value(player.json_data)
                .map_err(|err| format!("JSON Error when loading player {}: {}", itsf_id, err)),
            None => Err(format!("No player data found for player {}", itsf_id)),
        }
    }
}
