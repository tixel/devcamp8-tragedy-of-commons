use crate::game_move::GameMove;
use crate::game_session::{GameParams, GameSession, GameSignal, SessionState, SignalPayload};
use crate::types::{PlayerStats, ReputationAmount, ResourceAmount};
use crate::utils::{convert_keys_from_b64, try_get_and_convert, try_get_game_moves};
use hdk::prelude::*;
use holo_hash::*;
use holochain_types::signal::Signal;
use std::collections::HashMap;
use std::any::type_name;

const NO_REPUTATION: ReputationAmount = 0;

// todo: rename it so we don't have name clash with SessionState
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, SerializedBytes)]
pub struct RoundState {
    pub resource_amount: ResourceAmount,
    pub player_stats: PlayerStats,
}

#[hdk_entry(id = "game_round", visibility = "public")]
#[derive(PartialEq, Eq)]
pub struct GameRound {
    pub round_num: u32,
    pub session: EntryHash,
    pub round_state: RoundState,
    pub previous_round_moves: Vec<EntryHash>,
}

impl GameRound {}

impl Clone for GameRound {
    fn clone(&self) -> Self {
        GameRound {
            round_num: self.round_num,
            session: self.session.clone(),
            round_state: self.round_state.clone(),
            previous_round_moves: vec![], //TODO clone moves
        }
    }

    fn clone_from(&mut self, source: &Self) {
        *self = source.clone()
    }
}

impl RoundState {
    /// Creates a new RoundState instance with the provided input
    pub fn new(resource_amount: ResourceAmount, player_stats: PlayerStats) -> RoundState {
        RoundState {
            resource_amount,
            player_stats,
        }
    }
}

impl GameRound {
    /// Creates a new GameRound instance with the provided input
    pub fn new(
        round_num: u32,
        session: EntryHash,
        round_state: RoundState,
        previous_round_moves: Vec<EntryHash>,
    ) -> GameRound {
        GameRound {
            round_num,
            session,
            round_state,
            previous_round_moves,
        }
    }
}


#[hdk_entry(id = "game_scores", visibility = "public")]
#[derive(Clone, PartialEq, Eq)]
pub struct GameScores {
    pub game_session: GameSession,
    pub game_session_entry_hash: EntryHashB64,
    pub player_stats:PlayerStats,
    //TODO add the actual results :-)
}


/*
validation rules:

- In any game session there's always only one round with the respective round_num
- len of rounds update chain is always <= game_session.params.num_rounds + 1

- validation calculus: validate one round at a time and assume params of previous round
    are already valid
-

TODO: impl validation as:
validate_update_entry_game_round_results -> EntryID


*/

// NOTE: this fn would be used both in validation and when creating game round entries
// so it has to be very lightweight and can not make any DHT queries
pub fn calculate_round_state(params: GameParams, player_moves: Vec<GameMove>) -> RoundState {
    // todo:
    // calculate round state from the player moves

    // resources
    let consumed_resources_in_round: i32 = player_moves.iter().map(|x| x.resources).sum();
    let total_leftover_resource = params.start_amount - consumed_resources_in_round;

    // player stats
    let mut stats: HashMap<AgentPubKeyB64, (ResourceAmount, ReputationAmount)> = HashMap::new();
    for p in player_moves.iter() {
        let a = p.owner.clone();
        let tuple: (ResourceAmount, ReputationAmount) = (p.resources, NO_REPUTATION);
        stats.insert(a, tuple);
    }

    RoundState {
        resource_amount: total_leftover_resource,
        player_stats: stats,
    }
}

// NOTE: game round is always created once players made their moves, so every round is always
// a retrospective of moves made, not created before and updated later
// NOTE: given the retrospective nature, maybe we should call this fn "close current round" or
// "start next round" to avoid adding more confusion
// fn new_game_round(input: GameRoundResultsInput) -> ExternResult<EntryHash> {
//     // validate that player_moves.len() == session.game_params.invited.len(),
//     // otherwise current round isn't complete and we can't create a new one

//     // let state = calculate_round_state
//     // if latest_round not None:
//     //  update existing round entry on the latest_round hash (continuing the update chain)
//     // else:
//     //  create new round entry
//     //  make a link from session -> round
//     // if round is finished or lost:
//     //  update game session state

//     unimplemented!()
// }

// Should retrieve all game moves corresponding to the current round entry (in case of round 0 this
// would actually be a game session entry) and attempt to close the current round by creating it's entry.
// This would solely depend on the amount of moves retrieved being equal to the amount of players in the game
#[hdk_extern]
pub fn try_to_close_round(prev_round_hash: EntryHashB64) -> ExternResult<HeaderHashB64> {
    println!("try to close round");
    let prev_round: GameRound = get_game_round(prev_round_hash.clone());
    let game_session: GameSession = get_game_session(prev_round.session.clone().into());
    let moves = get_game_moves(prev_round_hash.clone());
    println!("all data fetched");
    println!("moves list #{:?}", moves);
    let moves_len = moves.len();
    if moves_len < game_session.players.len() {
        println!("some moves found: #{:?}", moves_len);
        println!("not closing round");
        return Err(WasmError::Host(format!("Still waiting on players")));
    };
    println!("all players made their moves");
    let round_state = calculate_round_state(game_session.game_params, moves);
    let result = create_next_round_or_end_game(game_session, prev_round, round_state);
    match result {
        Ok(hash) => ExternResult::Ok(HeaderHashB64::from(hash)),
        Err(why) => ExternResult::Err(why),
    }
}

fn create_next_round_or_end_game(
    game_session: GameSession,
    prev_round: GameRound,
    round_state: RoundState,
) -> ExternResult<HeaderHash> {
    if game_session.game_params.num_rounds < prev_round.round_num
    {
        // emit signal -
        println!("ending game");
        end_game_finished(game_session.clone(), round_state)
    } else if round_state.resource_amount < 0 {
        println!("game lost");
        end_game_lost(game_session.clone(), round_state)
    }else {
        println!("creating new round");
        create_new_round(prev_round.round_num, game_session.clone(), round_state)
    }
}

fn extract_moves(links: Links, game_session: &GameSession) -> Vec<GameMove> {
    let links_vec = links.into_inner();
    println!("number of moves: #{:?}", links_vec.len());
    println!("number of players: #{:?}", game_session.players.len());
    if (links_vec.len() < game_session.players.len()) {
        let missing_moves_count = game_session.players.len() - links_vec.len();
        return vec![];
    }
    let mut moves: Vec<GameMove> = vec![];
    for l in links_vec {
        println!("getting move for link: #{:?}", l);
        let result = get_game_move(l.target);
        println!("move: #{:?}", result);
        moves.push(result);
    }
    moves
}

fn get_game_round(entry_hash: EntryHashB64) -> GameRound {
    try_get_and_convert(entry_hash.into()).unwrap()
}

fn get_game_session(entry_hash: EntryHashB64) -> GameSession {
    try_get_and_convert(entry_hash.into()).unwrap()
}

fn get_game_moves(entry_hash: EntryHashB64) -> Vec<GameMove> {
    try_get_game_moves(entry_hash.into())
}

fn get_game_move(entry_hash: EntryHash) -> GameMove {
    try_get_and_convert(entry_hash.into()).unwrap()
}

fn create_new_round(
    prev_round_num: u32,
    session: GameSession,
    round_state: RoundState,
) -> ExternResult<HeaderHash> {
    let session_hash = hash_entry(&session)?;
    // TODO: instead of creating a new entry, we should continue the update chain
    // from the previous round entry hash and commit an updated version
    let round = GameRound {
        round_num: prev_round_num + 1,
        round_state: round_state,
        session: session_hash.clone(),
        previous_round_moves: vec![],
    };
    let header_hash_round = create_entry(&round)?;
    let entry_hash_round = hash_entry(&round)?;
    let signal_payload = SignalPayload {
        // tixel: not sure if we need the full objects or only the hashes or both. The tests will tell...
        game_session: session.clone(),
        game_session_entry_hash: session_hash.clone(),
        previous_round: round,
        previous_round_entry_hash: entry_hash_round.clone(),
    };
    let signal = ExternIO::encode(GameSignal::StartNextRound(signal_payload))?;
    // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
    // the AgentPubKey type, we need to convert our keys to the expected data type
    remote_signal(signal, convert_keys_from_b64(session.players.clone()))?;
    println!("sending signal to {:?}", session.players.clone());

    Ok(header_hash_round)
}

fn end_game_finished(session: GameSession, round_state: RoundState) -> ExternResult<HeaderHash> {
    println!("calculating scores");
    let session_hash = hash_entry(&session)?;
    // TODO: update GameSession entry to set it's state to closed
    session.status = SessionState::Finished;
    update_entry(session);
    SignalPayload{
        game_session: session,
        game_session_entry_hash: session_hash,
        previous_round: 
    }

    let signal = ExternIO::encode(GameSignal::GameOver(scores))?;
    // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
    // the AgentPubKey type, we need to convert our keys to the expected data type
    remote_signal(signal, convert_keys_from_b64(session.players.clone()))?;
    println!("sending signal to {:?}", session.players.clone());

    Ok(scores_header_hash)
}

fn end_game_lost(session: GameSession, round_state: RoundState) -> ExternResult<HeaderHash> {
    println!("calculating scores");
    let session_hash = hash_entry(&session)?;
    // TODO: update GameSession entry to set it's state to closed
    session.status = SessionState::Lost
    let signal = ExternIO::encode(GameSignal::GameOver(scores))?;
    // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
    // the AgentPubKey type, we need to convert our keys to the expected data type
    remote_signal(signal, convert_keys_from_b64(session.players.clone()))?;
    println!("sending signal to {:?}", session.players.clone());

    Ok(scores_header_hash)
}

fn type_of<T>(_: T) -> &'static str {
    type_name::<T>()
}

// Retrieves all available game moves made in a certain round, where entry_hash identifies
// base for the links.
fn get_all_round_moves(round_entry_hash: EntryHash) {
    unimplemented!();
}

#[cfg(test)]
#[rustfmt::skip]   // skipping formatting is needed, because to correctly import fixt we needed "use ::fixt::prelude::*;" which rustfmt does not like
mod tests {
    use super::*;
    use crate::game_session::{GameSession, GameSignal, SessionState, SignalPayload};
    use crate::types::ResourceAmount;
    use crate::{
        game_round::{calculate_round_state, GameRound, RoundState},
        game_session::GameParams,
        persistence,
    };
    use ::fixt::prelude::*;
    use hdk::prelude::*;
    use holochain_types::prelude::{EntryHashB64, HeaderHashB64};
    use holochain_types::{prelude::HoloHashed, TimestampKey};
    use holochain_zome_types::element::Element;
    use mockall::predicate::*;
    use mockall::*;
    use mockall_double::*;
    use std::time::SystemTime;
    use std::{collections::HashMap, vec};
    use super::*;
    use ::fixt::prelude::*;
    use mockall::mock;

    use holochain_types::prelude::ElementFixturator;

    #[test]
    // to run just this test =>   RUSTFLAGS='-A warnings' cargo test --features "mock" --package tragedy_of_commons --lib -- game_round::tests::test_try_to_close_round_fails_not_enough_moves --exact --nocapture
    fn test_try_to_close_round_fails_not_enough_moves() {
        println!("closing round should fail because only one of two players has made a move.");
        // mock agent info
        let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

        let mut mock_hdk = hdk::prelude::MockHdkT::new();
        let game_params = GameParams {
            regeneration_factor: 1,
            start_amount: 100,
            num_rounds: 3,
            resource_coef: 3,
            reputation_coef: 2,
        };
        let game_round_zero = GameRound {
            round_num: 0,
            session: session_entry_hash.into(),
            round_state: RoundState {
                resource_amount: 100,
                player_stats: HashMap::new(),
            },
            previous_round_moves: vec![],
        };
        let game_session = GameSession {
            owner: agent_pubkey_alice.clone(),
            status: SessionState::InProgress,
            game_params,
            players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
        };
        let mut element_with_game_round: Element = fixt!(Element);
        *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round_zero.clone().try_into().unwrap());
        let mut element_with_game_session: Element = fixt!(Element);
        *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_round)));

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_session)));


        let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
        let link_to_move_alice_round1 = Link {
            target: move_alice_round1_entry_hash.into(),
            timestamp: Timestamp::from(chrono::offset::Utc::now()),
            tag: LinkTag::new("game_move"),
            create_link_hash: move_alice_round1_link_header_hash.into(),
        };
        let game_moves: Links = vec![link_to_move_alice_round1].into();

        mock_hdk
            .expect_get_links()
            .times(1)
            .return_once(move |_| Ok(game_moves));

        let game_move_alice = GameMove {
            owner: agent_pubkey_alice.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 10,
        };
        let mut element_with_game_move_alice = fixt!(Element);
        *element_with_game_move_alice.as_entry_mut() =
            ElementEntry::Present(game_move_alice.try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_move_alice)));
            let header_hash_final_round = fixt!(HeaderHash);

        hdk::prelude::set_hdk(mock_hdk);
        let result = try_to_close_round(prev_round_entry_hash.clone());
        let err = result.err().unwrap();
        match err {
            WasmError::Host(x) => assert_eq!(x, "Still waiting on players"),
            _ => assert_eq!(true, false),
        }
    }

    #[test]
    fn test_try_to_close_round_success_create_next_round() {
        println!("start test");
        // mock agent info
        let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

        let mut mock_hdk = hdk::prelude::MockHdkT::new();
        let game_params = GameParams {
            regeneration_factor: 1,
            start_amount: 100,
            num_rounds: 3,
            resource_coef: 3,
            reputation_coef: 2,
        };
        let game_round = GameRound {
            round_num: 0,
            session: session_entry_hash.into(),
            round_state: RoundState {
                resource_amount: 100,
                player_stats: HashMap::new(),
            },
            previous_round_moves: vec![],
        };

        let game_session = GameSession {
            owner: agent_pubkey_alice.clone(),
            status: SessionState::InProgress,
            game_params,
            players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
        };

        let mut element_with_game_round: Element = fixt!(Element);
        *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round.clone().try_into().unwrap());

        let mut element_with_game_session: Element = fixt!(Element);
        *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_round)));

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_session)));


        let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let move_bob_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
        let move_bob_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
        let link_to_move_alice_round1 = Link {
            target: move_alice_round1_entry_hash.into(),
            timestamp: Timestamp::from(chrono::offset::Utc::now()),
            tag: LinkTag::new("game_move"),
            create_link_hash: move_alice_round1_link_header_hash.into(),
        };
        let link_to_move_bob_round1 = Link {
            target: move_bob_round1_entry_hash.into(),
            timestamp: Timestamp::from(chrono::offset::Utc::now()),
            tag: LinkTag::new("game_move"),
            create_link_hash: move_bob_round1_link_header_hash.into(),
        };
        let game_moves: Links = vec![link_to_move_alice_round1, link_to_move_bob_round1].into();

        mock_hdk
            .expect_get_links()
            .times(1)
            .return_once(move |_| Ok(game_moves));

        let game_move_alice = GameMove {
            owner: agent_pubkey_alice.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 10,
        };
        let game_move_bob = GameMove {
            owner: agent_pubkey_bob.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 10,
        };

        let mut element_with_game_move_alice = fixt!(Element);
        *element_with_game_move_alice.as_entry_mut() =
            ElementEntry::Present(game_move_alice.try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_move_alice)));

        let mut element_with_game_move_bob = fixt!(Element);
        *element_with_game_move_bob.as_entry_mut() =
            ElementEntry::Present(game_move_bob.try_into().unwrap());
        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_move_bob)));

        let header_hash_next_round = fixt!(HeaderHash);
        let header_hash_next_round_closure = header_hash_next_round.clone();
        mock_hdk
            .expect_create()
            .times(1)
            .return_once(move |_| Ok(header_hash_next_round_closure));

        let entry_hash_game_session = fixt!(EntryHash);
        mock_hdk
            .expect_hash_entry()
            .times(1)
            .return_once(move |_| Ok(entry_hash_game_session));
        let entry_hash_scores = fixt!(EntryHash);
        mock_hdk
            .expect_hash_entry()
            .times(1)
            .return_once(move |_| Ok(entry_hash_scores));

        mock_hdk
            .expect_remote_signal()
            .times(1)
            .return_once(move |_| Ok(()));

        hdk::prelude::set_hdk(mock_hdk);
        let result = try_to_close_round(prev_round_entry_hash.clone());
        assert_eq!(result.unwrap(), HeaderHashB64::from(header_hash_next_round.clone()));
    }

    #[test]
    // #[ignore = "WIP should send scores "]
    fn test_try_to_close_round_success_end_game_resources_depleted(){
        println!("start test");
        let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

        let mut mock_hdk = hdk::prelude::MockHdkT::new();
        let game_params = GameParams {
            regeneration_factor: 1,
            start_amount: 100,
            num_rounds: 1,
            resource_coef: 3,
            reputation_coef: 2,
        };
        let game_round = GameRound {
            round_num: 0,
            session: session_entry_hash.into(),
            round_state: RoundState {
                resource_amount: 100,
                player_stats: HashMap::new(),
            },
            previous_round_moves: vec![],
        };

        let game_session = GameSession {
            owner: agent_pubkey_alice.clone(),
            status: SessionState::InProgress,
            game_params,
            players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
        };

        let mut element_with_game_round: Element = fixt!(Element);
        *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round.clone().try_into().unwrap());

        let mut element_with_game_session: Element = fixt!(Element);
        *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_round)));

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_session)));


        let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let move_bob_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
        let move_bob_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
        let link_to_move_alice_round1 = Link {
            target: move_alice_round1_entry_hash.into(),
            timestamp: Timestamp::from(chrono::offset::Utc::now()),
            tag: LinkTag::new("game_move"),
            create_link_hash: move_alice_round1_link_header_hash.into(),
        };
        let link_to_move_bob_round1 = Link {
            target: move_bob_round1_entry_hash.into(),
            timestamp: Timestamp::from(chrono::offset::Utc::now()),
            tag: LinkTag::new("game_move"),
            create_link_hash: move_bob_round1_link_header_hash.into(),
        };
        let game_moves: Links = vec![link_to_move_alice_round1, link_to_move_bob_round1].into();

        mock_hdk
            .expect_get_links()
            .times(1)
            .return_once(move |_| Ok(game_moves));

        let game_move_alice = GameMove {
            owner: agent_pubkey_alice.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 10,
        };
        let game_move_bob = GameMove {
            owner: agent_pubkey_bob.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 100, // bob takes all the resources at once
        };

        let mut element_with_game_move_alice = fixt!(Element);
        *element_with_game_move_alice.as_entry_mut() =
            ElementEntry::Present(game_move_alice.try_into().unwrap());

        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_move_alice)));

        let mut element_with_game_move_bob = fixt!(Element);
        *element_with_game_move_bob.as_entry_mut() =
            ElementEntry::Present(game_move_bob.try_into().unwrap());
        mock_hdk
            .expect_get()
            .times(1)
            .return_once(move |_| Ok(Some(element_with_game_move_bob)));

        // let header_hash_final_round = fixt!(HeaderHash);
        // let header_hash_final_round_closure = header_hash_final_round.clone();
        let entry_hash_scores = fixt!(EntryHash);
        let game_scores = GameScores{
            game_session: game_session.clone(),
            game_session_entry_hash: EntryHashB64::from(entry_hash_scores),
        };
        // mock_hdk
        //     .expect_create()
        //     // .with(mockall::predicate::eq(
        //     //     EntryWithDefId::try_from(game_scores).unwrap()
        //     // ))
        //     .times(1)
        //     .return_once(move |_| Ok(header_hash_final_round_closure));


        let entry_hash_game_session = fixt!(EntryHash);
        mock_hdk
            .expect_hash_entry()
            .times(1)
            .return_once(move |_| Ok(entry_hash_game_session));
        
        let entry_hash_scores = fixt!(EntryHash);
        let header_hash_scores = fixt!(HeaderHash);
        let header_hash_scores_closure = header_hash_scores.clone();
        mock_hdk
            .expect_hash_entry()
            .times(1)
            .return_once(move |_| Ok(entry_hash_scores));
        mock_hdk
            .expect_create()
            // .with(mockall::predicate::eq(EntryWithDefId::try_from(&game_scores).unwrap()))
            .times(1)
            .return_once(move |_| Ok(header_hash_scores_closure));
        mock_hdk
            .expect_remote_signal()
            .times(1)
            .return_once(move |_| Ok(()));

        hdk::prelude::set_hdk(mock_hdk);
        let result = try_to_close_round(prev_round_entry_hash.clone());
        assert_eq!(result.unwrap(), HeaderHashB64::from(header_hash_scores.clone()));
    }

    #[test]
    #[ignore = "not implemented"]
    fn test_try_to_close_round_end_game_all_rounds_played(){

    }

    #[test]
    #[ignore = "refactoring"]
    fn test_calculate_round_state() {
        let gp = GameParams {
            regeneration_factor: 1,
            start_amount: 100,
            num_rounds: 3,
            resource_coef: 3,
            reputation_coef: 2,
        };

        let p1_key = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let move1 = GameMove {
            owner: p1_key.clone().into(),
            previous_round: EntryHashB64::from(fixt!(EntryHash)),
            resources: 5,
        };

        let p2_key = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let move2 = GameMove {
            owner: p2_key.clone(),
            previous_round: EntryHashB64::from(fixt!(EntryHash)),
            resources: 10,
        };
        let s = calculate_round_state(gp.clone(), vec![move1, move2]);
        assert_eq!(gp.clone().start_amount - 15, s.resource_amount);

        let stats_p1: (ResourceAmount, ReputationAmount) =
            *s.player_stats.get(&p1_key.clone().into()).unwrap();
        assert_eq!(stats_p1.0, 5);
        assert_eq!(stats_p1.1, 0);

        let stats_p2: (ResourceAmount, ReputationAmount) =
            *s.player_stats.get(&p2_key.clone().into()).unwrap();
        assert_eq!(stats_p2.0, 10);
        assert_eq!(stats_p1.1, 0);
    }
}
