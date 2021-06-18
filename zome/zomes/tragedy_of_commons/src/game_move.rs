use std::{collections::HashMap, vec};

use crate::prelude::SignedHeader;
use crate::{
    game_round::{self, calculate_round_state, GameRound, RoundState},
    game_session::{GameScores, GameSession, GameSignal, SessionState, SignalPayload},
    persistence::{self, Repository},
    types::ResourceAmount,
    utils::{convert_keys_from_b64, try_get_and_convert, try_get_game_moves},
};
use hdk::prelude::*;
use holo_hash::*;
use mockall::*;

#[hdk_entry(id = "game_move", visibility = "public")]
pub struct GameMove {
    pub owner: AgentPubKeyB64,
    // For the very first round this option would be None, because we create game rounds
    // retrospectively. And since all players are notified by the signal when they can make
    // a move, maybe we could pass that value from there, so that every player has it
    // when they're making a move
    pub previous_round: EntryHashB64,
    pub resources: ResourceAmount,
}
#[derive(Clone, Debug, Serialize, Deserialize, SerializedBytes)]
pub struct GameMoveInput {
    pub resource_amount: ResourceAmount,
    // NOTE: if we're linking all moves to the round, this can never be None
    // as we'll need a base for the link. Instead moves for the round 0 could be
    // linked directly from the game session.
    pub previous_round: EntryHashB64,
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
    // todo: add guard clauses for empty input
    let game_move = GameMove {
        owner: AgentPubKeyB64::from(agent_info()?.agent_initial_pubkey),
        resources: input.resource_amount,
        previous_round: input.previous_round.clone(),
    };
    create_entry(&game_move);
    let entry_hash_game_move = hash_entry(&game_move)?;

    let header_hash_link = create_link(
        input.previous_round.clone().into(),
        entry_hash_game_move.clone(),
        LinkTag::new("game_move"),
    )?;
    // todo: (if we're making a link from round to move) make a link round -> move
    // note: instead of calling try_to_close_Round right here, we can have a UI make
    // this call for us. This way making a move wouldn't be blocked by the other moves'
    // retrieval process and the process of commiting the round entry.
    Ok(header_hash_link.into())
}

// Question: how do we make moves discoverable by the players?
// Option1: make a link from game session / game round to which this move belongs?
//      note: this is where things start to get more complicated with the game round that is
//      only created retrospectively. We will have to manage this duality with link base being
//      either a game session or a game round. But maybe that's not a bad thing? That'll still
//      be a related Holochain entry after all.

// Should retrieve all game moves corresponding to the current round entry (in case of round 0 this
// would actually be a game session entry) and attempt to close the current round by creating it's entry.
// This would solely depend on the amount of moves retrieved being equal to the amount of players in the game
#[hdk_extern]
pub fn try_to_close_round(prev_round_hash: EntryHashB64) -> ExternResult<EntryHash> {
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
    create_next_round_or_end_game(game_session, prev_round, round_state)
}

fn create_next_round_or_end_game(
    game_session: GameSession,
    prev_round: GameRound,
    round_state: RoundState,
) -> ExternResult<EntryHash> {
    if (game_session.game_params.num_rounds < prev_round.round_num)
        || (round_state.resource_amount < 0)
    {
        // emit signal -
        println!("ending game");
        end_game(game_session.clone(), round_state)
    } else {
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
) -> ExternResult<EntryHash> {
    let session_hash = hash_entry(&session)?;
    // TODO: instead of creating a new entry, we should continue the update chain
    // from the previous round entry hash and commit an updated version
    let round = GameRound {
        round_num: prev_round_num + 1,
        round_state: round_state,
        session: session_hash.clone(),
        previous_round_moves: vec![],
    }; 
    create_entry(&round)?;
    let entry_hash_round = hash_entry(&round)?;
    get(entry_hash_round.clone(), GetOptions::content());
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
    tracing::debug!("sending signal to {:?}", session.players.clone());

    Ok(entry_hash_round)
}

fn end_game(session: GameSession, round_state: RoundState) -> ExternResult<EntryHash> {
    let session_hash = hash_entry(&session)?;
    let scores = GameScores {
        // tixel: not sure if we need the full objects or only the hashes or both. The tests will tell...
        game_session: session.clone(),
        game_session_entry_hash: session_hash.clone(),
    };
    create_entry(&scores)?;
    let scores_entry_hash = hash_entry(&scores)?;

    // TODO: update GameSession entry to set it's state to closed

    let signal = ExternIO::encode(GameSignal::GameOver(scores))?;
    // Since we're storing agent keys as AgentPubKeyB64, and remote_signal only accepts
    // the AgentPubKey type, we need to convert our keys to the expected data type
    remote_signal(signal, convert_keys_from_b64(session.players.clone()))?;
    tracing::debug!("sending signal to {:?}", session.players.clone());

    Ok(scores_entry_hash)
}

// Retrieves all available game moves made in a certain round, where entry_hash identifies
// base for the links.
fn get_all_round_moves(round_entry_hash: EntryHash) {
    unimplemented!();
}

mock! {
    SignedHeader {}     // Name of the mock struct, less the "Mock" prefix
    impl Clone for SignedHeader {   // specification of the trait to mock
        fn clone(&self) -> Self;
    }
}

#[cfg(test)]
#[rustfmt::skip]   // skipping formatting is needed, because to correctly import fixt we needed "use ::fixt::prelude::*;" which rustfmt does not like
mod tests {
    use super::*;
    use crate::game_session::{GameScores, GameSession, GameSignal, SessionState, SignalPayload};
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

    use holochain_types::prelude::ElementFixturator;

    #[test]
    fn test_try_to_close_round_fails_not_enough_moves() {
        println!("closing round should fail because only one of two players has made a move.");
        // mock agent info
        let agent_pubkey_alice = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let agent_pubkey_bob = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

        let mut mock_hdk = hdk::prelude::MockHdkT::new();
        let game_params = GameParams {
            regeneration_factor: 1.1,
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
    #[ignore]
    fn test_try_to_close_round() {
        println!("start test");
        // mock agent info
        let agent_pubkey = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let agent2_pubkey = AgentPubKeyB64::from(fixt!(AgentPubKey));
        let prev_round_entry_hash = EntryHashB64::from(fixt!(EntryHash));
        let session_entry_hash = EntryHashB64::from(fixt!(EntryHash));

        let mut mock_hdk = hdk::prelude::MockHdkT::new();
        let game_params = GameParams {
            regeneration_factor: 1.1,
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
            owner: agent_pubkey.clone(),
            status: SessionState::InProgress,
            game_params,
            players: vec![agent_pubkey.clone(), agent2_pubkey],
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
            owner: agent_pubkey.clone(),
            previous_round: prev_round_entry_hash.clone().into(),
            resources: 10,
        };
        let game_move_bob = GameMove {
            owner: agent_pubkey.clone(),
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

        // mock_hdk
        //     .expect_get()
        //     // .with(hdk::prelude::mockall::predicate::eq(
        //     //     GetInput::new(prev_round_entry_hash.clone().into(), GetOptions::latest())))
        //     .times(1)
        //     .return_once(move |_| Ok(el));

        // let input = GameSessionInput {
        //     game_params: game_params,
        //     players: vec![fixt!(AgentPubKey), fixt!(AgentPubKey), fixt!(AgentPubKey)], // 3 random players
        // };

        let header_hash_final_round = fixt!(HeaderHash);
        mock_hdk
            .expect_create()
            .times(1)
            .return_once(move |_| Ok(header_hash_final_round));

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

        // let header_hash_link = fixt!(HeaderHash);
        // mock_hdk
        // .expect_create_link()
        // .times(1)
        // .return_once(move |_| Ok(header_hash_link));

        hdk::prelude::set_hdk(mock_hdk);
        try_to_close_round(prev_round_entry_hash.clone());
    }

    // #[test]
    // fn test_get_data() {
    //     // fn get_data(prev_round_hash: EntryHash) -> Result<(GameSession, GameRound, Links), WasmError> {
    //     //     let prev_round:GameRound = Repository::new().try_get_game_round(prev_round_hash.clone());
    //     //     let game_session:GameSession = Repository::new().try_get_game_session(prev_round.session.clone());
    //     //     let links  = get_links(prev_round_hash, Some(LinkTag::new("game_move")))?;
    //     //     Ok((game_session, prev_round, links))
    //     // }
    //     let prev_round_hash = EntryHash::from_raw_32(vec![219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219]);
    //     let session_entry_hash = EntryHash::from_raw_32(vec![219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219]);
    //     let game_round = GameRound {
    //         round_num: 111,
    //         session: session_entry_hash.clone(),
    //         round_state: RoundState {
    //             resource_amount: 100,
    //             player_stats: HashMap::new(),

    //         },
    //         previous_round_moves: vec![],
    //     };

    //     let mut mock_repo = persistence::MockRepositoryT::new();
    //     mock_repo.expect_try_it()
    //             .times(1)
    //             .return_once(move || game_round);
    //     persistence::set_repository(mock_repo);
    //     let x = get_data_t(prev_round_hash);
    //     assert_eq!(112, x.round_num);
    // }
}
