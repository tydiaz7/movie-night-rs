use std::collections::HashMap;
use std::env;
use rand::Rng;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use serde_json::{Result, Value};

use serenity::async_trait;
use serenity::framework::standard::macros::{command, group, hook};
use serenity::framework::standard::{Args, CommandResult, StandardFramework};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::utils::MessageBuilder;
use tokio::sync::RwLock;

// A container type is created for inserting into the Client's `data`, which
// allows for data to be accessible across all events and framework commands, or
// anywhere else that has a copy of the `data` Arc.
// These places are usually where either Context or Client is present.
//
// Documentation about TypeMap can be found here:
// https://docs.rs/typemap_rev/0.1/typemap_rev/struct.TypeMap.html
struct CommandCounter;

impl TypeMapKey for CommandCounter {
    type Value = Arc<RwLock<HashMap<String, u64>>>;
}

struct MessageCount;

impl TypeMapKey for MessageCount {
    // While you will be using RwLock or Mutex most of the time you want to modify data,
    // sometimes it's not required; like for example, with static data, or if you are using other
    // kinds of atomic operators.
    //
    // Arc should stay, to allow for the data lock to be closed early.
    type Value = Arc<AtomicUsize>;
}

struct MovieSuggestions;

impl TypeMapKey for MovieSuggestions {
    type Value = Arc<String>;
}

#[group]
#[commands(ping, command_usage, uwu_count, movie_suggestions, add)]
struct General;

#[hook]
async fn before(ctx: &Context, msg: &Message, command_name: &str) -> bool {
    println!("Running command '{}' invoked by '{}'", command_name, msg.author.tag());

    let counter_lock = {
        // While data is a RwLock, it's recommended that you always open the lock as read.
        // This is mainly done to avoid Deadlocks for having a possible writer waiting for multiple
        // readers to close.
        let data_read = ctx.data.read().await;

        // Since the CommandCounter Value is wrapped in an Arc, cloning will not duplicate the
        // data, instead the reference is cloned.
        // We wrap every value on in an Arc, as to keep the data lock open for the least time possible,
        // to again, avoid deadlocking it.
        data_read.get::<CommandCounter>().expect("Expected CommandCounter in TypeMap.").clone()
    };

    // Just like with client.data in main, we want to keep write locks open the least time
    // possible, so we wrap them on a block so they get automatically closed at the end.
    {
        // The HashMap of CommandCounter is wrapped in an RwLock; since we want to write to it, we will
        // open the lock in write mode.
        let mut counter = counter_lock.write().await;

        // And we write the amount of times the command has been called to it.
        let entry = counter.entry(command_name.to_string()).or_insert(0);
        *entry += 1;
    }

    true
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.id != ctx.cache.current_user_id()
           && msg.content.to_lowercase().contains("uwu")
        {
            let determinator: u8 = rand::thread_rng().gen();
                if determinator == 12 {
                    let uwu_response = MessageBuilder::new().push("OwO").build();
                    if let Err(why) = msg.channel_id.say(&ctx.http, &uwu_response).await {
                        println!("Error sending message: {:?}", why);
                    };
                } else {
                    let uwu_response = MessageBuilder::new().push("UwU").build();
                    if let Err(why) = msg.channel_id.say(&ctx.http, &uwu_response).await {
                        println!("Error sending message: {:?}", why);
                    };
                }
            // Since data is located in Context, this means you are also able to use it within events!
            let count = {
                let data_read = ctx.data.read().await;
                data_read.get::<MessageCount>().expect("Expected MessageCount in TypeMap.").clone()
            };

            // Here, we are checking how many "owo" there are in the message content.
            let uwu_in_msg = msg.content.to_ascii_lowercase().matches("uwu").count();

            // Atomic operations with ordering do not require mut to be modified.
            // In this case, we want to increase the message count by 1.
            // https://doc.rust-lang.org/std/sync/atomic/struct.AtomicUsize.html#method.fetch_add
            count.fetch_add(uwu_in_msg, Ordering::SeqCst);
        }
        if msg.author.id != ctx.cache.current_user_id() {
            let movie_sugg = {
                let data_read = ctx.data.read().await;
                data_read.get::<MovieSuggestions>().expect("Expected MovieSuggestions in TypeMap").clone()
            };
            let is_movie = msg.content.to_ascii_lowercase().contains("movie");
            if is_movie {
                // movie_sugg.(msg.content, Ordering::Relaxed);
                let response = MessageBuilder::new()
                .push("User ")
                .push_bold_safe(&msg.author.name)
                .push(" added a movie.")
                .build();
                if let Err(why) = msg.channel_id.say(&ctx.http, &response).await {
                    println!("Error sending message: {:?}", why);
                };
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_API_TOKEN").expect("Token not found");

    let framework = StandardFramework::new()
    .configure(|c| c.with_whitespace(true).prefix("!"))
    .before(before)
    .group(&GENERAL_GROUP);

    let intents = GatewayIntents::GUILD_MESSAGES
                  | GatewayIntents::DIRECT_MESSAGES
                  | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
    .event_handler(Handler)
    .framework(framework)
    .await
    .expect("Err creating client");

    // This is where we can initially insert the data we desire into the "global" data TypeMap.
    // client.data is wrapped on a RwLock, and since we want to insert to it, we have to open it in
    // write mode, but there's a small thing catch:
    // There can only be a single writer to a given lock open in the entire application, this means
    // you can't open a new write lock until the previous write lock has closed.
    // This is not the case with read locks, read locks can be open indefinitely, BUT as soon as
    // you need to open the lock in write mode, all the read locks must be closed.
    //
    // You can find more information about deadlocks in the Rust Book, ch16-03:
    // https://doc.rust-lang.org/book/ch16-03-shared-state.html
    //
    // All of this means that we have to keep locks open for the least time possible, so we put
    // them inside a block, so they get closed automatically when dropped.
    // If we don't do this, we would never be able to open the data lock anywhere else.
    //
    // Alternatively, you can also use `ClientBuilder::type_map_insert` or
    // `ClientBuilder::type_map` to populate the global TypeMap without dealing with the RwLock.
    {
        // Open the data lock in write mode, so keys can be inserted to it.
        let mut data = client.data.write().await;

        // The CommandCounter Value has the following type:
        // Arc<RwLock<HashMap<String, u64>>>
        // So, we have to insert the same type to it.
        data.insert::<CommandCounter>(Arc::new(RwLock::new(HashMap::default())));

        data.insert::<MessageCount>(Arc::new(AtomicUsize::new(0)));

        data.insert::<MovieSuggestions>(Arc::new(String::default()));
    }

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;

    Ok(())
}

/// Usage: `~command_usage <command_name>`
/// Example: `~command_usage ping`
#[command]
async fn command_usage(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let command_name = match args.single_quoted::<String>() {
        Ok(x) => x,
        Err(_) => {
            msg.reply(ctx, "I require an argument to run this command.").await?;
            return Ok(());
        },
    };

    // Yet again, we want to keep the locks open for the least time possible.
    let amount = {
        // Since we only want to read the data and not write to it, we open it in read mode,
        // and since this is open in read mode, it means that there can be multiple locks open at
        // the same time, and as mentioned earlier, it's heavily recommended that you only open
        // the data lock in read mode, as it will avoid a lot of possible deadlocks.
        let data_read = ctx.data.read().await;

        // Then we obtain the value we need from data, in this case, we want the command counter.
        // The returned value from get() is an Arc, so the reference will be cloned, rather than
        // the data.
        let command_counter_lock =
        data_read.get::<CommandCounter>().expect("Expected CommandCounter in TypeMap.").clone();

        let command_counter = command_counter_lock.read().await;
        // And we return a usable value from it.
        // This time, the value is not Arc, so the data will be cloned.
        command_counter.get(&command_name).map_or(0, |x| *x)
    };

    if amount == 0 {
        msg.reply(ctx, format!("The command `{}` has not yet been used.", command_name)).await?;
    } else {
        msg.reply(
                ctx,
        format!("The command `{}` has been used {} time/s this session!", command_name, amount),
        )
        .await?;
    }

    Ok(())
}

#[command]
async fn uwu_count(ctx: &Context, msg: &Message) -> CommandResult {
    let raw_count = {
        let data_read = ctx.data.read().await;
        data_read.get::<MessageCount>().expect("Expected MessageCount in TypeMap.").clone()
    };

    let count = raw_count.load(Ordering::Relaxed);

    if count == 1 {
        msg.reply(
                ctx,
        "You are the first one to say uwu this session! *because it's on the command name* :P",
        )
        .await?;
    } else {
        msg.reply(ctx, format!("UWU Has been said {} times!", count)).await?;
    }

    Ok(())
}

#[command]
async fn movie_suggestions(ctx: &Context, msg: &Message) -> CommandResult {
    let raw_list = {
        let data_read = ctx.data.read().await;
        data_read.get::<MovieSuggestions>().expect("Expected MovieSuggestions in TypeMap").clone()
    };

    msg.reply(ctx, format!("I have no idea what I'm doing lul {}", raw_list)).await?;
    Ok(())
}

#[command]
async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let imdb_url = match args.single_quoted::<String>() {
        Ok(x) => x,
        Err(_) => {
            msg.reply(ctx, "I require an argument to run this command.").await?;
            return Ok(());
        },
    };
    // TODO: Account for movie title only (Nice to have)? or IMDb ID only
    // Scrape message embed instead?
    let split: usize = {
        if imdb_url.starts_with("https") {
            4
        } else {
            2
        }
    };
        let imdb_id = imdb_url.split("/").nth(split).unwrap();
        msg.reply(ctx, format!("I think this is the ID from the URL given: {}", imdb_id)).await;
        let res = reqwest::get(format!("https://api.themoviedb.org/3/find/{}?api_key=26dddc9c9f07cfa0d8930cb732dc6400&language=en-US&external_source=imdb_id", imdb_id)).await?.text().await?;
        let json_res: Value = serde_json::from_str(&res)?; // TODO: MovieObject
        println!("{}",json_res["movie_results"][0]["title"]);
        let tmbd_info = MessageBuilder::new()
            .push("Movie title: ")
            .push_safe(&json_res["movie_results"][0]["title"])//+6
//                        +36+
//                        +++++++++++++++++++++++++++3333333333333333333333333333333333333333333333333333333333333333333333
            .push("\nSynopsis: ")
            .push_safe(&json_res["movie_results"][0]["overview"])
            .build();
        if let Err(why) = msg.channel_id.say(&ctx.http, &tmbd_info).await {
            println!("Error sending message: {:?}", why);
        };
    Ok(())
}

