use super::types::{DataAppendState, Term};
use crate::MyBehaviour;
use async_std::task::sleep;
use libp2p::gossipsub::{Hasher, MessageId, PublishError};
use libp2p::gossipsub::{Topic, TopicHash};
use libp2p::{PeerId, Swarm};
use serde::*;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::DerefMut;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::errors::Error;

#[derive(Debug, Clone)]
pub struct LatestCommit {
    pub key: String,
    pub timestamp: u64,
}

#[derive(Clone, Debug)]
pub struct StagingData<T: Clone> {
    pub key: String,
    pub data: T,
}

pub type LatestPrepareToCommit = LatestCommit;

#[derive(Debug, Clone)]
pub struct NodeDataState<T: Clone> {
    pub state: DataAppendState,
    pub data: HashMap<String, T>,
    pub staging_data: Option<StagingData<T>>,
    pub latest_commit: Option<LatestCommit>,
    pub latest_prepare_to_commit: Option<LatestPrepareToCommit>,
}

impl<T: Clone> NodeDataState<T> {
    pub fn commit(&mut self) -> Result<String, Error> {
        if let Some(data) = &self.staging_data {
            self.data.insert(data.key.clone(), data.data.clone());
            return Ok(String::from("commited"));
        }
        return Err(Error::PrepareToCommitDataNotFound);
    }
    pub fn prepare_to_commit(&mut self, staging_data: StagingData<T>) {
        self.staging_data = Some(StagingData {
            key: staging_data.key,
            data: staging_data.data,
        })
    }
}

#[derive(Debug, Clone)]

pub struct LastHeartbeat {
    pub time_stamp: u128,
}

#[derive(Debug, Clone)]

pub struct RaftState {
    pub leader: Option<PeerId>,
    pub term: Term,
    pub candidate_votes: Option<HashMap<PeerId, u32>>,
    pub peers: Vec<PeerId>,
    pub last_heartbeat: Option<LastHeartbeat>,
}

impl RaftState {
    pub fn init() -> RaftState {
        RaftState {
            leader: None,
            term: 0,
            candidate_votes: None,
            peers: Vec::new(),
            last_heartbeat: None,
        }
    }

    pub fn announce_leader<H: libp2p::gossipsub::Hasher + Clone>(
        &self,
        peer_id: PeerId,
        swarm: &mut Swarm<MyBehaviour>,
        topic: Topic<H>,
    ) {
        if self.is_leader(peer_id) {
            let message = PublishMessage {
                msg_type: MessageType::LeaderAnnouncement,
                data: peer_id.to_base58(),
            };
            
            println!("announce_leader");
            let _ = swarm
                .behaviour_mut()
                .gossipsub
                .publish(topic, bincode::serialize(&message).unwrap());
        }
    }

    pub fn is_leader(&self, peer_id: PeerId) -> bool {
        if let Some(leader) = self.leader {
            return leader == peer_id;
        }
        false
    }

    pub fn print(&self) {
        println!("Raft:{:?}", self)
    }
    pub fn get_leader(&self) -> Option<PeerId> {
        self.leader
    }
    pub async fn leader_election(&mut self) {
        
        sleep(Duration::from_secs(10)).await;

        if self.leader.is_some() {
            return;
        }
        if let Some(candidate_votes) = &self.candidate_votes {
            let mut max_votes = 0u32;
            let mut elected_leader: Option<PeerId> = None;
            for i in candidate_votes.iter() {
                if *i.1 > max_votes {
                    max_votes = *i.1;
                    elected_leader = Some(i.0.clone());
                }
            }

            self.leader = elected_leader;
        }
        //no candidates_votes
        else {
            if self.peers.len().eq(&1) {
                self.leader = Some(self.peers[0]);
            }
        }
    }

    pub fn send_heart_beat<H: libp2p::gossipsub::Hasher + Clone>(
        &mut self,
        swarm: &mut Swarm<MyBehaviour>,
        topic: Topic<H>,
    ) -> Result<MessageId, PublishError> {
        let time_stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        let message = PublishMessage {
            msg_type: MessageType::Heartbeat,
            data: time_stamp.to_string(),
        };
        let res = swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, bincode::serialize(&message).unwrap());
        match res {
            Ok(msg_id) => {
                self.last_heartbeat = Some(LastHeartbeat { time_stamp });
                Ok(msg_id)
            }
            Err(msg) => Err(msg),
        }
    }
    pub fn add_peer(&mut self, peer: PeerId) {
        if self.peers.contains(&peer) {
            return;
        }
        self.peers.push(peer);
    }
}

#[derive(Debug, Clone)]
pub struct NodeConfig<T: Clone> {
    pub peer_id: Option<PeerId>,
    pub term: Term,
    pub data_state: Option<NodeDataState<T>>,
}

impl<T: Clone + Debug + DerefMut> NodeConfig<T> {
    pub fn default() -> NodeConfig<T> {
        NodeConfig {
            peer_id: None,
            term: 0,
            data_state: None,
        }
    }

    pub fn init() -> NodeConfig<T> {
        NodeConfig {
            peer_id: None,
            term: 0,
            data_state: Some(NodeDataState {
                state: Default::default(),
                data: HashMap::new(),
                latest_commit: None,
                latest_prepare_to_commit: None,
                staging_data: None,
            }),
        }
    }

    pub fn start(&mut self, peer_id: PeerId) {
        self.peer_id = Some(peer_id);
    }

    pub fn update_term(&mut self) {
        self.term += 1;
    }

    pub fn print(&self) {
        println!("NodeConfig:{:?}", &self)
    }
}

pub async fn get_res(elapsed: Duration) -> bool {
    elapsed > Duration::from_secs(10)
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageType {
    Heartbeat,
    LogData,
    LeaderAnnouncement,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishMessage {
    pub msg_type: MessageType,
    pub data: String,
}
