use crate::types::{PlayerStats, ResourceAmount, new_player_stats};
use crate::{
    game_round::{GameRound, RoundState},
    types::ReputationAmount,
    utils::convert_keys_from_b64,
};
use hdk::prelude::*;
use holo_hash::*;
use std::{collections::HashMap, time::SystemTime};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    InProgress,
    Lost { last_round: EntryHash },
    // TODO: when validating things, check that last game round is finished to verify
    // that session itself is finished
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize, Copy, PartialEq, Eq)]
pub struct GameParams {
    pub regeneration_factor: u32,
    pub start_amount: ResourceAmount,
    pub num_rounds: u32,
    pub resource_coef: u32,
    pub reputation_coef: u32,
}

#[hdk_entry(id = "game_session", visibility = "public")]
#[derive(Clone, PartialEq, Eq)]
pub struct GameSession {
    pub owner: AgentPubKeyB64, // who started the game
    // pub created_at: Timestamp,     // when the game was started
//    pub status: SessionState,         // how the game is going   ---> TIXEL: why make it stateful, creates the need for updates, 
                                      // increases the eventual consistency challenges
    pub game_params: GameParams,      // what specific game are we playing
    pub players: Vec<AgentPubKeyB64>, // who is playing
}

#[hdk_entry(id = "game_scores", visibility = "public")]
#[derive(Clone, PartialEq, Eq)]
pub struct GameScores {
    // pub ended_at: Timestamp,     // when the game was started
    pub session: EntryHashB64,      // which game
    pub stats: PlayerStats,         // who is playing
}

#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes)]
pub struct GameSessionInput {
    pub game_params: GameParams,
    pub players: Vec<AgentPubKeyB64>,
}

#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub struct SignalPayloadNextRound {
    pub game_session: GameSession,
    pub game_session_header_hash: HeaderHashB64,
    pub current_round: GameRound,
    pub current_round_header_hash: HeaderHashB64,
    pub next_round_header_hash: HeaderHashB64,
}

#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub struct SignalPayloadStartGame {
    pub game_session: GameSession,
    pub game_session_header_hash: HeaderHashB64,
    pub current_round: GameRound,
    pub current_round_header_hash: HeaderHashB64,
}

#[derive(Debug, Serialize, Deserialize, SerializedBytes)]
pub struct SignalPayloadGameOver {
    pub game_scores: GameScores,
}

/*
validation rules:

- make sure session is created only when invites are answered and there's at least one accepted
    - TODO: add addresses of accepted invites into game session, later

*/

impl GameSession {
    // called in different contexts:
    // if validation: if round isn't available, validation sin't finished
    // if session state update: round is available
    pub fn update_state(&self, _game_round: GameRound) {
        // this is called every time after GameRound is created

        // if round is lost <= 0:
        //  game session is lost
        // elif number round == num_rounds:
        //  game session is finished
        // else:
        //  game session is in progress
    }
}

// placeholder function that can be called from UI/test, until invitation zoom is added.
pub fn start_dummy_session(player_list: Vec<AgentPubKeyB64>) -> ExternResult<HeaderHash> {
    let input = GameSessionInput {
        game_params: GameParams {
            regeneration_factor: 1,
            start_amount: 100,
            num_rounds: 3,
            resource_coef: 3,
            reputation_coef: 2,
        },
        players: player_list,
    };
    new_session(input)
}

/// Create a new GameSession with the confirmed players (who accepted their invites).
/// NOTE: we're only creating session for those who accepted and only if there are at
/// least two of them -- otherwise there won't be any turns.
pub fn new_session(input: GameSessionInput) -> ExternResult<HeaderHash> {
    // agent that starts new game
    let agent_info: AgentInfo = agent_info()?;

    // TODO: get timestamp as systime
    
    // create entry for game session
    let gs = GameSession {
        owner: AgentPubKeyB64::from(agent_info.agent_initial_pubkey),
        game_params: input.game_params,  // rules of the game
        players: input.players.clone(),
    };
    let game_session_header_hash = create_entry(&gs)?;
    let game_session_entry_hash = hash_entry(&gs)?;

    // make link from every players agent address to game session entry
    // tixel: this is not needed I think, implicit links are in game session
    // might only be needed if remote_signal for some reason would proof to be unreliable
    // e-nastasia: I think we'll need it to implement "list all games I've created"
    // functionality for any user.
    // create_link(agent_info.agent_initial_pubkey.clone().into(),
    // entry_hash_game_session.clone(), LinkTag::new("game_sessions"))?;

    // create game round results for round 0
    // this is starting point for all the game moves of round 1 to reference (implicit link)
    let no_moves: Vec<EntryHash> = vec![];

    // TODO: create a link from session to game round entry to make the round discoverable
    let round_one = GameRound {
        round_state: RoundState::InProgress,
        round_num: 1,
        session: game_session_header_hash,
        player_stats: new_player_stats(input.players.clone()),
        player_moves: no_moves,
        resources_left: gs.game_params.start_amount,
    };

    let round_one_header_hash = create_entry(&round_one)?;

    // use remote signals from RSM to send a real-time notif to invited players
    //  ! using remote signal to ping other holochain backends, instead of emit_signal
    //  that would talk with the UI
    // NOTE: we're sending signals to notify that a new round has started and
    // that players need to make their moves
    // WARNING: remote_signal is fire and forget, no error if it fails, might be a weak point if this were production happ
    let signal_payload = SignalPayloadStartGame {
        game_session: gs.clone(),
        game_session_header_hash: game_session_header_hash.into(),
        current_round: round_one,
        current_round_header_hash: round_one_header_hash.into(),
    };
    let signal = ExternIO::encode(GameSignal::StartGame(signal_payload))?;
    // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
    // the AgentPubKey type, we need to convert our keys to the expected data type
    remote_signal(signal, convert_keys_from_b64(input.players.clone()))?;
    tracing::debug!("sending signal to {:?}", input.players.clone());

    // // todo: get timestamp as systime
    // create_entry(&calendar_event)?;

    Ok(round_one_header_hash)
}

#[derive(Serialize, Deserialize, SerializedBytes, Debug)]
#[serde(tag = "signal_name", content = "signal_payload")]
pub enum GameSignal {
    StartGame(SignalPayloadStartGame),
    NextRound(SignalPayloadNextRound),
    GameOver(GameScores),
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use ::fixt::prelude::*;
//     use hdk::prelude::*;
//     use std::vec;

//     #[test]
//     fn test_new_session() {
//         let mut mock_hdk = hdk::prelude::MockHdkT::new();
//         let game_params = GameParams {
//             regeneration_factor: 1.1,
//             start_amount: 100,
//             num_rounds: 3,
//             resource_coef: 3,
//             reputation_coef: 2,
//         };

//         // mock agent info
//         let agent_pubkey = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let agent_info = AgentInfo::new(agent_pubkey.clone().into(), agent_pubkey.clone().into());
//         let agent2_pubkey = AgentPubKeyB64::from(fixt!(AgentPubKey));

//         mock_hdk
//             .expect_agent_info()
//             .times(1)
//             .return_once(move |_| Ok(agent_info));

//         // mock create entry
//         let headerhash = fixt!(HeaderHash);

//         let entryhash = fixt!(EntryHash);
//         let closure_header_hash = headerhash.clone();
//         mock_hdk
//             .expect_create()
//             .with(hdk::prelude::mockall::predicate::eq(
//                 EntryWithDefId::try_from(GameSession {
//                     owner: agent_pubkey.clone().into(),
//                     status: SessionState::InProgress,
//                     game_params: game_params.clone(),
//                     players: vec![agent_pubkey.clone().into(), agent2_pubkey],
//                 })
//                 .unwrap(),
//             ))
//             .times(1)
//             .return_once(move |_| Ok(closure_header_hash));

//         let input = GameSessionInput {
//             game_params: game_params,
//             players: vec![AgentPubKeyB64::from(fixt!(AgentPubKey)), AgentPubKeyB64::from(fixt!(AgentPubKey)), AgentPubKeyB64::from(fixt!(AgentPubKey))], // 3 random players
//         };

//         let entry_hash_game_session = fixt!(EntryHash);
//         mock_hdk
//             .expect_hash_entry()
//             .times(1)
//             .return_once(move |_| Ok(entry_hash_game_session));

//         mock_hdk
//             .expect_remote_signal()
//             .times(1)
//             .return_once(move |_| Ok(()));

//         let header_hash_link = fixt!(HeaderHash);
//         mock_hdk
//             .expect_create_link()
//             .times(1)
//             .return_once(move |_| Ok(header_hash_link));

//         hdk::prelude::set_hdk(mock_hdk);
//         new_session(input);
//     }
// }
