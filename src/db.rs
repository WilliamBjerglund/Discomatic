/*
We should make a actual shared SQL database instead of JSON files i realised
This file just sets up the shared database and all the modules that need persisted data can access it
*/

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::Error;

const DATABASE_FILE: &str = "BotDatabase.db";

// function to open and or create a database all when needed
// all tables that exist Returns a connection pool shared across the bot
pub async fn init_pool() -> Result<SqlitePool, Error> {
    let options = SqliteConnectOptions::new()
        .filename(DATABASE_FILE)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new().connect_with(options).await?;

    // Total recorded League of Legends playtime per user, in seconds.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS playtime_totals (
            user_id INTEGER PRIMARY KEY,
            total_seconds INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // most recent playtime message per channel
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS leaderboard_messages (
            channel_id INTEGER PRIMARY KEY,
            message_id INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // One leaderboard message per channel
    // next due will oinly go when auto refresh is on for the channel.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS channel_leaderboards (
            channel_id INTEGER PRIMARY KEY,
            guild_id INTEGER NOT NULL,
            message_id INTEGER NOT NULL,
            auto_enabled INTEGER NOT NULL DEFAULT 0,
            interval_seconds INTEGER NOT NULL DEFAULT 0,
            next_due INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
