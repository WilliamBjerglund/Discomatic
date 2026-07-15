// src/music_player/commands.rs

use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![
        super::player::join(),
        super::player::play(),
        super::player::stop(),
        super::player::skip(),
    ]
}
