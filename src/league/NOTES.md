# Introduction

This module tracks how long Discord users spend playing League of Legends and displays the results as a server leaderboard.

It uses Discord presence updates to detect when a user starts or stops playing, stores completed playtime totals in SQLite, and supports both manual and automatically refreshed leaderboards.

The implementation was created with the `Gummiees/playtime-discord-bot` project as a reference.

# Current Features

- Detects League of Legends sessions from Discord activity updates

- Displays the top 10 users by total playtime

- Provides /playtime and /playtimeauto commands for updates

# Important Notes

- All Active sessions are lostd if and when the bot restarts because they are currently only stored in memory.

- Discord presence access must be enabled for session detection to work.

- Users who appear offline or disable activity sharing are not tracked.
