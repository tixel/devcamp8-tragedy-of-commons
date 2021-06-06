use crate::game_move::GameMove;
use crate::game_session::GameParams;
use crate::types::{PlayerStats, ReputationAmount, ResourceAmount};
use hdk::prelude::*;
use holo_hash::AgentPubKeyB64;
use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::*;
    use ::fixt::prelude::*;
    use hdk::prelude::*;
    use mockall::mock;
    use std::vec;

    #[test]
    fn test_calculate_round_state() {
        let gp = GameParams {
            regeneration_factor: 1.1,
            start_amount: 100,
            num_rounds: 3,
            resource_coef: 3,
            reputation_coef: 2,
        };

        let p1_key = fixt!(AgentPubKey);
        let move1 = GameMove {
            owner: p1_key.clone(),
            previous_round: fixt!(EntryHash),
            resources: 5,
        };

        let p2_key = fixt!(AgentPubKey);
        let move2 = GameMove {
            owner: p2_key.clone(),
            previous_round: fixt!(EntryHash),
            resources: 10,
        };
        let s = calculate_round_state(gp.clone(), vec![move1, move2]);
        assert_eq!(gp.clone().start_amount - 15, s.resource_amount);

        let stats_p1: (ResourceAmount, ReputationAmount) =
            *s.player_stats.get(&p1_key.clone()).unwrap();
        assert_eq!(stats_p1.0, 5);
        assert_eq!(stats_p1.1, 0);

        let stats_p2: (ResourceAmount, ReputationAmount) =
            *s.player_stats.get(&p2_key.clone()).unwrap();
        assert_eq!(stats_p2.0, 10);
        assert_eq!(stats_p1.1, 0);
    }
}
