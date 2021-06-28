use crate::game_move::GameMove;
use crate::game_session::{GameParams, GameScores, GameSession, GameSignal, SessionState, SignalPayloadGameOver, SignalPayloadNextRound};
use crate::types::{PlayerStats, ReputationAmount, ResourceAmount};
use crate::utils::{convert_keys_from_b64, entry_hash_from_element, try_from_element, try_get_and_convert, try_get_by_header_and_convert, try_get_game_moves};
use hdk::prelude::*;
use holo_hash::*;
use std::collections::HashMap;
use std::any::type_name;

const NO_REPUTATION: ReputationAmount = 0;


#[hdk_entry(id = "game_round", visibility = "public")]
#[derive(PartialEq, Eq)]
pub struct GameRound {
    pub round_state:RoundState,
    pub round_num: u32,
    pub session: HeaderHash,
    pub resources_left: ResourceAmount,
    pub player_stats: PlayerStats,
    pub player_moves: Vec<EntryHash>,
}

impl GameRound {}

impl Clone for GameRound {
    fn clone(&self) -> Self {
        GameRound {
            round_state: self.round_state,
            round_num: self.round_num,
            session: self.session.clone(),
            resource_amount: self.resource_amount,
            player_stats: self.player_stats.clone(),
            player_moves: vec![], //TODO clone moves
        }
    }

    fn clone_from(&mut self, source: &Self) {
        *self = source.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, SerializedBytes)]
pub enum RoundState {
    InProgress,
    Finished,
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
pub fn calculate_round_state(params: GameParams, player_moves: Vec<GameMove>) -> (ResourceAmount, PlayerStats) {
    // todo:
    // calculate round state from the player moves

    // resources
    let consumed_resources_in_round: i32 = player_moves.iter().map(|x| x.resources).sum();
    let total_leftover_resource = params.start_amount - consumed_resources_in_round;

    // player stats
    let mut stats: HashMap<AgentPubKeyB64, (ResourceAmount, ReputationAmount)> = HashMap::new();
    for p in player_moves.iter() {
        let a = p.owner.into();
        let tuple: (ResourceAmount, ReputationAmount) = (p.resources, NO_REPUTATION);
        stats.insert(a, tuple);
    }

    (total_leftover_resource, stats)
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
pub fn try_to_close_round(current_round_header_hash: HeaderHash) -> ExternResult<HeaderHashB64> {
    println!("try to close round");

    println!("fetching data");
    //get_game_round(current_round_header_hash);
    let current_round_element = match get(current_round_header_hash.clone(), GetOptions::latest())? {
        Some(element) => element,
        None => return Err(WasmError::Guest("Current round not found".into())),
    };
    let current_round_entry_hash: &EntryHash = entry_hash_from_element(current_round_element)?;
    let current_round: GameRound = try_from_element(current_round_element)?;

    
    // get current game_session
    let game_session_element = match get(current_round.session.clone(), GetOptions::content())? {
        Some(element) => element,
        None => return Err(WasmError::Guest("Current round not found".into())),
    };
    let game_session: GameSession = get_game_session(current_round.session.into());
    let game_session_header_hash: &HeaderHash = current_round_element.header_address();
    let game_session_entry_hash = entry_hash_from_element(game_session_element)?;
    
    // get game moves
    let links = get_links(current_round_entry_hash.clone(), Some(LinkTag::new("game_move")))?;
    let mut moves: Vec<GameMove> = vec![];
    for link in links.into_inner() {
        let item: GameMove = match get(link.target.clone(), GetOptions::default())? {
            Some(x) => {
                let g:GameMove = try_from_element(x)?;
                g
            }
            None => return Err(WasmError::Guest("Cannot extract game move from link".into())),
        };
        moves.push(item);
    }
    println!("all data fetched");
    println!("****************");

    println!("check number of moves");
    println!("moves list #{:?}", moves);
    let moves_len = moves.len();
    if moves_len < game_session.players.len() {
        println!("number of moves found: #{:?}", moves_len);
        return Err(WasmError::Guest("Cannot close round: wait until all moves are made".into()));
    };
    // TODO add check is no player made 2 moves. all move need unique owners

    
    println!("****************");
    println!("all players made their moves: calculating round state");
    let (resources_left, stats) = calculate_round_state(game_session.game_params, moves);
    
    // complete round state and update round entry
    current_round.resources_left = resources_left;
    current_round.player_stats = stats;
    current_round.round_state = RoundState::Finished;
    let updated_current_round_header_hash = update_entry(current_round_header_hash, current_round)?;

    // decide what to do next
    // - continue game, start next round
    // - end game, because resources are depleted
    // - end game, because all rounds are played
    if (current_round.round_num < game_session.game_params.num_rounds || resources_left > 0){ // can start next round?
        println!("continue: creating next round");
        // TODO: instead of creating a new entry, we should continue the update chain
        // from the previous round entry hash and commit an updated version
        let next_round = GameRound {
            round_num: current_round.round_num + 1,
            round_state: RoundState::InProgress,
            session: *game_session_header_hash,
            resources_left: resources_left,
            player_stats: stats,
            player_moves: vec![],
        };
        let next_round_header_hash = create_entry(&next_round)?;
        let next_round_entry_hash = hash_entry(&next_round)?;

        let signal_payload = SignalPayloadNextRound {
            game_session: game_session.clone(),
            game_session_header_hash: HeaderHashB64::from(*game_session_header_hash), 
            current_round: current_round,
            current_round_header_hash: HeaderHashB64::from(updated_current_round_header_hash),
            next_round_header_hash: next_round_header_hash.into(),
        };
        let signal = ExternIO::encode(GameSignal::NextRound(signal_payload))?;
        // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
        // the AgentPubKey type, we need to convert our keys to the expected data type
        remote_signal(signal, convert_keys_from_b64(game_session.players.clone()))?;
        println!("sending signal to {:?}", game_session.players.clone());

        Ok(next_round_header_hash.into())

    } else {
        // distinction between game ended because all rounds completed of all resources depleted can be easily made in frontend
        // based on 
        // calculate and save gamescores
        let game_scores = GameScores{
            session: EntryHashB64::from(*game_session_entry_hash),
            stats: stats,
        };
        let game_scores_header_hash = create_entry(game_scores)?;
        let game_scores_entry_hash = hash_entry(&game_scores)?;
    
        // link scores to gamesession
        let game_scores_game_session_link = create_link(
            game_session_entry_hash.clone(),
            game_scores_entry_hash.clone(),
            LinkTag::new("game_scores"),
        )?;
        // prepare signal
        let signal_payload = SignalPayloadGameOver{
            game_scores: game_scores,
        };
        // send signal
        let signal = ExternIO::encode(GameSignal::GameOver(game_scores))?;
        remote_signal(signal, convert_keys_from_b64(game_session.players.clone()))?;
        println!("sending signal to {:?}", game_session.players.clone());
        // return hash of scores
        Ok(game_scores_header_hash.into())
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

fn get_game_round(header_hash: HeaderHash) -> GameRound {
    try_get_by_header_and_convert(header_hash).unwrap()
}

fn get_game_session(h: HeaderHash) -> GameSession {
    try_get_by_header_and_convert(h).unwrap()
}

fn get_game_moves(entry_hash: EntryHash) -> Vec<GameMove> {
    try_get_game_moves(entry_hash)
}

fn get_game_move(entry_hash: EntryHash) -> GameMove {
    try_get_and_convert(entry_hash.into()).unwrap()
}



fn type_of<T>(_: T) -> &'static str {
    type_name::<T>()
}

// Retrieves all available game moves made in a certain round, where entry_hash identifies
// base for the links.
fn get_all_round_moves(round_entry_hash: EntryHash) {
    unimplemented!();
}

// #[cfg(test)]
// #[rustfmt::skip]   // skipping formatting is needed, because to correctly import fixt we needed "use ::fixt::prelude::*;" which rustfmt does not like
// mod tests {
//     use super::*;
//     use crate::game_session::{GameSession, GameSignal, SessionState, SignalPayload};
//     use crate::types::ResourceAmount;
//     use crate::{
//         game_round::{calculate_round_state, GameRound, RoundState},
//         game_session::GameParams,
//         persistence,
//     };
//     use ::fixt::prelude::*;
//     use hdk::prelude::*;
//     use holochain_types::prelude::{EntryHashB64, HeaderHashB64};
//     use holochain_types::{prelude::HoloHashed, TimestampKey};
//     use holochain_zome_types::element::Element;
//     use mockall::predicate::*;
//     use mockall::*;
//     use mockall_double::*;
//     use std::time::SystemTime;
//     use std::{collections::HashMap, vec};
//     use super::*;
//     use ::fixt::prelude::*;
//     use mockall::mock;

//     use holochain_types::prelude::ElementFixturator;

//     #[test]
//     // to run just this test =>   RUSTFLAGS='-A warnings' cargo test --features "mock" --package tragedy_of_commons --lib -- game_round::tests::test_try_to_close_round_fails_not_enough_moves --exact --nocapture
//     fn test_try_to_close_round_fails_not_enough_moves() {
//         println!("closing round should fail because only one of two players has made a move.");
//         // mock agent info
//         let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

//         let mut mock_hdk = hdk::prelude::MockHdkT::new();
//         let game_params = GameParams {
//             regeneration_factor: 1,
//             start_amount: 100,
//             num_rounds: 3,
//             resource_coef: 3,
//             reputation_coef: 2,
//         };
//         let game_round_zero = GameRound {
//             round_num: 0,
//             session: session_entry_hash.into(),
//             round_state: RoundState {
//                 resource_amount: 100,
//                 player_stats: HashMap::new(),
//             },
//             previous_round_moves: vec![],
//         };
//         let game_session = GameSession {
//             owner: agent_pubkey_alice.clone(),
//             status: SessionState::InProgress,
//             game_params,
//             players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
//         };
//         let mut element_with_game_round: Element = fixt!(Element);
//         *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round_zero.clone().try_into().unwrap());
//         let mut element_with_game_session: Element = fixt!(Element);
//         *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_round)));

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_session)));


//         let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
//         let link_to_move_alice_round1 = Link {
//             target: move_alice_round1_entry_hash.into(),
//             timestamp: Timestamp::from(chrono::offset::Utc::now()),
//             tag: LinkTag::new("game_move"),
//             create_link_hash: move_alice_round1_link_header_hash.into(),
//         };
//         let game_moves: Links = vec![link_to_move_alice_round1].into();

//         mock_hdk
//             .expect_get_links()
//             .times(1)
//             .return_once(move |_| Ok(game_moves));

//         let game_move_alice = GameMove {
//             owner: agent_pubkey_alice.clone(),
//             previous_round: prev_round_entry_hash.clone().into(),
//             resources: 10,
//         };
//         let mut element_with_game_move_alice = fixt!(Element);
//         *element_with_game_move_alice.as_entry_mut() =
//             ElementEntry::Present(game_move_alice.try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_move_alice)));
//             let header_hash_final_round = fixt!(HeaderHash);

//         hdk::prelude::set_hdk(mock_hdk);
//         let result = try_to_close_round(prev_round_entry_hash.clone());
//         let err = result.err().unwrap();
//         match err {
//             WasmError::Host(x) => assert_eq!(x, "Still waiting on players"),
//             _ => assert_eq!(true, false),
//         }
//     }

//     #[test]
//     fn test_try_to_close_round_success_create_next_round() {
//         println!("start test");
//         // mock agent info
//         let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

//         let mut mock_hdk = hdk::prelude::MockHdkT::new();
//         let game_params = GameParams {
//             regeneration_factor: 1,
//             start_amount: 100,
//             num_rounds: 3,
//             resource_coef: 3,
//             reputation_coef: 2,
//         };
//         let game_round = GameRound {
//             round_num: 0,
//             session: session_entry_hash.into(),
//             round_state: RoundState {
//                 resource_amount: 100,
//                 player_stats: HashMap::new(),
//             },
//             previous_round_moves: vec![],
//         };

//         let game_session = GameSession {
//             owner: agent_pubkey_alice.clone(),
//             status: SessionState::InProgress,
//             game_params,
//             players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
//         };

//         let mut element_with_game_round: Element = fixt!(Element);
//         *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round.clone().try_into().unwrap());

//         let mut element_with_game_session: Element = fixt!(Element);
//         *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_round)));

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_session)));


//         let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let move_bob_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
//         let move_bob_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
//         let link_to_move_alice_round1 = Link {
//             target: move_alice_round1_entry_hash.into(),
//             timestamp: Timestamp::from(chrono::offset::Utc::now()),
//             tag: LinkTag::new("game_move"),
//             create_link_hash: move_alice_round1_link_header_hash.into(),
//         };
//         let link_to_move_bob_round1 = Link {
//             target: move_bob_round1_entry_hash.into(),
//             timestamp: Timestamp::from(chrono::offset::Utc::now()),
//             tag: LinkTag::new("game_move"),
//             create_link_hash: move_bob_round1_link_header_hash.into(),
//         };
//         let game_moves: Links = vec![link_to_move_alice_round1, link_to_move_bob_round1].into();

//         mock_hdk
//             .expect_get_links()
//             .times(1)
//             .return_once(move |_| Ok(game_moves));

//         let game_move_alice = GameMove {
//             owner: agent_pubkey_alice.clone(),
//             previous_round: prev_round_entry_hash.clone().into(),
//             resources: 10,
//         };
//         let game_move_bob = GameMove {
//             owner: agent_pubkey_bob.clone(),
//             previous_round: prev_round_entry_hash.clone().into(),
//             resources: 10,
//         };

//         let mut element_with_game_move_alice = fixt!(Element);
//         *element_with_game_move_alice.as_entry_mut() =
//             ElementEntry::Present(game_move_alice.try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_move_alice)));

//         let mut element_with_game_move_bob = fixt!(Element);
//         *element_with_game_move_bob.as_entry_mut() =
//             ElementEntry::Present(game_move_bob.try_into().unwrap());
//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_move_bob)));

//         let header_hash_next_round = fixt!(HeaderHash);
//         let header_hash_next_round_closure = header_hash_next_round.clone();
//         mock_hdk
//             .expect_create()
//             .times(1)
//             .return_once(move |_| Ok(header_hash_next_round_closure));

//         let entry_hash_game_session = fixt!(EntryHash);
//         mock_hdk
//             .expect_hash_entry()
//             .times(1)
//             .return_once(move |_| Ok(entry_hash_game_session));
//         let entry_hash_scores = fixt!(EntryHash);
//         mock_hdk
//             .expect_hash_entry()
//             .times(1)
//             .return_once(move |_| Ok(entry_hash_scores));

//         mock_hdk
//             .expect_remote_signal()
//             .times(1)
//             .return_once(move |_| Ok(()));

//         hdk::prelude::set_hdk(mock_hdk);
//         let result = try_to_close_round(prev_round_entry_hash.clone());
//         assert_eq!(result.unwrap(), HeaderHashB64::from(header_hash_next_round.clone()));
//     }

//     #[test]
//     // #[ignore = "WIP should send scores "]
//     fn test_try_to_close_round_success_end_game_resources_depleted(){
//         println!("start test");
//         let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

//         let mut mock_hdk = hdk::prelude::MockHdkT::new();
//         let game_params = GameParams {
//             regeneration_factor: 1,
//             start_amount: 100,
//             num_rounds: 1,
//             resource_coef: 3,
//             reputation_coef: 2,
//         };
//         let game_round = GameRound {
//             round_num: 0,
//             session: session_entry_hash.into(),
//             round_state: RoundState {
//                 resource_amount: 100,
//                 player_stats: HashMap::new(),
//             },
//             previous_round_moves: vec![],
//         };

//         let game_session = GameSession {
//             owner: agent_pubkey_alice.clone(),
//             status: SessionState::InProgress,
//             game_params,
//             players: vec![agent_pubkey_alice.clone(), agent_pubkey_bob.clone()],
//         };

//         let mut element_with_game_round: Element = fixt!(Element);
//         *element_with_game_round.as_entry_mut() = ElementEntry::Present(game_round.clone().try_into().unwrap());

//         let mut element_with_game_session: Element = fixt!(Element);
//         *element_with_game_session.as_entry_mut() = ElementEntry::Present(game_session.clone().try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_round)));

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_session)));


//         let move_alice_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let move_bob_round1_entry_hash = EntryHashB64::from(fixt!(EntryHash));
//         let move_alice_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
//         let move_bob_round1_link_header_hash = HeaderHashB64::from(fixt!(HeaderHash));
//         let link_to_move_alice_round1 = Link {
//             target: move_alice_round1_entry_hash.into(),
//             timestamp: Timestamp::from(chrono::offset::Utc::now()),
//             tag: LinkTag::new("game_move"),
//             create_link_hash: move_alice_round1_link_header_hash.into(),
//         };
//         let link_to_move_bob_round1 = Link {
//             target: move_bob_round1_entry_hash.into(),
//             timestamp: Timestamp::from(chrono::offset::Utc::now()),
//             tag: LinkTag::new("game_move"),
//             create_link_hash: move_bob_round1_link_header_hash.into(),
//         };
//         let game_moves: Links = vec![link_to_move_alice_round1, link_to_move_bob_round1].into();

//         mock_hdk
//             .expect_get_links()
//             .times(1)
//             .return_once(move |_| Ok(game_moves));

//         let game_move_alice = GameMove {
//             owner: agent_pubkey_alice.clone(),
//             previous_round: prev_round_entry_hash.clone().into(),
//             resources: 10,
//         };
//         let game_move_bob = GameMove {
//             owner: agent_pubkey_bob.clone(),
//             previous_round: prev_round_entry_hash.clone().into(),
//             resources: 100, // bob takes all the resources at once
//         };

//         let mut element_with_game_move_alice = fixt!(Element);
//         *element_with_game_move_alice.as_entry_mut() =
//             ElementEntry::Present(game_move_alice.try_into().unwrap());

//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_move_alice)));

//         let mut element_with_game_move_bob = fixt!(Element);
//         *element_with_game_move_bob.as_entry_mut() =
//             ElementEntry::Present(game_move_bob.try_into().unwrap());
//         mock_hdk
//             .expect_get()
//             .times(1)
//             .return_once(move |_| Ok(Some(element_with_game_move_bob)));

//         // let header_hash_final_round = fixt!(HeaderHash);
//         // let header_hash_final_round_closure = header_hash_final_round.clone();
//         let entry_hash_scores = fixt!(EntryHash);
//         let game_scores = GameScores{
//             game_session: game_session.clone(),
//             game_session_entry_hash: EntryHashB64::from(entry_hash_scores),
//         };
//         // mock_hdk
//         //     .expect_create()
//         //     // .with(mockall::predicate::eq(
//         //     //     EntryWithDefId::try_from(game_scores).unwrap()
//         //     // ))
//         //     .times(1)
//         //     .return_once(move |_| Ok(header_hash_final_round_closure));


//         let entry_hash_game_session = fixt!(EntryHash);
//         mock_hdk
//             .expect_hash_entry()
//             .times(1)
//             .return_once(move |_| Ok(entry_hash_game_session));
        
//         let entry_hash_scores = fixt!(EntryHash);
//         let header_hash_scores = fixt!(HeaderHash);
//         let header_hash_scores_closure = header_hash_scores.clone();
//         mock_hdk
//             .expect_hash_entry()
//             .times(1)
//             .return_once(move |_| Ok(entry_hash_scores));
//         mock_hdk
//             .expect_create()
//             // .with(mockall::predicate::eq(EntryWithDefId::try_from(&game_scores).unwrap()))
//             .times(1)
//             .return_once(move |_| Ok(header_hash_scores_closure));
//         mock_hdk
//             .expect_remote_signal()
//             .times(1)
//             .return_once(move |_| Ok(()));

//         hdk::prelude::set_hdk(mock_hdk);
//         let result = try_to_close_round(prev_round_entry_hash.clone());
//         assert_eq!(result.unwrap(), HeaderHashB64::from(header_hash_scores.clone()));
//     }

//     #[test]
//     #[ignore = "not implemented"]
//     fn test_try_to_close_round_end_game_all_rounds_played(){

//     }

//     #[test]
//     #[ignore = "refactoring"]
//     fn test_calculate_round_state() {
//         let gp = GameParams {
//             regeneration_factor: 1,
//             start_amount: 100,
//             num_rounds: 3,
//             resource_coef: 3,
//             reputation_coef: 2,
//         };

//         let p1_key = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let move1 = GameMove {
//             owner: p1_key.clone().into(),
//             previous_round: EntryHashB64::from(fixt!(EntryHash)),
//             resources: 5,
//         };

//         let p2_key = AgentPubKeyB64::from(fixt!(AgentPubKey));
//         let move2 = GameMove {
//             owner: p2_key.clone(),
//             previous_round: EntryHashB64::from(fixt!(EntryHash)),
//             resources: 10,
//         };
//         let s = calculate_round_state(gp.clone(), vec![move1, move2]);
//         assert_eq!(gp.clone().start_amount - 15, s.resource_amount);

//         let stats_p1: (ResourceAmount, ReputationAmount) =
//             *s.player_stats.get(&p1_key.clone().into()).unwrap();
//         assert_eq!(stats_p1.0, 5);
//         assert_eq!(stats_p1.1, 0);

//         let stats_p2: (ResourceAmount, ReputationAmount) =
//             *s.player_stats.get(&p2_key.clone().into()).unwrap();
//         assert_eq!(stats_p2.0, 10);
//         assert_eq!(stats_p1.1, 0);
//     }
// }
