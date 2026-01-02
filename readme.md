# Impostor / Spyfall Discord Bot

A Discord bot written in Rust using the `serenity` library. It facilitates a "Spyfall" style party game where players receive a secret word, while one or more "Impostors" receive no word (or a hint) and try to blend in.

## Features

* **Role Assignment:** Automatically assigns roles: Innocents, Impostors, and the Jester.
* **Privacy First:** Uses **Ephemeral Messages** (buttons) to reveal secret words. No Direct Messages (DMs) are required, preventing "Cannot send messages to this user" errors.
* **Highly Configurable:** Rounds can be customized via slash commands:
    * Set the number of Impostors.
    * Enable/Disable hints for Impostors.
    * Enable/Disable Impostor co-op (seeing other Impostors' names).
    * Enable/Disable the Jester role.
* **Custom Word Banks:** Loads word pairs dynamically from a local `dictionary.csv` file.

## Prerequisites

* [Rust](https://www.rust-lang.org/) and Cargo installed.
* A Discord Bot Token (from the [Discord Developer Portal](https://discord.com/developers/applications)).

## Setup

1.  **Clone the repository:**
    ```bash
    git clone <your-repo-url>
    cd impostor_bot
    ```

2.  **Create configuration file:**
    Create a file named `.env` in the root directory and add your bot token:
    ```env
    DISCORD_TOKEN=your_actual_token_here
    GUILD_ID=id_of_your_server
    ```

3.  **Prepare the word list:**
    Ensure a file named `dictionary.csv` exists in the root directory. It must have the following header and structure:
    ```csv
    common,impostor
    Beach,Pool
    School,University
    Coffee,Tea
    ```

4.  **Run the bot:**
    ```bash
    cargo run
    ```

## Usage

Once the bot is online, use the slash command in Discord:

```text
/start_game players: @User1 @User2 @User3 impostor_count: 1 hint: False know_each_other: False jester: False