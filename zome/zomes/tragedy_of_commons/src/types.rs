use std::collections::HashMap;
// NOTE(e-nastasia): I don't like that we include everything here, I'd like to make
// that import more precise. But maybe that's ok?
use hdk::prelude::*;
use holo_hash::AgentPubKeyB64;

pub type ResourceAmount = i32;
pub type ReputationAmount = i32;
// TODO(e-nastasia): how do we make this datatype serializable?
pub type PlayerStats = HashMap<AgentPubKeyB64, (ResourceAmount, ReputationAmount)>;

/// Generates empty PlayerStats with 0 values for every player in players
pub fn new_player_stats(players: Vec<AgentPubKeyB64>) -> PlayerStats {
    players
        .into_iter()
        .map(move |pub_key| (pub_key, (0, 0)))
        .collect::<PlayerStats>()
}
