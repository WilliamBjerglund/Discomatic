/*
league/commands.rs
This file contains all the commands for the league module.
*/

use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![super::playtime::playtime(), super::playtime::playtimeauto()]
}
