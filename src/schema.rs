// @generated automatically by Diesel CLI.

diesel::table! {
    player_images (itsf_id) {
        itsf_id -> Integer,
        data -> Binary,
        format -> Text,
    }
}

diesel::table! {
    players (itsf_id) {
        itsf_id -> Integer,
        json_data -> Binary,
    }
}

diesel::allow_tables_to_appear_in_same_query!(player_images, players,);
