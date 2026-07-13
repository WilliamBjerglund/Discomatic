use crate::{Data, Error};

pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![super::dice::roll()]
}
