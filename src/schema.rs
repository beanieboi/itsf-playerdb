// @generated automatically by Diesel CLI.

diesel::table! {
    players (itsf_id) {
        itsf_id -> Int4,
        json_data -> Jsonb,
    }
}
