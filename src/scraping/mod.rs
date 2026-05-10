use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use crate::{
    background::BackgroundOperationProgress,
    data::DatabaseRef,
    data::{dtfb, itsf},
};
use futures_util::future::join_all;

mod download;
mod dtfb_players;
mod itsf_rankings;
mod players;

async fn download_itsf_players(
    db: &DatabaseRef,
    player_refs: &[players::PlayerRef],
    progress: Arc<BackgroundOperationProgress>,
    _force: bool,
) -> Result<HashMap<String, i32>, String> {
    let mut player_ids = HashMap::new();
    let mut missing_players = player_refs.to_vec();

    if !missing_players.is_empty() {
        progress.set_progress(1, missing_players.len() + 1);
        progress.log(format!(
            "[ITSF] Downloading {} ITSF player profiles",
            missing_players.len()
        ));

        // query players in sets of N, to hide ITSF server latency
        const MAX_CONCURRENT: usize = 5;
        while !missing_players.is_empty() {
            let mut player_futures = Vec::new();
            let count = missing_players.len().min(MAX_CONCURRENT);
            for _ in 0..count {
                let player_ref = missing_players.pop().unwrap();
                player_futures.push(async move {
                    let player = match &player_ref {
                        players::PlayerRef::Code(code) => players::download_player_info_by_code(code).await,
                    };
                    player.map(|player| (player_ref, player))
                });
            }

            for player in join_all(player_futures).await {
                match player {
                    Ok((player_ref, player)) => {
                        progress.log(format!(
                            "[ITSF] .. downloaded player info for ID={}: {} {} ({:?}, {:?})",
                            player.itsf_id, player.first_name, player.last_name, player.category, player.country_code
                        ));
                        let player_id = player.itsf_id;
                        let image = match &player_ref {
                            players::PlayerRef::Code(code) => {
                                players::download_player_image_by_code(code, player_id).await
                            }
                        };
                        db.add_player(player);
                        player_ids.insert(player_ref.key(), player_id);
                        match image {
                            Ok(Some(image)) => db.set_player_image(image),
                            Ok(None) => {}
                            Err(err) => progress.warn(format!("[ITSF] Failed to download player image: {}", err)),
                        }
                    }
                    Err(err) => {
                        progress.warn(format!("[ITSF] Failed to download player: {}", err));
                    }
                }
            }
        }

        progress.log("[ITSF] Done".to_string());
    }

    Ok(player_ids)
}

async fn do_itsf_rankings_downloads(
    db: &DatabaseRef,
    years: Vec<i32>,
    categories: Vec<itsf::RankingCategory>,
    classes: Vec<itsf::RankingClass>,
    progress: Arc<BackgroundOperationProgress>,
    max_rank: usize,
    force: bool,
) -> Result<(), String> {
    for year in years {
        for category in categories.iter().cloned() {
            for class in classes.iter().cloned() {
                progress.log(format!(
                    "[ITSF] Scraping ITSF rankings for {}, {:?}, {:?}",
                    year, category, class
                ));
                let rankings = itsf_rankings::download(year, category, class, max_rank).await?;

                let itsf_player_refs: Vec<players::PlayerRef> = rankings
                    .iter()
                    .map(|entry| players::PlayerRef::Code(entry.player_code.clone()))
                    .collect();
                let player_ids = download_itsf_players(db, &itsf_player_refs, progress.clone(), force).await?;

                for placement in rankings {
                    if let Some(player_id) = player_ids.get(&placement.player_code) {
                        db.add_player_itsf_ranking(
                            *player_id,
                            itsf::Ranking {
                                year,
                                category,
                                class,
                                place: placement.place,
                            },
                        );
                    } else {
                        progress.warn(format!(
                            "[ITSF] Skipping ranking for unresolved player code {}",
                            placement.player_code
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn start_itsf_rankings_download(
    db: DatabaseRef,
    years: Vec<i32>,
    categories: Vec<itsf::RankingCategory>,
    classes: Vec<itsf::RankingClass>,
    max_rank: usize,
    force: bool,
) -> Weak<BackgroundOperationProgress> {
    let (arc, weak) = BackgroundOperationProgress::new("ITSF Rankings Download", 1);
    tokio::spawn(async move {
        match do_itsf_rankings_downloads(&db, years, categories, classes, arc.clone(), max_rank, force).await {
            Ok(_) => {}
            Err(err) => log::error!("failed to download ITSF rankings: {}", err),
        };
        arc.set_progress(1, 1);
    });
    weak
}

fn add_dtfb_player_data(db: &DatabaseRef, dtfb_player: dtfb_players::DtfbPlayerInfo) {
    db.set_player_dtfb_id(dtfb_player.itsf_id, dtfb_player.dtfb_id);

    for result in dtfb_player.championship_results {
        db.add_player_dtfb_championship_result(
            dtfb_player.itsf_id,
            dtfb::NationalChampionshipResult {
                year: result.year,
                place: result.place,
                category: result.category,
                class: result.class,
            },
        );
    }

    for ranking in dtfb_player.national_rankings {
        db.add_player_dtfb_ranking(
            dtfb_player.itsf_id,
            dtfb::NationalRanking {
                year: ranking.year,
                place: ranking.place,
                category: ranking.category,
            },
        );
    }

    for team in dtfb_player.teams {
        db.add_player_dtfb_team(dtfb_player.itsf_id, team.0, team.1);
    }
}

async fn download_and_store_dtfb_players(
    db: &DatabaseRef,
    mut dtfb_player_ids: Vec<i32>,
    progress: Arc<BackgroundOperationProgress>,
    force: bool,
) -> Result<(), String> {
    if dtfb_player_ids.is_empty() {
        return Ok(());
    }

    progress.log(format!("[DTFB] Downloading {} players", dtfb_player_ids.len()));

    const MAX_CONCURRENT: usize = 5;
    while !dtfb_player_ids.is_empty() {
        let mut player_futures = Vec::new();
        let count = dtfb_player_ids.len().min(MAX_CONCURRENT);
        for _ in 0..count {
            let dtfb_id = dtfb_player_ids.pop().unwrap();
            player_futures.push(dtfb_players::DtfbPlayerInfo::download(dtfb_id));
        }

        let mut downloaded_players = Vec::new();
        for dtfb_player in join_all(player_futures).await {
            match dtfb_player {
                Ok(dtfb_player) => {
                    progress.log(format!(
                        "[DTFB] .. downloaded player info for DTFB={}, ITSF={}",
                        dtfb_player.dtfb_id, dtfb_player.itsf_id,
                    ));
                    downloaded_players.push(dtfb_player);
                }
                Err(err) => {
                    progress.warn(format!("[DTFB] Failed to download player: {}", err));
                }
            }
        }

        let player_code_futures = downloaded_players
            .iter()
            .filter(|player| force || db.get_player(player.itsf_id).is_none())
            .map(|player| async move {
                let player_code =
                    players::find_player_code_by_license(&player.first_name, &player.last_name, player.itsf_id).await;
                (player, player_code)
            })
            .collect::<Vec<_>>();
        let itsf_player_refs: Vec<players::PlayerRef> = join_all(player_code_futures)
            .await
            .into_iter()
            .filter_map(|(player, player_code)| match player_code {
                Ok(Some(code)) => Some(players::PlayerRef::Code(code)),
                Ok(None) => {
                    progress.warn(format!(
                        "[ITSF] Could not find ITSF player code for DTFB={}, {} {} ({})",
                        player.dtfb_id, player.first_name, player.last_name, player.itsf_id
                    ));
                    None
                }
                Err(err) => {
                    progress.warn(format!(
                        "[ITSF] Failed to resolve ITSF player code for DTFB={}, {} {} ({}): {}",
                        player.dtfb_id, player.first_name, player.last_name, player.itsf_id, err
                    ));
                    None
                }
            })
            .collect();
        download_itsf_players(db, &itsf_player_refs, progress.clone(), force).await?;

        for dtfb_player in downloaded_players {
            add_dtfb_player_data(db, dtfb_player);
        }
    }

    Ok(())
}

async fn do_dtfb_rankings_download(
    db: DatabaseRef,
    seasons: Vec<i32>,
    progress: Arc<BackgroundOperationProgress>,
    max_rank: usize,
    force: bool,
) -> Result<(), String> {
    progress.log(format!(
        "[DTFB] starting download of DTFB rankings for seasons {:?}",
        seasons
    ));

    let mut downloaded_dtfb_player_ids = HashSet::new();

    for season in seasons {
        let ranking_ids = dtfb_players::collect_dtfb_rankings_for_season(season).await?;
        for ranking_id in ranking_ids {
            let dtfb_player_ids = dtfb_players::collect_dtfb_ids_from_rankings(ranking_id, max_rank).await?;
            let missing_player_ids = dtfb_player_ids
                .into_iter()
                .filter(|dtfb_id| downloaded_dtfb_player_ids.insert(*dtfb_id))
                .collect();

            download_and_store_dtfb_players(&db, missing_player_ids, progress.clone(), force).await?;
        }
    }

    progress.log("[DTFB] done".to_string());

    Ok(())
}

pub fn start_dtfb_rankings_download(
    db: DatabaseRef,
    seasons: Vec<i32>,
    max_rank: usize,
    force: bool,
) -> Weak<BackgroundOperationProgress> {
    let (arc, weak) = BackgroundOperationProgress::new("DTFB Rankings Download", 1);
    tokio::spawn(async move {
        match do_dtfb_rankings_download(db, seasons, arc.clone(), max_rank, force).await {
            Ok(_) => {}
            Err(err) => log::error!("failed to download DTFB rankings: {}", err),
        };
        arc.set_progress(1, 1);
    });
    weak
}
