# Game Spoof GUI

A small Windows utility that fakes game launches so Discord detects you as
"playing" a game. Lets you complete "play a game" quests without installing
the games.

## How it works
The app downloads Discord's list of detectable games. When you press Run, it
copies itself under the game's executable name into a `games` folder next to
the app and starts it as a small window. Discord sees that process and shows
the game as running. No real game files are downloaded.

## Build
1. Install Rust: https://rustup.rs
2. `cargo build --release`
3. Run `target/release/game_spoof_rust.exe`

## Disclaimer
This tool may violate Discord's Terms of Service and could put your account
at risk. Use at your own risk. Not affiliated with or endorsed by Discord.