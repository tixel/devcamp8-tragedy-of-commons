// use game_session::GameSession;
use hdk::prelude::*;
#[allow(unused)]
use holo_hash::{AgentPubKeyB64, EntryHashB64};

#[allow(unused_imports)]
use crate::{
    game_move::GameMoveInput,
    game_session::{GameSessionInput, GameSignal, SignalPayload},
};
#[allow(unused_imports)]
#[allow(dead_code)]
#[allow(unused)]
mod game_move;
#[allow(unused_imports)]
#[allow(dead_code)]
#[allow(unused)]
mod game_round;
#[allow(unused_imports)]
#[allow(dead_code)]
#[allow(unused)]
mod game_session;
mod types;
mod utils;
mod persistence;

pub fn err(reason: &str) -> WasmError {
    WasmError::Guest(String::from(reason))
}

entry_defs![
    Path::entry_def(),
    game_session::GameSession::entry_def(),
    game_round::GameRound::entry_def(),
    game_move::GameMove::entry_def(),
    game_session::GameScores::entry_def()
];

// give unrestricted access to recv_remote_signal, which is needed for sending remote signals
#[hdk_extern]
fn init(_: ()) -> ExternResult<InitCallbackResult> {
    // grant unrestricted access to accept_cap_claim so other agents can send us claims
    let mut functions: GrantedFunctions = BTreeSet::new();
    functions.insert((zome_info()?.zome_name, "recv_remote_signal".into()));

    create_cap_grant(CapGrantEntry {
        tag: "".into(),
        access: ().into(), // empty access converts to unrestricted
        functions,
    })?;

    Ok(InitCallbackResult::Pass)
}

// function required to process remote signals see hdk/src/p2p.rs
#[hdk_extern]
fn recv_remote_signal(signal: ExternIO) -> ExternResult<()> {
    debug!("Received remote signal {:?}", signal);
    let game_signal_result: Result<GameSignal, SerializedBytesError> = signal.decode();
    //debug!("Received REMOTE signal {:?}", sig);
    match game_signal_result {
        Ok(a) => emit_signal(a),
        Err(_) => Err(WasmError::Guest("Remote signal failed".into())),
    }
}

/// Placeholder function that can be called from UI/test, until invitation zoom is added.
#[hdk_extern]
pub fn start_dummy_session(player_list: Vec<AgentPubKeyB64>) -> ExternResult<HeaderHash> {
    game_session::start_dummy_session(player_list)
}

/// Function to call when player wants to start a new game and has already selected
/// invitees for this game. This function is only supposed to handle invite zome integration
/// and it shouldn't be really creating a new GameSession entry.
// #[hdk_extern]
// pub fn propose_new_session() -> ExternResult<HeaderHash> {}

/// Function to call by the invite zome once all invites are taken care of
/// and we can actually create the GameSession and start playing
pub fn create_new_session(input: GameSessionInput) -> ExternResult<HeaderHash> {
    game_session::new_session(input)
}

// TODO: think of better naming to distinguish between sessions "as owner" and "as player"
/// Function to list all game sessions that the caller has created
/// In other words, all sessions that the caller owns
// #[hdk_extern]
// pub fn get_my_owned_sessions() -> ExternResult<Vec<EntryHashB64>> {}

/// Function to list all game sessions in which caller has been a player/owner
/// This list would include both owned game sessions and those to which caller has
/// been invited by other players
// pub fn get_all_my_sessions() -> ExternResult<Vec<EntryHashB64>> {}

/// Function to list all active sessions in which caller participates
// pub fn get_my_active_sessions() -> ExternResult<Vec<EntryHashB64>> {}

/// Function to make a new move in the game specified by input
pub fn make_new_move(input: GameMoveInput) -> ExternResult<HeaderHash> {
    game_move::new_move(input)
}

/// Function to call from the UI on a regular basis to try and close the currently
/// active GameRound. It will check the currently available GameRound state and then
/// will close it if it's possible. If not, it will return None
pub fn try_to_close_round(prev_round_hash: EntryHash) -> ExternResult<EntryHash> {
    // TODO: this should probably go to the game_round.rs instead
    game_move::try_to_close_round(prev_round_hash)
}

#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes)]
pub struct SignalTest {
    pub content: String,
    pub value: String,
}
