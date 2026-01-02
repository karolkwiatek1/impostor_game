use std::env;
use std::sync::Arc;
use dotenv::dotenv;
use rand::seq::SliceRandom;
use rand::prelude::IndexedRandom;
use serde::Deserialize;
use serenity::all::{
    ButtonStyle, CommandOptionType, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    CreateCommand, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, GatewayIntents, GuildId, Interaction, Ready, UserId,
    EditInteractionResponse, ResolvedValue,
};
use serenity::async_trait;
use serenity::prelude::*;

// --- DATA STRUCTURES ---

#[derive(Debug, Deserialize, Clone)]
struct WordPair {
    common: String,   // Word for innocent players
    impostor: String, // Word for the impostor
}

// Expanded game state
struct GameState {
    impostor_ids: Vec<UserId>,      // List of impostors (can be multiple)
    jester_id: Option<UserId>,      // Jester ID (if exists)
    common_word: String,
    impostor_word: String,
    participants: Vec<UserId>,
    // Round configuration
    impostors_know_each_other: bool,
    impostor_has_hint: bool,
}

struct WordDatabase;
impl TypeMapKey for WordDatabase {
    type Value = Arc<Vec<WordPair>>;
}

struct CurrentGame;
impl TypeMapKey for CurrentGame {
    type Value = Arc<RwLock<GameState>>;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is ready to play!", ready.user.name);

        let command = CreateCommand::new("impostor")
            .description("Starts an advanced game round")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "players", "List of players (@user1 @user2...)")
                    .required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::Integer, "impostor_count", "How many impostors?")
                    .required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::Boolean, "hint", "Does the Impostor see the Common Word?")
                    .required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::Boolean, "know_each_other", "Do Impostors know each other?")
                    .required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::Boolean, "jester", "Is there a Jester role?")
                    .required(true),
            );

        // Using Guild ID for faster registration
        let guild_id = env::var("GUILD_ID").expect("Missing guild id");
        let guild_id: u64 = guild_id
            .parse()
            .expect("GUILD_ID must be a valid 64-bit integer");
        let guild_id = GuildId::new(guild_id as u64);
        let _ = guild_id.set_commands(&ctx.http, vec![command]).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                if command.data.name == "impostor" {
                    let _ = command.create_response(&ctx.http, CreateInteractionResponse::Defer(CreateInteractionResponseMessage::new())).await;

                    // --- 1. ARGUMENT PARSING ---
                    let mut participants_raw = String::new();
                    let mut impostor_count = 1;
                    let mut impostor_has_hint = false;
                    let mut impostors_know_each_other = false;
                    let mut has_jester = false;

                    // Get options from ResolvedOptions
                    let options = command.data.options();
                    for option in options {
                        match option.name {
                            "players" => {
                                if let ResolvedValue::String(s) = option.value {
                                    participants_raw = s.to_string();
                                }
                            },
                            "impostor_count" => {
                                if let ResolvedValue::Integer(i) = option.value {
                                    impostor_count = i as usize;
                                }
                            },
                            "hint" => {
                                if let ResolvedValue::Boolean(b) = option.value {
                                    impostor_has_hint = b;
                                }
                            },
                            "know_each_other" => {
                                if let ResolvedValue::Boolean(b) = option.value {
                                    impostors_know_each_other = b;
                                }
                            },
                            "jester" => {
                                if let ResolvedValue::Boolean(b) = option.value {
                                    has_jester = b;
                                }
                            },
                            _ => {}
                        }
                    }

                    // Process player list
                    let mut participants = Vec::new();
                    let mut mentions_debug = String::new();
                    let parts: Vec<&str> = participants_raw.split_whitespace().collect();
                    for part in parts {
                        let cleaned: String = part.chars().filter(|c| c.is_digit(10)).collect();
                        if let Ok(id_u64) = cleaned.parse::<u64>() {
                            let uid = UserId::new(id_u64);
                            // Avoid duplicates
                            if !participants.contains(&uid) {
                                participants.push(uid);
                                mentions_debug.push_str(&format!("<@{}> ", id_u64));
                            }
                        }
                    }

                    // PLAYER COUNT VALIDATION
                    let required_roles = impostor_count + if has_jester { 1 } else { 0 };
                    // Must have at least 1 innocent left
                    if participants.len() <= required_roles {
                        let msg = format!("Not enough players! Need more than {} (Impostors + Jester), but you provided {}.", required_roles, participants.len());
                        let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content(msg)).await;
                        return;
                    }

                    let data = ctx.data.read().await;
                    let words_db = data.get::<WordDatabase>().expect("Missing word database").clone();
                    let game_state_lock = data.get::<CurrentGame>().expect("Missing game state").clone();

                    if words_db.is_empty() {
                         let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content("The word database is empty!")).await;
                         return;
                    }

                    // --- 2. RANDOMIZATION LOGIC (SCOPE) ---
                    let (random_pair, selected_impostors, selected_jester) = {
                        let mut rng = rand::rng();

                        // Clone player list to shuffle it
                        let mut shuffled_players = participants.clone();
                        shuffled_players.shuffle(&mut rng);

                        let pair = words_db.choose(&mut rng).unwrap().clone();

                        // Take Impostors from the beginning of the shuffled list
                        let imps = shuffled_players.iter().take(impostor_count).cloned().collect::<Vec<_>>();

                        // If Jester exists, take the next one
                        let jest = if has_jester {
                            shuffled_players.get(impostor_count).cloned()
                        } else {
                            None
                        };

                        (pair, imps, jest)
                    };
                    // RNG dies here, safe to await

                    // --- 3. SAVE GAME STATE ---
                    {
                        let mut game = game_state_lock.write().await;
                        game.impostor_ids = selected_impostors.clone();
                        game.jester_id = selected_jester;
                        game.common_word = random_pair.common.clone();
                        game.impostor_word = random_pair.impostor.clone();
                        game.participants = participants.clone();
                        game.impostors_know_each_other = impostors_know_each_other;
                        game.impostor_has_hint = impostor_has_hint;
                    }

                    // --- 4. SEND MESSAGE ---
                    let button = CreateButton::new("check_word")
                        .label("🕵️ Check your role and word")
                        .style(ButtonStyle::Primary);

                    let row = CreateActionRow::Buttons(vec![button]);

                    let msg_content = format!(
                        "**Game Started**\n\n Players: {}\n Impostors: {}\n Jester: {}\n\nClick the button below to see who you are!",
                        mentions_debug,
                        impostor_count,
                        if has_jester { "YES" } else { "NO" }
                    );

                    let _ = command.edit_response(&ctx.http, EditInteractionResponse::new().content(msg_content).components(vec![row])).await;

                    println!("START: Impostors: {:?}, Jester: {:?}, Word: {}", selected_impostors, selected_jester, random_pair.common);
                }
            }

            Interaction::Component(component) => {
                if let ComponentInteractionDataKind::Button = component.data.kind {
                    if component.data.custom_id == "check_word" {

                        let user_id = component.user.id;

                        let data = ctx.data.read().await;
                        let game_lock = data.get::<CurrentGame>().expect("Missing game state").clone();
                        let game = game_lock.read().await;

                        if !game.participants.contains(&user_id) {
                             let _ = component.create_response(&ctx.http, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("You are not participating in this game.").ephemeral(true))).await;
                             return;
                        }

                        // --- BUILD RESPONSE FOR SPECIFIC ROLE ---
                        let mut response_text = String::new();

                        if game.impostor_ids.contains(&user_id) {
                            // == IS IMPOSTOR ==
                            response_text.push_str(&format!("**IMPOSTOR**\n"));

                            if game.impostor_has_hint {
                                response_text.push_str(&format!("\n Hint: ||{}||", game.impostor_word));
                            }

                            if game.impostors_know_each_other && game.impostor_ids.len() > 1 {
                                let others: Vec<String> = game.impostor_ids.iter()
                                    .filter(|&id| *id != user_id)
                                    .map(|id| format!("<@{}>", id))
                                    .collect();

                                if !others.is_empty() {
                                    response_text.push_str(&format!("\nYour partners: {}", others.join(", ")));
                                }
                            }

                        } else if Some(user_id) == game.jester_id {
                            // == IS JESTER ==
                            // Jester gets the same word as innocents, but knows they are the Jester
                            response_text.push_str(&format!("**JESTER**\n **{}**\n\n", game.common_word));

                        } else {
                            // == IS INNOCENT ==
                            response_text.push_str(&format!("Innocent.\n**{}**", game.common_word));
                        }

                        let _ = component.create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new()
                                    .content(response_text)
                                    .ephemeral(true) // ONLY FOR RECIPIENT
                            ),
                        ).await;
                    }
                }
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Missing token");

    let mut words_list = Vec::new();
    let rdr_result = csv::Reader::from_path("dictionary.csv");

    match rdr_result {
        Ok(mut rdr) => {
            for result in rdr.deserialize() {
                match result {
                    Ok(record) => words_list.push(record),
                    Err(e) => println!("CSV row error: {:?}", e),
                }
            }
            println!("Loaded {} word pairs from CSV file.", words_list.len());
        }
        Err(e) => panic!("Cannot open dictionary.csv: {:?}", e),
    }

    if words_list.is_empty() {
        panic!("CSV file is empty or malformatted!");
    }

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    // Initial empty state
    let initial_game_state = GameState {
        impostor_ids: Vec::new(),
        jester_id: None,
        common_word: String::new(),
        impostor_word: String::new(),
        participants: Vec::new(),
        impostors_know_each_other: false,
        impostor_has_hint: false,
    };

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .type_map_insert::<WordDatabase>(Arc::new(words_list))
        .type_map_insert::<CurrentGame>(Arc::new(RwLock::new(initial_game_state)))
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}