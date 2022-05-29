use std::io::BufRead;

use super::download;
use crate::models;
use scraper::{ElementRef, Html, Selector};

fn get_div_with_class<'a>(root: &'a Html, class: &'static str) -> Vec<ElementRef<'a>> {
    let div_selector = Selector::parse("div").unwrap();
    root.select(&div_selector)
        .filter(|div| div.value().attr("class") == Some(class))
        .collect()
}

fn is_uppercase(word: &str) -> bool {
    word.chars().all(|c| c.is_uppercase())
}

fn to_normalcase(word: &str) -> String {
    let mut result = String::new();

    for ch in word.chars().enumerate() {
        if ch.0 == 0 {
            result.push(ch.1);
        } else {
            for ch in ch.1.to_lowercase() {
                result.push(ch);
            }
        }
    }

    result
}

async fn download_player_info_from(itsf_id: i32, url: &str) -> Result<models::Player, String> {
    let itsf = download::download(&url).await?;

    let nomdujoueur = get_div_with_class(&itsf, "nomdujoueur");
    let nomdujoueur = nomdujoueur.first().ok_or("can't find div nomdujoueur")?;
    let name = nomdujoueur
        .text()
        .next()
        .ok_or("can't find text in nomdujoueur div")?;

    let last_name = name
        .split(" ")
        .filter(|word| is_uppercase(word))
        .map(to_normalcase)
        .collect::<Vec<String>>()
        .join(" ");

    let first_name = name
        .split(" ")
        .filter(|word| !is_uppercase(word))
        .collect::<Vec<&str>>()
        .join(" ");

    // TODO: this only seems to work for ASCII. we need to figure out a way to properly parse UTF.

    let span_selector = Selector::parse("span").unwrap();
    let country_code = nomdujoueur
        .select(&span_selector)
        .next()
        .ok_or("can't find country code")?;
    let country_code = country_code
        .text()
        .next()
        .ok_or("can't find country code text")?;
    if !country_code.starts_with("(") || !country_code.ends_with(")") {
        return Err(format!("invalid country code ({:?})", country_code));
    }
    let country_code = country_code[1..]
        .split(" ")
        .next()
        .ok_or(format!("invalid country code ({:?})", country_code))?;

    let contenu_typeinfojoueur = get_div_with_class(&itsf, "contenu_typeinfojoueur");
    if contenu_typeinfojoueur.len() < 2 {
        return Err(format!(
            "invalid number of contenu_typeinfojoueur ({})",
            contenu_typeinfojoueur.len()
        ));
    }

    let contenu_typeinfojoueur_even = get_div_with_class(&itsf, "contenu_typeinfojoueur even");
    if contenu_typeinfojoueur_even.len() < 1 {
        return Err(format!(
            "invalid number of contenu_typeinfojoueur ({})",
            contenu_typeinfojoueur_even.len()
        ));
    }

    let category = contenu_typeinfojoueur_even[0]
        .text()
        .next()
        .ok_or("can't find category text")?;
    let category = match category {
        "MEN" => Ok(models::PlayerCategory::Men),
        "WOMEN" => Ok(models::PlayerCategory::Women),
        "JUNIOR MALE" => Ok(models::PlayerCategory::JuniorMale),
        "JUNIOR FEMALE" => Ok(models::PlayerCategory::JuniorFemale),
        "SENIOR MALE" => Ok(models::PlayerCategory::SeniorMale),
        "SENIOR FEMALE" => Ok(models::PlayerCategory::SeniorFemale),
        _ => Err(format!("invalid category: {}", category)),
    }?;

    let birth_year = contenu_typeinfojoueur[1]
        .text()
        .next()
        .ok_or("can't find birth year")?;
    let birth_year = birth_year
        .parse::<i32>()
        .unwrap_or_else(|err| {
            log::error!("{}: can't parse birth year '{}': {:?}", url, birth_year, err);
            0
        });

    Ok(models::Player {
        itsf_id,
        first_name: first_name.into(),
        last_name: last_name.into(),
        dtfb_license: None,
        birth_year,
        country_code: Some(country_code.into()),
        category: category.into(),
    })
}

pub async fn download_player_info(itsf_id: i32) -> Result<models::Player, String> {
    let url = format!("https://www.tablesoccer.org/page/player&numlic={}", itsf_id);
    download_player_info_from(itsf_id, &url)
        .await
        .map_err(|msg| format!("Player[{}]: {}", url, msg))
}
