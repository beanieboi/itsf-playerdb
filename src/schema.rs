// @generated automatically by Diesel CLI.

diesel::table! {
    player_images (itsf_id) {
        itsf_id -> Int4,
        data -> Bytea,
        format -> Text,
    }
}

diesel::table! {
    players (itsf_id) {
        itsf_id -> Int4,
        json_data -> Jsonb,    }
}

diesel::allow_tables_to_appear_in_same_query!(player_images, players,);
