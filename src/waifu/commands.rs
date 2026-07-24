/*
waifu/commands.rs
This file contains the commands for the waifu module. The commands are registered in main.rs and are available to be used by the bot.
*/

use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![super::nekos::waifu()]
}
