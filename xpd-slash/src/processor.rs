use crate::{Error, SlashState};
use twilight_interactions::command::CommandModel;
use twilight_model::{
    application::{
        command::CommandType,
        interaction::{
            application_command::CommandData, Interaction, InteractionData, InteractionType,
        },
    },
    http::interaction::{InteractionResponse, InteractionResponseType},
    id::{marker::GuildMarker, Id},
    user::User,
};

pub async fn process(
    interaction: Interaction,
    state: SlashState,
) -> Result<InteractionResponse, Error> {
    Ok(if interaction.kind == InteractionType::ApplicationCommand {
        process_app_cmd(interaction, state).await?
    } else {
        InteractionResponse {
            kind: InteractionResponseType::Pong,
            data: None,
        }
    })
}

async fn process_app_cmd(
    interaction: Interaction,
    state: SlashState,
) -> Result<InteractionResponse, Error> {
    #[cfg(debug_assertions)]
    trace!("{interaction:#?}");
    let data = if let Some(data) = interaction.data {
        if let InteractionData::ApplicationCommand(cmd) = data {
            *cmd
        } else {
            return Err(Error::WrongInteractionData);
        }
    } else {
        return Err(Error::NoInteractionData);
    };
    let invoker = match interaction.member {
        Some(val) => val.user,
        None => interaction.user,
    }
    .ok_or(Error::NoInvoker)?;
    let guild_id = interaction.guild_id.ok_or(Error::NoGuildId)?;
    match data.kind {
        CommandType::ChatInput => {
            process_slash_cmd(data, guild_id, invoker, state, interaction.token).await
        }
        CommandType::User => {
            process_user_cmd(data, guild_id, invoker, state, interaction.token).await
        }
        CommandType::Message => {
            process_msg_cmd(data, guild_id, invoker, state, interaction.token).await
        }
        _ => Err(Error::WrongInteractionData),
    }
}

async fn process_slash_cmd(
    data: CommandData,
    guild_id: Id<GuildMarker>,
    invoker: User,
    state: SlashState,
    interaction_token: String,
) -> Result<InteractionResponse, Error> {
    match data.name.as_str() {
        "help" => Ok(crate::help::help(&invoker)),
        "rank" => {
            let target = crate::cmd_defs::RankCommand::from_interaction(data.into())?
                .user
                .map_or_else(|| invoker.clone(), |v| v.resolved);
            crate::levels::get_level(guild_id, target, invoker, state, interaction_token).await
        }
        "xp" => Ok(InteractionResponse {
            data: Some(
                crate::manager::process_xp(
                    crate::cmd_defs::XpCommand::from_interaction(data.into())?,
                    guild_id,
                    state,
                )
                .await?,
            ),
            kind: InteractionResponseType::ChannelMessageWithSource,
        }),
        "card" => Ok(crate::manage_card::card_update(
            crate::cmd_defs::CardCommand::from_interaction(data.into())?,
            invoker,
            &state,
            guild_id,
        )
        .await?),
        "leaderboard" => Ok(crate::levels::leaderboard(guild_id)),
        _ => Err(Error::UnrecognizedCommand),
    }
}

async fn process_user_cmd(
    data: CommandData,
    guild_id: Id<GuildMarker>,
    invoker: User,
    state: SlashState,
    interaction_token: String,
) -> Result<InteractionResponse, Error> {
    let msg_id = data.target_id.ok_or(Error::NoMessageTargetId)?;
    let user = data
        .resolved
        .as_ref()
        .ok_or(Error::NoResolvedData)?
        .users
        .get(&msg_id.cast())
        .ok_or(Error::NoTarget)?;
    crate::levels::get_level(guild_id, user.clone(), invoker, state, interaction_token).await
}

async fn process_msg_cmd(
    data: CommandData,
    guild_id: Id<GuildMarker>,
    invoker: User,
    state: SlashState,
    interaction_token: String,
) -> Result<InteractionResponse, Error> {
    let msg_id = data.target_id.ok_or(Error::NoMessageTargetId)?;
    let user = &data
        .resolved
        .as_ref()
        .ok_or(Error::NoResolvedData)?
        .messages
        .get(&msg_id.cast())
        .ok_or(Error::NoTarget)?
        .author;
    crate::levels::get_level(guild_id, user.clone(), invoker, state, interaction_token).await
}