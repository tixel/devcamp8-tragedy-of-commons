// // NOTE: didn't had time to figure out how to apply this once on a lib level
// // TODO: remove it later

use std::{cell::RefCell, collections::HashMap};

#[allow(dead_code)]
use hdk::prelude::*;
use mockall::automock;

use crate::{
    game_move::GameMove,
    game_round::{GameRound, RoundState},
    game_session::GameSession,
    utils::try_get_and_convert,
};

#[cfg(feature = "mock")]
thread_local!(pub static REPO: RefCell<Box<dyn RepositoryT>> = RefCell::new(Box::new(ErrRepository)));

#[cfg(not(feature = "mock"))]
thread_local!(pub static REPO: RefCell<Box<dyn RepositoryT>> = RefCell::new(Box::new(ErrRepository)));

#[cfg_attr(feature = "mock", automock)]
pub trait RepositoryT: Send + Sync {
    fn try_it(&self) -> GameRound;
    fn try_get_game_round(&self, entry_hash: EntryHash) -> GameRound;
    fn try_get_game_session(&self, entry_hash: EntryHash) -> GameSession;
    fn try_get_game_moves(&self, entry_hash: EntryHash) -> Links;
    fn try_get_game_move(&self, entry_hash: EntryHash) -> GameMove;
}

pub struct ErrRepository;

impl RepositoryT for ErrRepository {
    fn try_it(&self) -> GameRound {
        unimplemented!()
    }
    fn try_get_game_round(&self, _entry_hash: EntryHash) -> GameRound {
        unimplemented!()
    }
    fn try_get_game_session(&self, _entry_hash: EntryHash) -> GameSession {
        unimplemented!()
    }
    fn try_get_game_moves(&self, _entry_hash: EntryHash) -> Links {
        unimplemented!()
    }
    fn try_get_game_move(&self, _entry_hash: EntryHash) -> GameMove {
        unimplemented!()
    }
}

#[derive(Copy, Clone)]
pub struct Repository;

impl RepositoryT for Repository {
    fn try_it(&self) -> GameRound {
        let session_entry_hash = EntryHash::from_raw_32(vec![
            219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219,
            219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219, 219,
        ]);
        let game_round = GameRound {
            round_num: 0,
            session: session_entry_hash.clone(),
            round_state: RoundState {
                resource_amount: 100,
                player_stats: HashMap::new(),
            },
            previous_round_moves: vec![],
        };
        game_round
    }

    fn try_get_game_round(&self, entry_hash: EntryHash) -> GameRound {
        try_get_and_convert(entry_hash).ok().unwrap() // DIRTY -> clean up
    }

    fn try_get_game_session(&self, entry_hash: EntryHash) -> GameSession {
        try_get_and_convert(entry_hash).ok().unwrap() // DIRTY -> clean up
    }
    fn try_get_game_moves(&self, entry_hash: EntryHash) -> Links {
        let result = get_links(entry_hash, Some(LinkTag::new("game_move")));
        let empty_links: Links = vec![].into();
        let l = result.ok().unwrap_or(empty_links);
        l
    }
    fn try_get_game_move(&self, entry_hash: EntryHash) -> GameMove {
        try_get_and_convert(entry_hash).ok().unwrap()
    }
}

#[cfg(test)]
pub fn set_repository<R: 'static>(repo: R)
where
    R: RepositoryT,
{
    REPO.with(|h| {
        *h.borrow_mut() = Box::new(repo);
    });
}
