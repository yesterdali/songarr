# songarr-discord

A Discord bot that plays music from your Songarr server in voice channels —
including your endless personalized **Wave**. Each Discord user links their own
Songarr account, so `/wave` is tuned to *their* taste.

## Build requirements

The voice engine (songbird) needs **libopus** at build time. Install it so the
build links the system library instead of compiling Opus from source:

- Debian/Ubuntu: `sudo apt install libopus-dev pkg-config`
- Fedora: `sudo dnf install opus-devel pkgconf-pkg-config`
- macOS: `brew install opus pkg-config`

Then: `cargo build -p songarr-discord --release`.

## Discord setup

1. Create an application + bot at <https://discord.com/developers/applications>.
2. Copy the **bot token**.
3. No privileged intents are required (the bot only uses slash commands + voice
   state). Leave Message Content off.
4. Invite the bot with the `bot` and `applications.commands` scopes and at least
   these permissions: **Connect**, **Speak**, **Send Messages**.

## Configuration (environment variables)

| Variable | Required | Purpose |
|---|---|---|
| `DISCORD_TOKEN` | yes | Bot token. |
| `SONGARR_URL` | no | Default Songarr base URL offered when a user runs `/link` without one. |
| `SONGARR_DISCORD_DB` | no | SQLite path for account links (default `songarr-discord.db`). |
| `DISCORD_TEST_GUILD` | no | Register slash commands to this guild ID instantly (otherwise global registration can take ~1h to appear). Great for testing. |

## Run

```sh
DISCORD_TOKEN=... SONGARR_URL=https://songarr.example.com \
  cargo run -p songarr-discord --release
```

## Commands

- `/link <username> <password> [server]` — link your Songarr account (reply is
  ephemeral; only the salt + derived token are stored, never the password).
- `/unlink` — remove your link.
- `/play <query>` — search your library and play the top match.
- `/wave` — start your endless personalized Wave; it auto-refills as it drains.
- `/skip`, `/pause`, `/resume`, `/stop` — playback control.
- `/queue` — show what's coming up.
- `/nowplaying` — show the current track.

All playback commands require you to be in a voice channel.

## Notes

- Audio is streamed from Songarr as MP3 and re-encoded to Opus by songbird for
  Discord. (The native-format guarantee applies to the *app's* downloads, not to
  realtime voice — Discord is always Opus.)
- Each user's Wave/library is their own; the bot acts as whoever invoked the
  command, using their stored credentials.
