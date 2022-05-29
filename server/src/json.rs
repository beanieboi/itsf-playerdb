#[derive(serde::Serialize)]
struct JsonOk<T: serde::Serialize> {
    data: T,
}

#[derive(serde::Serialize)]
struct JsonErr<T: serde::Serialize> {
    error: T,
}

pub fn ok<T: serde::Serialize>(data: T) -> impl serde::Serialize {
    let ok = JsonOk { data };
    ok
}

pub fn err<T: serde::Serialize>(error: T) -> impl serde::Serialize {
    let err = JsonErr { error };
    err
}
