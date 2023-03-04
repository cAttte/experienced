use crate::{manager::Error, processor::CommandProcessorError, AppState};
use sqlx::query;
use base64::Engine;
use twilight_model::{
    channel::message::MessageFlags,
    http::{
        attachment::Attachment,
        interaction::{InteractionResponse, InteractionResponseType},
    },
    id::{marker::GuildMarker, Id},
    user::User,
};
use twilight_util::builder::{embed::EmbedBuilder, InteractionResponseDataBuilder};

pub async fn get_level(
    guild_id: Id<GuildMarker>,
    user: User,
    invoker: User,
    token: String,
    state: AppState,
) -> Result<InteractionResponse, CommandProcessorError> {
    #[allow(clippy::cast_possible_wrap)]
    let guild_id = guild_id.get() as i64;
    // Select current XP from the database, return 0 if there is no row
    #[allow(clippy::cast_possible_wrap)]
    let xp = query!(
        "SELECT xp FROM levels WHERE id = $1 AND guild = $2",
        user.id.get() as i64,
        guild_id
    )
    .fetch_optional(&state.db)
    .await?
    .map_or(0, |v| v.xp);
    let rank = query!(
        "SELECT COUNT(*) as count FROM levels WHERE xp > $1 AND guild = $2",
        xp,
        guild_id
    )
    .fetch_one(&state.db)
    .await?
    .count
    .unwrap_or(0)
        + 1;
    #[allow(clippy::cast_sign_loss)]
    let level_info = mee6::LevelInfo::new(xp as u64);
    let content = if user.bot {
        "Bots aren't ranked, that would be silly!".to_string()
    } else if invoker == user {
        if xp == 0 {
            "You aren't ranked yet, because you haven't sent any messages!".to_string()
        } else {
            return generate_level_response(state, token, user, level_info, rank).await;
        }
    } else if xp == 0 {
        format!(
            "{}#{} isn't ranked yet, because they haven't sent any messages!",
            user.name,
            user.discriminator()
        )
    } else {
        return generate_level_response(state, token, user, level_info, rank).await;
    };
    Ok(InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(
            InteractionResponseDataBuilder::new()
                .flags(MessageFlags::EPHEMERAL)
                .embeds([EmbedBuilder::new().description(content).build()])
                .build(),
        ),
    })
}

async fn generate_level_response(
    state: AppState,
    token: String,
    user: User,
    level_info: mee6::LevelInfo,
    rank: i64,
) -> Result<InteractionResponse, CommandProcessorError> {
    tokio::task::spawn(async move {
        let Err(err) = add_card(state.clone(), &token, user, level_info, rank).await else { return; };
        warn!("{err:#?}");
        let interaction_client = state.client.interaction(state.my_id);
        let embed = EmbedBuilder::new().description(err.to_string()).build();
        let embeds = &[embed];
        match interaction_client.create_followup(&token).embeds(embeds) {
            Ok(awaitable) => {
                if let Err(e) = awaitable.await {
                    warn!("{e:#?}");
                };
            }
            Err(e) => {
                warn!("{e:#?}");
            }
        };
    });
    Ok(InteractionResponse {
        kind: InteractionResponseType::DeferredChannelMessageWithSource,
        data: None,
    })
}

async fn add_card(
    state: AppState,
    token: &str,
    user: User,
    level_info: mee6::LevelInfo,
    rank: i64,
) -> Result<(), CommandProcessorError> {
    let interaction_client = state.client.interaction(state.my_id);
    #[allow(clippy::cast_possible_wrap)]
    let non_color_customizations = query!(
        "SELECT font, toy_image FROM custom_card WHERE id = $1",
        user.id.get() as i64
    )
    .fetch_one(&state.db)
    .await;
    let (font, toy) = non_color_customizations.map_or_else(
        |_| ("Roboto".to_string(), None),
        |v| (v.font.unwrap_or_else(|| "Roboto".to_string()), v.toy_image),
    );
    let avatar = get_avatar(&state, &user).await?;
    #[allow(clippy::cast_precision_loss)]
    let png = crate::render_card::render(
        state.clone(),
        crate::render_card::Context {
            level: level_info.level(),
            rank,
            name: user.name.clone(),
            discriminator: user.discriminator().to_string(),
            percentage: get_percentage_bar_as_pixels(level_info.percentage()),
            current: level_info.xp(),
            needed: mee6::xp_needed_for_level(level_info.level() + 1),
            colors: crate::colors::Colors::for_user(&state.db, user.id).await,
            font,
            toy,
            avatar,
        },
    )
    .await?;
    let card = Attachment {
        description: Some(format!(
            "{}#{} is level {} (rank #{}), and is {}% of the way to level {}.",
            user.name,
            user.discriminator(),
            level_info.level(),
            rank,
            (level_info.percentage() * 100.0).round(),
            level_info.level() + 1
        )),
        file: png,
        filename: "card.png".to_string(),
        id: 0,
    };
    interaction_client
        .create_followup(token)
        .attachments(&[card])?
        .allowed_mentions(None)
        .await?;
    Ok(())
}

pub fn leaderboard(guild_id: Id<GuildMarker>) -> InteractionResponse {
    let guild_link = format!("https://xp.valk.sh/{guild_id}");
    let embed = EmbedBuilder::new()
        .description(format!("[Click to view the leaderboard!]({guild_link})"))
        .color(crate::THEME_COLOR)
        .build();
    let data = InteractionResponseDataBuilder::new()
        .embeds([embed])
        .build();
    InteractionResponse {
        data: Some(data),
        kind: InteractionResponseType::ChannelMessageWithSource,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn get_percentage_bar_as_pixels(percentage: f64) -> u64 {
    percentage.mul_add(700.0, 40.0) as u64
}

async fn get_avatar(state: &AppState, user: &User) -> Result<String, Error> {
    let url = user.avatar.map_or_else(
        || {
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}/{}.png",
                user.id,
                user.discriminator % 5
            )
        },
        |hash| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{}.png",
                user.id, hash
            )
        },
    );
    let png = state.http.get(url).send().await?.bytes().await?;
    let data = format!("data:image/png;base64,{}", BASE64_ENGINE.encode(png));
    Ok(data)
}

const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::general_purpose::NO_PAD,
);
