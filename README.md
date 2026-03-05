# rust-discord-bot
Just a random discordbot to learn rust with.


## Notes

This bot isn't really meant to be ran by anyone else. It's simply a for-fun bot for myself and my friends.

View docker-compose.example.yml for variables that are needed for this project.

The underlying filesystem is setup in a certain way for STATIC to work correctly with other docker containers.

Several commands related to AI won't run correctly out of the box.

Feature set is mostly what I find useful with a few for fun commands here and there. There's a lot of bloat with some of the open source bots, and I wanted to craft something efficient and not too overwhelming with the amount of commands there are.


## Features


### AI

- Conversational AI using local Ollama instance when @mentioning the bot
- Currently optimized for gemma3, but expandable to other models
- Stable Diffusion integration with self-hosted instance (Currently not available)

### Anime
- Guess the OP game
- Integrations with [ShokoServer](https://github.com/ShokoAnime/ShokoServer/) for anime library management
- Sonarr integration for anime series tracking

### EmbedFix
- Fixes broken embeds for Instagram, Reddit (vxreddit), and Twitter (fxtwitter) links

### Language
- Kanji and Chinese character search and information
- Detailed character breakdowns and meanings

### Music
- YouTube audio clip extraction (download and edit clips from YouTube videos)
- Voice channel music playback with queue system

### Permission System
- Role-based permission system (admin/mod/trusted)
- Prevents unauthorized users from using destructive or restricted commands

### Random
- Basic utility commands: ping/bing, avatar, uptime, userinfo, guildinfo
- Random fox images
- Weather information lookup
- Deadge command
- Custom response system based on triggers

### AYDY (Are you dead yet?)
- Inspired by the Chinese "报平安" concept
- Daily check-in system with 24-hour reminder messages
- Tracks enrollment and marks users inactive after 48 hours of no response

### Response System
- Configurable trigger-based bot reactions via TOML config file (/app/config.toml)
- Supports custom automated responses to specific messages or patterns

### Stream Following
- Automated notifications when followed users go live
- Currently supports Kick streaming platform

### Ticketing System
- User support ticket creation and management (Work in Progress)

### TMDB (The Movie Database)
- Movie search with detailed information
- TV show search and information lookup

### Utility

#### Papers
- Persistent embed message system for role assignment
- Generic modifiable embedded messages for server information

#### Bot Profile
- Commands for modifying bot profile (avatar, status, etc.)

#### Tags
- Server-specific key-value storage system
- Create, retrieve, and manage custom text snippets

### Website
- Web interface hosted on port 8080
- Administrative tools including:
  - Emoji list viewer
  - Tag management
  - Command documentation
  - Server configuration