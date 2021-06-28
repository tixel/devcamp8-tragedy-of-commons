use std::{collections::HashMap, vec};

use crate::prelude::SignedHeader;
use crate::{
    game_round::{self, calculate_round_state, GameRound, RoundState},
    game_session::{GameSession, GameSignal, SessionState},
    persistence::{self, Repository},
    types::ResourceAmount,
    utils::{convert_keys_from_b64, try_get_and_convert, try_get_game_moves},
};
use hdk::prelude::*;
use holo_hash::*;
use mockall::*;

#[hdk_entry(id = "game_move", visibility = "public")]
pub struct GameMove {
    pub owner: AgentPubKey,
    // For the very first round this option would be None, because we create game rounds
    // retrospectively. And since all players are notified by the signal when they can make
    // a move, maybe we could pass that value from there, so that every player has it
    // when they're making a move
    pub round: EntryHash,
    pub resources: ResourceAmount,
}
#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes)]
pub struct GameMoveInput {
    pub resource_amount: ResourceAmount,
    // NOTE: if we're linking all moves to the round, this can never be None
    // as we'll need a base for the link. Instead moves for the round 0 could be
    // linked directly from the game session.
    pub entry_hash_round: EntryHashB64,
}

/*
validation rules:
    - TODO: impl validation to make sure move is commited by player who's playing the game

for the context, here are notes on how we've made this decision:
- validate that one player only made one move for any round
    - right now we'll need to run get_links for that, can we avoid it?
    - alternative: get agent activity
        retrieves source chain headers from this agent
        get all headers that are get_link / new entry for game move
        validate that we're not repeating the same move

        validate that moves are made with timestamp >= game session
    - another alternative: avoid strict validation here, instead take first move
        made by agent for any round and use it when calculating
        - NOTE: we'll have vulnerability
        - NOTE: update round closing rules to check that every AGENT made a move
*/
#[hdk_extern]
pub fn new_move(input: GameMoveInput) -> ExternResult<HeaderHash> {

    let entry_hash_game_round:EntryHash = input.entry_hash_round.into();
    // todo: add guard clauses for empty input
    let game_move = GameMove {
        owner: agent_info()?.agent_initial_pubkey,
        resources: input.resource_amount,
        round: input.entry_hash_round.into(),
    };
    let header_hash_game_move = create_entry(&game_move)?;
    let entry_hash_game_move = hash_entry(&game_move)?;

    let game_move_link = create_link(
        entry_hash_game_round.clone(),
        entry_hash_game_move.clone(),
        LinkTag::new("game_move"),
    )?;
    // todo: (if we're making a link from round to move) make a link round -> move
    // note: instead of calling try_to_close_Round right here, we can have a UI make
    // this call for us. This way making a move wouldn't be blocked by the other moves'
    // retrieval process and the process of commiting the round entry.
    Ok(game_move_link.into())
}

// Question: how do we make moves discoverable by the players?
// Option1: make a link from game session / game round to which this move belongs?
//      note: this is where things start to get more complicated with the game round that is
//      only created retrospectively. We will have to manage this duality with link base being
//      either a game session or a game round. But maybe that's not a bad thing? That'll still
//      be a related Holochain entry after all.
