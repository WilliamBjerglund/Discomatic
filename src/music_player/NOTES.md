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
  - it will now leave after 15 minutes provided it has had nothing to do in those 15 minutes
    - it also leaves after 5 if `alone` and properly handles being manually disconnected.

# Dependencies

- yt-dlp
- ffmpeg (a dependency of yt-dlp)

# Crazy Ideas

- `Join Sound even whilst music is playing` the idea here is to have a Join Sound that could be attached to a user based on commands or rmeoved using urls when a user joins a VC with the bot in it a Sound is played attached to the user but the way we do it is Audio Mixing whihc ffmpeg supports. so we will redesign the pipeline such that we can have the music play at that exact moment at just say 20% volume and then 100% volume for the join sound that way the join sound is prominent but the music is still noticeable.
