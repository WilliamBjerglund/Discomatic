use poise::serenity_prelude as serenity;
use sqlx::SqlitePool;

pub fn start(ctx: &serenity::Context, pool: SqlitePool) {
    // Automatic leaderboard refreshes.
    tokio::spawn(super::playtime::run_auto_leaderboard_loop(
        ctx.http.clone(),
        pool.clone(),
    ));

    // Bot status leaderboard rotation.
    tokio::spawn(super::status::run_status_update_loop(ctx.clone(), pool));
}
