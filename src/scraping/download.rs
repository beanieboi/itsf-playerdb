use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

async fn get(url: &str, headers: &[(&str, &str)]) -> Result<String, reqwest::Error> {
    let client = Client::builder()
        .cookie_store(true)
        .danger_accept_invalid_certs(true)
        .build()?;

    let mut request = client.get(url);
    for header in headers {
        request = request.header(header.0, header.1);
    }

    request.send().await?.text().await
}

pub async fn download(url: &str, headers: &[(&str, &str)]) -> Result<String, String> {
    get(url, headers).await.map_err(|err| err.to_string())
}

pub async fn post_json<T, R>(url: &str, headers: &[(&str, &str)], body: &T) -> Result<R, String>
where
    T: Serialize,
    R: DeserializeOwned,
{
    let client = Client::builder()
        .cookie_store(true)
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|err| err.to_string())?;

    let body = serde_json::to_string(body).map_err(|err| err.to_string())?;
    let mut request = client.post(url).header("Content-Type", "application/json").body(body);
    for header in headers {
        request = request.header(header.0, header.1);
    }

    let response = request.send().await.map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{} returned HTTP {}", url, response.status()));
    }

    let response = response.text().await.map_err(|err| err.to_string())?;
    serde_json::from_str(&response).map_err(|err| err.to_string())
}
