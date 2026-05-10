use actix_web::{http::StatusCode, HttpResponse};

#[derive(serde::Serialize)]
struct JsonOk<T: serde::Serialize> {
    data: T,
}

#[derive(serde::Serialize)]
struct JsonErr<T: serde::Serialize> {
    error: T,
}

pub fn ok<T: serde::Serialize>(data: T) -> impl serde::Serialize {
    JsonOk { data }
}

pub fn err<T: serde::Serialize>(error: T) -> impl serde::Serialize {
    JsonErr { error }
}

pub fn response<T: serde::Serialize>(status: StatusCode, data: T) -> HttpResponse {
    let body = serde_json::to_vec(&data).expect("JSON serialization failed");
    HttpResponse::build(status)
        .content_type("application/json; charset=utf-8")
        .body(body)
}
