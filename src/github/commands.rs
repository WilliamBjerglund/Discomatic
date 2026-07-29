/*
src/github/commands.rs
This file contains the commands for the Github module.
*/

use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![super::requests::request()]
}
