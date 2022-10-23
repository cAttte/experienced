use crate::AppState;
use sqlx::query;
use twilight_model::{
    application::{
        command::CommandType,
        interaction::{
            application_command::{CommandData, CommandOptionValue},
            Interaction, InteractionData, InteractionType,
        },
    },
    channel::message::MessageFlags,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    user::User,
};
use twilight_util::builder::InteractionResponseDataBuilder;

pub async fn process(
    interaction: Interaction,
    state: AppState,
) -> Result<InteractionResponse, CommandProcessorError> {
    Ok(if interaction.kind == InteractionType::ApplicationCommand {
        InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(process_app_cmd(interaction, state).await?),
        }
    } else {
        InteractionResponse {
            kind: InteractionResponseType::Pong,
            data: None,
        }
    })
}

async fn process_app_cmd(
    interaction: Interaction,
    state: AppState,
) -> Result<InteractionResponseData, CommandProcessorError> {
    #[cfg(debug_assertions)]
    println!("DEBUG: {:#?}", interaction);
    let data = if let Some(data) = interaction.data {
        if let InteractionData::ApplicationCommand(cmd) = data {
            *cmd
        } else {
            return Err(CommandProcessorError::WrongInteractionData);
        }
    } else {
        return Err(CommandProcessorError::NoInteractionData);
    };
    let invoker = match interaction.member {
        Some(val) => val.user,
        None => interaction.user,
    }
    .ok_or(CommandProcessorError::NoInvoker)?;
    let target = match data.kind {
        CommandType::ChatInput => process_slash_cmd(&data, &invoker),
        CommandType::User => process_user_cmd(&data),
        CommandType::Message => process_msg_cmd(&data),
        _ => return Err(CommandProcessorError::WrongInteractionData),
    }?;
    get_level(target, &invoker, state).await
}

fn process_slash_cmd<'a>(
    data: &'a CommandData,
    invoker: &'a User,
) -> Result<&'a User, CommandProcessorError> {
    if &data.name != "level" && &data.name != "rank" {
        return Err(CommandProcessorError::UnrecognizedCommand);
    };
    for option in &data.options {
        if option.name == "user" {
            if let CommandOptionValue::User(user_id) = option.value {
                return data
                    .resolved
                    .as_ref()
                    .ok_or(CommandProcessorError::NoResolvedData)?
                    .users
                    .get(&user_id)
                    .ok_or(CommandProcessorError::NoTarget);
            };
        }
    }
    Ok(invoker)
}

fn process_user_cmd(data: &CommandData) -> Result<&User, CommandProcessorError> {
    let msg_id = data
        .target_id
        .ok_or(CommandProcessorError::NoMessageTargetId)?;
    data.resolved
        .as_ref()
        .ok_or(CommandProcessorError::NoResolvedData)?
        .users
        .get(&msg_id.cast())
        .ok_or(CommandProcessorError::NoTarget)
}

fn process_msg_cmd(data: &CommandData) -> Result<&User, CommandProcessorError> {
    let msg_id = data
        .target_id
        .ok_or(CommandProcessorError::NoMessageTargetId)?;
    Ok(&data
        .resolved
        .as_ref()
        .ok_or(CommandProcessorError::NoResolvedData)?
        .messages
        .get(&msg_id.cast())
        .ok_or(CommandProcessorError::NoTarget)?
        .author)
}

async fn get_level(
    user: &User,
    invoker: &User,
    state: AppState,
) -> Result<InteractionResponseData, CommandProcessorError> {
    // Select current XP from the database, return 0 if there is no row
    let xp = match query!("SELECT xp FROM levels WHERE id = ?", user.id.to_string())
        .fetch_one(&state.db)
        .await
    {
        Ok(val) => val.xp,
        Err(e) => match e {
            sqlx::Error::RowNotFound => 0,
            _ => Err(e)?,
        },
    };
    let rank = query!("SELECT COUNT(*) as count FROM levels WHERE xp > ?", xp)
        .fetch_one(&state.db)
        .await?
        .count
        + 1;
    let level_info = libmee6::LevelInfo::new(xp);
    let content = if user.bot {
        "Bots aren't ranked, that would be silly!".to_string()
    } else if invoker == user {
        if xp == 0 {
            "You aren't ranked yet, because you haven't sent any messages!".to_string()
        } else {
            format!(
                "You are level {} (rank #{}), and are {}% of the way to level {}.",
                level_info.level(),
                rank,
                level_info.percentage(),
                level_info.level() + 1
            )
        }
    } else if xp == 0 {
        format!(
            "{}#{} isn't ranked yet, because they haven't sent any messages!",
            user.name, user.discriminator
        )
    } else {
        format!(
            "{}#{} is level {} (rank #{}), and is {}% of the way to level {}.",
            user.name,
            user.discriminator(),
            level_info.level(),
            rank,
            level_info.percentage(),
            level_info.level() + 1
        )
    };
    Ok(InteractionResponseDataBuilder::new()
        .flags(MessageFlags::EPHEMERAL)
        .content(content)
        .build())
}

#[derive(Debug, thiserror::Error)]
pub enum CommandProcessorError {
    #[error("Discord sent a command that is not known!")]
    UnrecognizedCommand,
    #[error("Discord did not send a user object for the command invoker when it was required!")]
    NoInvoker,
    #[error("Discord did not send a user object for the command target when it was required!")]
    NoTarget,
    #[error("Discord did not send part of the Resolved Data!")]
    NoResolvedData,
    #[error("Discord did not send target ID for message!")]
    NoMessageTargetId,
    #[error("Discord sent interactiond data for an unsupported interaction type!")]
    WrongInteractionData,
    #[error("Discord did not send any interaction data!")]
    NoInteractionData,
    #[error("SQLx encountered an error: {0}")]
    Sqlx(#[from] sqlx::Error),
}