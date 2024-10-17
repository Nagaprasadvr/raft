mod utils;
use utils::helpers::{LastHeartbeat, MessageType, NodeConfig, PublishMessage, RaftState};

use futures::StreamExt;
use libp2p::{gossipsub, mdns, noise, swarm::NetworkBehaviour, swarm::SwarmEvent, PeerId};
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::time::Duration;
use tokio::{io, select, time};
use tracing_subscriber::EnvFilter;

#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut node_config = NodeConfig::<String>::init();
    let mut raft_state = RaftState::init();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_quic()
        .with_behaviour(|key| {
            // To content-address message, we can take the hash of message and use it as an ID.
            let message_id_fn = |message: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                message.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            // Set a custom gossipsub configuration
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
                .build()
                .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

            // build a gossipsub network behaviour
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )?;

            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
            Ok(MyBehaviour { gossipsub, mdns })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Create a Gossipsub topic
    let topic = gossipsub::IdentTopic::new("raft-consensus");

    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    swarm.listen_on("/ip4/0.0.0.0/udp/0/quic-v1".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    // Read full lines from stdin

    println!("Peer ID : {}", swarm.local_peer_id().to_base58());

    // Create an interval timer
    let mut heartbeat_interval = time::interval(Duration::from_secs(10));

    let mut leader_elections_interval = time::interval(Duration::from_secs(20));

    let mut check_for_leaders = false;


    loop {
        select! {
            // _ = leader_elections_interval.tick() =>{
                // println!("leader_elections");
            //}
            //polling
            _ = heartbeat_interval.tick() => {
                raft_state.leader_election().await;
                // raft_state.print();
                println!("leader:{:?}",raft_state.leader);
                if  node_config.peer_id.is_some(){
                    if raft_state.is_leader(node_config.peer_id.unwrap()){
                        raft_state.announce_leader(node_config.peer_id.unwrap(),&mut swarm,topic.clone());
                    let _ = raft_state.send_heart_beat(&mut swarm, topic.clone());

                    }
               }

            }

            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discovered a new peer: {peer_id}");
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                        raft_state.add_peer(peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        println!("mDNS discover peer has expired: {peer_id}");
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) =>{
                    println!("event message");
                    //if node_config.peer_id.is_some() && raft_state.is_leader(node_config.peer_id.unwrap()){
                     //continue;
                    //}
                    println!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data));

                    let deserialized_msg = bincode::deserialize::<PublishMessage>(&message.data);

                    println!("des-msg:{:?}",deserialized_msg);

                    match deserialized_msg {
                        Ok(msg) =>{
                            match msg.msg_type {
                        MessageType::Heartbeat=>{
                            println!("last heartbeat:{:?}",msg.data);
                            raft_state.last_heartbeat = Some(LastHeartbeat{ time_stamp:msg.data.parse::<u128>().unwrap() })
                        },
                        MessageType::LeaderAnnouncement=>{
                            println!("leader:{:?}",msg.data);
                            raft_state.leader = Some( PeerId::from_str(&msg.data).unwrap());

                        },
                        MessageType::LogData=>{},
                        _ =>{}
                    }

                        },
                        Err(e)=>{
                            println!("Error:{:?}",e);
                        }
                    }
                    },
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Local node is listening on {address}");
                    let peer_id = swarm.local_peer_id();
                    node_config.start(*peer_id);
                    raft_state.add_peer(*peer_id)
                }
                _ => {}
            }

        }
    }
}
