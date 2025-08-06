// @generated automatically by Diesel CLI.

diesel::table! {
    nodes (id) {
        id -> Nullable<Integer>,
        url -> Text,
        ping -> Integer,
        uptime -> Integer,
        last_checked -> Integer,
        zmq -> Nullable<Integer>,
        cors -> Nullable<Integer>,
    }
}
