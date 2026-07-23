/*
src/music_player/commands.rs
This file contains the commands for the music_player module. The commands are registered in main.rs and are available to be used by the bot.
*/

use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![
        super::player::join(),
        super::player::play(),
        super::player::stop(),
        super::player::skip(),
    ]
}
