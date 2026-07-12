# Introduction

This module provides voice playblack and music queue to the discord bot.

It allows users to play audio from YouTube by submitting either a direct link or a search query. The bot joins the user’s current voice channel automatically, retrieves audio using `yt-dlp`, and manages playback through Songbird.

The implementation was created with the `phoxwupsh/turto` project as inspiration.

# Current Features

- It can join and leave a channel i almost forgot the leave part at first....
  - /join / leave

- Youtube playback
  - /play gives support for links and search querys
  - also auto joins channel if /play is given and not in vc

# Dependencies

- yt-dlp
- ffmpeg (a dependency of yt-dlp)
