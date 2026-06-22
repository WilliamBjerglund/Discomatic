/*
Dice roller that understands notation like d20, 2d6, or 2d20+4, defaulting to a single d6.
*/

use rand::RngExt;

use crate::{Context, Error};

struct DiceRoll {
    number_of_dice: u32,
    sides_per_die: u32,
    modifier: i64,
}

fn parse_dice_notation(input: Option<&str>) -> Result<DiceRoll, String> {
    let notation = input
        .unwrap_or("d6")
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "");

    let (dice_part, modifier_text) =
        match notation.find(|character| character == '+' || character == '-') {
            Some(position) => notation.split_at(position),
            None => (notation.as_str(), ""),
        };

    let (count_text, sides_text) = dice_part
        .split_once('d')
        .ok_or("Use dice notation such as d20, 2d6, or 2d20+4.")?;

    let number_of_dice: u32 = if count_text.is_empty() {
        1
    } else {
        count_text
            .parse()
            .map_err(|_| "The number of dice is invalid.")?
    };

    let sides_per_die: u32 = sides_text
        .parse()
        .map_err(|_| "The number of sides is invalid.")?;

    let modifier: i64 = if modifier_text.is_empty() {
        0
    } else {
        modifier_text
            .parse()
            .map_err(|_| "The modifier is not a valid number.")?
    };

    if !(1..=100).contains(&number_of_dice) {
        return Err("You may roll between 1 and 100 dice.".to_string());
    }

    if !(2..=1_000_000).contains(&sides_per_die) {
        return Err("Each die must have between 2 and 1,000,000 sides.".to_string());
    }

    if !(-1_000_000..=1_000_000).contains(&modifier) {
        return Err("The modifier is too large.".to_string());
    }

    Ok(DiceRoll {
        number_of_dice,
        sides_per_die,
        modifier,
    })
}

// Rolls dice using notation such as d20, 2d6, or 2d20+4.
#[poise::command(slash_command)]
pub async fn roll(
    ctx: Context<'_>,

    #[description = "Dice notation, such as d20, 2d6, or 2d20+4"] dice: Option<String>,
) -> Result<(), Error> {
    let dice_roll = match parse_dice_notation(dice.as_deref()) {
        Ok(dice_roll) => dice_roll,

        Err(message) => {
            ctx.send(
                poise::CreateReply::default()
                    .content(message)
                    .ephemeral(true),
            )
            .await?;

            return Ok(());
        }
    };

    let rolls: Vec<u32> = {
        let mut rng = rand::rng();

        (0..dice_roll.number_of_dice)
            .map(|_| rng.random_range(1..=dice_roll.sides_per_die))
            .collect()
    };

    let roll_total: i64 = rolls.iter().map(|roll| i64::from(*roll)).sum();
    let total = roll_total + dice_roll.modifier;

    let rolls_text = rolls
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    let modifier_text = if dice_roll.modifier == 0 {
        String::new()
    } else {
        format!(" {:+}", dice_roll.modifier)
    };

    ctx.say(format!(
        "**{}d{}{}**\nRolls: [{}]\nTotal: **{}**",
        dice_roll.number_of_dice, dice_roll.sides_per_die, modifier_text, rolls_text, total,
    ))
    .await?;

    Ok(())
}
