#![allow(clippy::redundant_field_names)]

extern crate diesel;

use crate::data::{dtfb, itsf};
use actix_web::{middleware::Logger, web, App, Error, HttpResponse, HttpServer};
use actix_web_httpauth::extractors::basic::BasicAuth;
use chrono::Datelike;
use rustls::ServerConfig;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::sync::{Mutex, MutexGuard, Weak};

mod background;
mod data;
mod json;
mod schema;
mod scraping;

fn is_authorized(auth: BasicAuth) -> bool {
    let env_password = std::env::var("PASSWORD").expect("PASSWORD missing from environment");
    let user_password = auth.password().unwrap().to_string();
    env_password == user_password
}

struct AppState {
    data: data::DatabaseRef,
    download: Mutex<Weak<background::BackgroundOperationProgress>>,
}
impl AppState {
    fn get_download(
        this: &web::Data<AppState>,
    ) -> Result<MutexGuard<Weak<background::BackgroundOperationProgress>>, Error> {
        this.download
            .lock()
            .map_err(|_| actix_web::error::ErrorInternalServerError("internal lock"))
    }
}

#[actix_web::get("/player/{itsf_lic}")]
async fn get_player(data: web::Data<AppState>, itsf_lic: web::Path<i32>) -> Result<HttpResponse, Error> {
    let itsf_lic = itsf_lic.into_inner();

    #[derive(serde::Serialize)]
    struct PlayerJson {
        pub first_name: String,
        pub last_name: String,
        pub birth_year: i32,
        pub country_code: String,
        pub image_url: String,
        pub itsf_rankings: Vec<itsf::Ranking>,
        pub dtfb_rankings: Vec<dtfb::NationalRanking>,
        pub dm_placements: Vec<dtfb::NationalChampionshipResult>,
        pub dtfl_teams: Vec<dtfb::NationalTeam>,
        pub comment: String,
    }

    match data.data.get_player(itsf_lic) {
        Some(player) => {
            let mut player = PlayerJson {
                first_name: player.first_name,
                last_name: player.last_name,
                birth_year: player.birth_year,
                country_code: player.country_code.unwrap_or(String::new()),
                image_url: format!("/image/{}.jpg", itsf_lic),
                itsf_rankings: player.itsf_rankings,
                dtfb_rankings: player.dtfb_national_rankings,
                dm_placements: player.dtfb_championship_results,
                dtfl_teams: player.dtfb_league_teams,
                comment: player.comments.last().map(|c| c.text.clone()).unwrap_or(String::new()),
            };

            player
                .itsf_rankings
                .retain(|ranking| ranking.class != itsf::RankingClass::Combined);
            player.itsf_rankings.sort_by(|a, b| b.year.cmp(&a.year));
            player.dtfb_rankings.sort_by(|a, b| b.year.cmp(&a.year));
            player.dm_placements.sort_by(|a, b| b.year.cmp(&a.year));
            player.dtfl_teams.sort_by(|a, b| b.year.cmp(&a.year));

            Ok(HttpResponse::Ok().json(json::ok(player)))
        }
        None => Ok(HttpResponse::NotFound().json(json::err("No such player"))),
    }
}

#[actix_web::get("/listplayers")]
async fn list_players(data: web::Data<AppState>) -> Result<HttpResponse, Error> {
    #[derive(serde::Serialize)]
    struct PlayerData {
        pub itsf_lic: i32,
        pub first_name: String,
        pub last_name: String,
    }

    let ids = data.data.get_player_ids();
    let players: Vec<PlayerData> = ids
        .iter()
        .map(|itsf_lic| {
            let player = data.data.get_player(*itsf_lic).unwrap();
            PlayerData {
                itsf_lic: *itsf_lic,
                first_name: player.first_name,
                last_name: player.last_name,
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(json::ok(players)))
}

#[actix_web::get("/image/{itsf_lic}.jpg")]
async fn get_player_image(data: web::Data<AppState>, itsf_lic: web::Path<i32>) -> Result<HttpResponse, Error> {
    let itsf_lic = itsf_lic.into_inner();

    match data.data.get_player_image(itsf_lic) {
        Some(player_image) => Ok(HttpResponse::Ok()
            .append_header(("Content-Type", "image/jpeg"))
            .body(player_image.image_data)),
        None => Ok(HttpResponse::NotFound().finish()),
    }
}

#[derive(serde::Serialize)]
struct DownloadStatus {
    running: bool,
    log: Vec<String>,
}

#[actix_web::get("/download_status")]
async fn download_status(data: web::Data<AppState>) -> Result<HttpResponse, Error> {
    let download = AppState::get_download(&data)?;
    let status = match download.upgrade() {
        Some(download) => DownloadStatus {
            running: true,
            log: download.get_log(),
        },
        None => DownloadStatus {
            running: false,
            log: Vec::new(),
        },
    };
    Ok(HttpResponse::Ok().json(json::ok(status)))
}

fn download_itsf(
    data: web::Data<AppState>,
    years: Vec<i32>,
    max_rank: usize,
    force: bool,
) -> Result<HttpResponse, Error> {
    let mut download = AppState::get_download(&data)?;
    if download.upgrade().is_some() {
        return Ok(HttpResponse::BadRequest().json(json::err("Ranking query still in progress")));
    }

    let categories = vec![
        itsf::RankingCategory::Open,
        itsf::RankingCategory::Women,
        itsf::RankingCategory::Senior,
        itsf::RankingCategory::Junior,
    ];
    let classes = vec![
        itsf::RankingClass::Singles,
        itsf::RankingClass::Doubles,
        itsf::RankingClass::Combined,
    ];
    *download = scraping::start_itsf_rankings_download(data.data.clone(), years, categories, classes, max_rank, force);

    Ok(HttpResponse::Ok().json(json::ok("Started download")))
}

#[derive(Deserialize)]
struct DownloadParams {
    year: Option<String>,
    max_rank: Option<usize>,
    force: Option<String>,
}

impl DownloadParams {
    fn parse_year(&self) -> Option<i32> {
        let min_year = 2010;
        let curr_year = chrono::Utc::now().naive_local().year();
        match &self.year {
            Some(year_str) => year_str.parse::<i32>().ok().and_then(|year| {
                if year >= min_year && year <= curr_year {
                    Some(year)
                } else {
                    None
                }
            }),
            None => Some(curr_year),
        }
    }

    fn parse_force(&self) -> bool {
        match &self.force {
            Some(force_str) => force_str == "true",
            None => false,
        }
    }
}

#[actix_web::post("/download_itsf")]
async fn download_itsf_single(
    data: web::Data<AppState>,
    params: web::Query<DownloadParams>,
    auth: BasicAuth,
) -> Result<HttpResponse, Error> {
    if !is_authorized(auth) {
        return Ok(HttpResponse::Forbidden().json(json::err("not authorized")));
    }

    let force = params.parse_force();
    let max_rank = params.max_rank.unwrap_or(1000);
    match params.parse_year() {
        Some(year) => download_itsf(data, vec![year], max_rank, force),
        None => Ok(HttpResponse::BadRequest().json(json::err("invalid year"))),
    }
}

#[actix_web::post("/download_itsf_all")]
async fn download_all_itsf(data: web::Data<AppState>, auth: BasicAuth) -> Result<HttpResponse, Error> {
    if !is_authorized(auth) {
        return Ok(HttpResponse::Forbidden().json(json::err("not authorized")));
    }

    let curr_year = chrono::Utc::now().naive_local().year();
    let years = (2010..curr_year + 1).collect();
    let max_rank = 1000;
    download_itsf(data, years, max_rank, false)
}

fn download_dtfb(
    data: web::Data<AppState>,
    seasons: Vec<i32>,
    max_rank: usize,
    force: bool,
) -> Result<HttpResponse, Error> {
    let mut download = AppState::get_download(&data)?;
    if download.upgrade().is_some() {
        return Ok(HttpResponse::BadRequest().json(json::err("Ranking query still in progress")));
    }

    *download = scraping::start_dtfb_rankings_download(data.data.clone(), seasons, max_rank, force);

    Ok(HttpResponse::Ok().json(json::ok("Started download")))
}

#[actix_web::post("/download_dtfb")]
async fn download_dtfb_single(
    data: web::Data<AppState>,
    params: web::Query<DownloadParams>,
    auth: BasicAuth,
) -> Result<HttpResponse, Error> {
    if !is_authorized(auth) {
        return Ok(HttpResponse::Forbidden().json(json::err("not authorized")));
    }

    let max_rank = params.max_rank.unwrap_or(1000);
    let force = params.parse_force();
    match params.parse_year() {
        Some(year) => download_dtfb(data, vec![year], max_rank, force),
        None => Ok(HttpResponse::BadRequest().json(json::err("invalid year"))),
    }
}

#[actix_web::post("/download_dtfb_all")]
async fn download_dtfb_all(data: web::Data<AppState>, auth: BasicAuth) -> Result<HttpResponse, Error> {
    if !is_authorized(auth) {
        return Ok(HttpResponse::Forbidden().json(json::err("not authorized")));
    }

    let curr_year = chrono::Utc::now().naive_local().year();
    let years = (2010..curr_year + 1).collect();
    let max_rank = 1000;
    download_dtfb(data, years, max_rank, false)
}

#[derive(Deserialize)]
struct AddCommentInfo {
    itsf_lic: i32,
    comment: String,
}

#[actix_web::post("/add_comment")]
async fn add_player_comment(
    data: web::Data<AppState>,
    info: web::Json<AddCommentInfo>,
    auth: BasicAuth,
) -> Result<HttpResponse, Error> {
    if !is_authorized(auth) {
        return Ok(HttpResponse::Forbidden().json(json::err("not authorized")));
    }

    data.data.add_player_comment(info.itsf_lic, info.comment.clone());
    Ok(HttpResponse::Ok().json(json::ok("added comment")))
}

fn get_rustls_config() -> Option<ServerConfig> {
    use rustls::{Certificate, PrivateKey};
    use rustls_pemfile::{read_all, Item};

    std::env::var("CERT_PEM").ok().map(|pem| {
        let pem = File::open(pem).expect("PEM file not found");
        let mut pem = BufReader::new(pem);
        let pem_sections = read_all(&mut pem).expect("Failed to parse PEM file");

        let certs: Vec<Certificate> = pem_sections
            .iter()
            .filter_map(|item| match item {
                Item::X509Certificate(cert) => Some(Certificate(cert.clone())),
                _ => None,
            })
            .collect();
        let key = pem_sections
            .iter()
            .filter_map(|item| match item {
                Item::RSAKey(key) => Some(PrivateKey(key.clone())),
                _ => None,
            })
            .next()
            .expect("no RSA key in PEM file");

        ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("Failed to initialize rustls")
    })
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL missing from environment");
    let html_path = std::env::var("HTML_ROOT").expect("HTML_ROOT missing from environment");
    let port = std::env::var("PORT").expect("PORT missing from environment");
    let port = port.parse::<u16>().expect("invalid PORT");
    let _password = std::env::var("PASSWORD").expect("PASSWORD missing from environment");

    let state = AppState {
        data: data::DatabaseRef::load(&database_url),
        download: Mutex::new(Weak::new()),
    };
    let state = web::Data::new(state);

    let mut server = HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(state.clone())
            .service(get_player)
            .service(get_player_image)
            .service(list_players)
            .service(download_status)
            .service(download_itsf_single)
            .service(download_all_itsf)
            .service(download_dtfb_single)
            .service(download_dtfb_all)
            .service(add_player_comment)
            .service(actix_files::Files::new("", &html_path).index_file("start.html"))
    });

    if let Some(server_config) = get_rustls_config() {
        log::info!("Starting HTTPS server at http://localhost:{}", port);
        server = server
            .bind_rustls(("0.0.0.0", port), server_config)
            .expect("Failed to start actix with rustls");
    } else {
        log::info!("Starting HTTP server at http://localhost:{}", port);
        server = server.bind(("0.0.0.0", port))?;
    }

    server.run().await
}
