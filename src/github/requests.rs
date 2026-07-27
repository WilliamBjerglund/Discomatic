/*
src/github/requests.rs

This file is a fun project attempting to learn and use the Github API to create issues on my Discord bots repository.
The idea is that users can just type /request in Discord type in what they want and i will have it on my Github to do later.
*/

use std::env;

use poise::Modal;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use crate::{Context, Error};
