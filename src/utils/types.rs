use std::{collections::HashMap, default};

pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
}

#[derive(Debug, Clone)]
pub enum DataAppendState {
    Commited,
    PrepareToCommit,
    None,
}

impl Default for DataAppendState {
    fn default() -> Self {
        DataAppendState::None
    }
}

pub type Term = u128;

