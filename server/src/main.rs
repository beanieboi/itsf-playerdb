#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate r2d2;

use std::{sync::Arc, sync::Weak};

use actix_web::{middleware::Logger, web, App, Error, HttpResponse, HttpServer, Responder};
//use actix_web_httpauth::extractors::basic::BasicAuth;
use diesel::prelude::*;
use models::{ItsfRankingCategory, ItsfRankingClass};
use std::sync::Mutex;

type SqliteDbPool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<SqliteConnection>>;

mod background;
mod json;
mod models;
mod queries;
mod schema;
mod scraping;

struct AppState {
    db_pool: SqliteDbPool,
    itsf_ranking_download: Mutex<Weak<background::BackgroundOperationProgress>>,
}

impl AppState {
    async fn execute_db_operation<F, R>(
        data: web::Data<AppState>,
        f: F,
    ) -> Result<R, actix_web::Error>
    where
        F: FnOnce(&SqliteConnection) -> R + Send + 'static,
        R: Send + 'static,
    {
        // use web::block to offload blocking Diesel code without blocking server thread
        web::block(move || {
            let conn = data.db_pool.get()?;
            let result: Result<R, r2d2::Error> = Ok(f(&conn));
            result
        })
        .await?
        .map_err(actix_web::error::ErrorInternalServerError)
    }

    fn itsf_ranking_download(
        &self,
    ) -> Result<Option<Arc<background::BackgroundOperationProgress>>, Error> {
        Ok(self
            .itsf_ranking_download
            .lock()
            .map_err(|_| actix_web::error::ErrorInternalServerError("internal lock"))?
            .upgrade())
    }
}

#[actix_web::get("/player/{itsf_lic}")]
async fn hello(data: web::Data<AppState>, itsf_lic: web::Path<i32>) -> Result<HttpResponse, Error> {
    let itsf_lic = itsf_lic.into_inner();

    let player =
        AppState::execute_db_operation(data, move |conn| queries::get_player(conn, itsf_lic))
            .await?;

    let json = match player {
        None => "{ \"error\": \"No player found\" }".into(),
        Some(player) => {
            let json = serde_json::to_string(&player).unwrap();
            format!("{{ \"data\": {} }}", json)
        }
    };

    Ok(HttpResponse::Ok().body(json))
}

#[actix_web::get("/addplayer/{itsf_lic}/{first_name}/{last_name}")]
async fn add_player(
    data: web::Data<AppState>,
    itsf_lic: web::Path<(i32, String, String)>,
) -> Result<HttpResponse, Error> {
    let (itsf_lic, first_name, last_name) = itsf_lic.into_inner();

    let ok = AppState::execute_db_operation(data, move |conn| {
        queries::add_player(
            &conn,
            models::Player {
                itsf_id: itsf_lic,
                first_name: first_name,
                last_name: last_name,
                dtfb_license: None,
                birth_year: 1234,
                country_code: Some("GER".into()),
                category: models::PlayerCategory::Men.into(),
            },
        )
    })
    .await?;

    let json = if ok {
        "{ \"data\": true }"
    } else {
        "{ \"error\": \"player already exists\" }".into()
    };
    Ok(HttpResponse::Ok().body(json))
}

#[actix_web::get("/download/{year}/{category}/{class}")]
async fn download_itsf(
    data: web::Data<AppState>,
    itsf_lic: web::Path<(i32, String, String)>,
) -> Result<HttpResponse, Error> {
    let year = if itsf_lic.0 > 2006 {
        itsf_lic.0
    } else {
        return Ok(HttpResponse::BadRequest().body(json::err("Invalid year")));
    };

    let category = match itsf_lic.1.to_lowercase().as_str() {
        "open" => ItsfRankingCategory::Open,
        "women" => ItsfRankingCategory::Women,
        "senior" => ItsfRankingCategory::Senior,
        "junior" => ItsfRankingCategory::Junior,
        _ => {
            return Ok(HttpResponse::BadRequest().body(json::err(
                "Invalid category. Must be one of ['open', 'women', 'senior', 'junior'].",
            )))
        }
    };

    let class = match itsf_lic.2.to_lowercase().as_str() {
        "singles" => ItsfRankingClass::Singles,
        "doubles" => ItsfRankingClass::Singles,
        "combined" => ItsfRankingClass::Singles,
        _ => {
            return Ok(HttpResponse::BadRequest().body(json::err(
                "Invalid class. Must be one of ['singles', 'doubles', 'combined'].",
            )))
        }
    };

    let mut itsf_ranking_download = data
        .itsf_ranking_download
        .lock()
        .map_err(|_| actix_web::error::ErrorInternalServerError("internal lock"))?;

    if let Some(_) = itsf_ranking_download.upgrade() {
        return Ok(HttpResponse::BadRequest().body(json::err("Ranking query still in progress")));
    }

    let conn = data
        .db_pool
        .get()
        .map_err(actix_web::error::ErrorInternalServerError)?;
    *itsf_ranking_download = scraping::start_itsf_rankings_download(conn, year, category, class);

    let json = json::ok(format!("Launched background operation"));
    Ok(HttpResponse::Ok().body(json))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    // Open SQLite database pool
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL missing");
    let db_manager = diesel::r2d2::ConnectionManager::<SqliteConnection>::new(database_url);
    let db_pool = r2d2::Pool::builder()
        .build(db_manager)
        .expect("Failed to create R2D2 pool.");

    let state = AppState {
        db_pool,
        itsf_ranking_download: Mutex::new(Weak::new()),
    };
    let state = web::Data::new(state);

    let ok = AppState::execute_db_operation(state.clone(), move |conn| {
        let d = chrono::NaiveDate::from_ymd(2015, 6, 3);
        let t = chrono::NaiveTime::from_hms_milli(12, 34, 56, 789);
        let dt = chrono::NaiveDateTime::new(d, t);

        queries::add_itsf_rankings(
            &conn,
            2012,
            dt,
            models::ItsfRankingCategory::Open,
            models::ItsfRankingClass::Doubles,
            &[(1, 2), (3, 4)],
        );
    })
    .await;

    log::info!("Starting HTTP server at http://localhost:8080");

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .service(hello)
            .service(add_player)
            .service(download_itsf)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
