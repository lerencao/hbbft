//! The local message delivery system.
use std::fmt::Debug;
use std::sync::Arc;
use crossbeam::Scope;
use crossbeam_channel::{unbounded, Sender, Receiver};
use proto::Message;

/// Message destination can be either of the two:
///
/// 1) `All`: all nodes, if sent to socket tasks, or all local algorithm
/// instances, if received from socket tasks.
///
/// 2) `Node(i)`: node i or local algorithm instances with the node index i.
#[derive(Clone, Debug)]
pub enum Target {
    All,
    Node(usize)
}

/// Message with a designated target.
#[derive(Clone, Debug)]
pub struct TargetedMessage<T: Clone + Debug + Send + Sync> {
    pub target: Target,
    pub message: Message<T>
}

impl<T: Clone + Debug + Send + Sync> TargetedMessage<T> {
    /// Initialises a message while checking parameter preconditions.
    pub fn new(target: Target, message: Message<T>) -> Option<Self> {
        match target {
            Target::Node(i) if i == 0 => {
                // Remote node indices start from 1.
                None
            },
            _ => Some(TargetedMessage{target, message})
        }
    }
}

/// Message sent by a given source. The sources are consensus nodes indexed 1
/// through N where N is the total number of nodes. Sourced messages are
/// required when it is essential to know the message origin but the set of
/// recepients is unkown without further computation which is irrelevant to the
/// message delivery task.
#[derive(Clone, Debug)]
pub struct SourcedMessage<T: Clone + Debug + Send + Sync> {
    pub source: usize,
    pub message: Message<T>
}

/// The messaging struct allows for targeted message exchange between comms
/// tasks on one side and algo tasks on the other.
pub struct Messaging<T: Clone + Debug + Send + Sync> {
    /// The total number of consensus nodes for indexing purposes.
    num_nodes: usize,

    /// Transmit sides of message channels to comms threads.
    to_comms_txs: Vec<Sender<Message<T>>>,
    /// Receive side of the routed message channel from comms threads.
    from_comms_rx: Receiver<SourcedMessage<T>>,
    /// Transmit sides of message channels to algo threads.
    to_algo_txs: Vec<Sender<SourcedMessage<T>>>,
    /// Receive side of the routed message channel from comms threads.
    from_algo_rx: Receiver<TargetedMessage<T>>,

    /// RX handles to be used by comms tasks.
    to_comms_rxs: Vec<Receiver<Message<T>>>,
    /// TX handle to be used by comms tasks.
    from_comms_tx: Sender<SourcedMessage<T>>,
    /// RX handles to be used by algo tasks.
    to_algo_rxs: Vec<Receiver<SourcedMessage<T>>>,
    /// TX handle to be used by algo tasks.
    from_algo_tx: Sender<TargetedMessage<T>>,
}

impl<T: Clone + Debug + Send + Sync> Messaging<T> {
    /// Initialises all the required TX and RX handles for the case on a total
    /// number `num_nodes` of consensus nodes.
    pub fn new(num_nodes: usize) -> Self
    {
        let to_comms: Vec<(Sender<Message<T>>, Receiver<Message<T>>)>
            = (0 .. num_nodes - 1)
            .map(|_| unbounded())
            .collect();
        let to_comms_txs = to_comms.iter()
            .map(|(tx, _)| tx.to_owned())
            .collect();
        let to_comms_rxs: Vec<Receiver<Message<T>>> = to_comms.iter()
            .map(|(_, rx)| rx.to_owned())
            .collect();
        let (from_comms_tx, from_comms_rx) = unbounded();
        let to_algo: Vec<(Sender<SourcedMessage<T>>,
                          Receiver<SourcedMessage<T>>)>
            = (0 .. num_nodes)
            .map(|_| unbounded())
            .collect();
        let to_algo_txs = to_algo.iter()
            .map(|(tx, _)| tx.to_owned())
            .collect();
        let to_algo_rxs: Vec<Receiver<SourcedMessage<T>>> = to_algo.iter()
            .map(|(_, rx)| rx.to_owned())
            .collect();
        let (from_algo_tx, from_algo_rx) = unbounded();

        Messaging {
            num_nodes: num_nodes,

            // internally used handles
            to_comms_txs: to_comms_txs,
            from_comms_rx: from_comms_rx,
            to_algo_txs: to_algo_txs,
            from_algo_rx: from_algo_rx,

            // externally used handles
            to_comms_rxs: to_comms_rxs,
            from_comms_tx: from_comms_tx,
            to_algo_rxs: to_algo_rxs,
            from_algo_tx: from_algo_tx,
        }
    }

    pub fn num_nodes(&self) -> usize {
        self.num_nodes
    }

    pub fn to_comms_rxs(&self) -> &Vec<Receiver<Message<T>>> {
        &self.to_comms_rxs
    }

    pub fn from_comms_tx(&self) -> &Sender<SourcedMessage<T>> {
        &self.from_comms_tx
    }

    pub fn to_algo_rxs(&self) -> &Vec<Receiver<SourcedMessage<T>>> {
        &self.to_algo_rxs
    }

    pub fn from_algo_tx(&self) -> &Sender<TargetedMessage<T>> {
        &self.from_algo_tx
    }

    /// Spawns the message delivery thread in a given thread scope.
    pub fn spawn<'a>(&self, scope: &Scope<'a>)
    where T: 'a
    {
        let to_comms_txs = self.to_comms_txs.to_owned();
        let from_comms_rx = self.from_comms_rx.to_owned();
        let to_algo_txs = self.to_algo_txs.to_owned();
        let from_algo_rx = self.from_algo_rx.to_owned();

        scope.spawn(move || {
            // This loop forwards messages according to their metadata.
            loop { select_loop! {
                recv(from_algo_rx, message) => {
                    match message {
                        TargetedMessage {
                            target: Target::All,
                            message
                        } => {
                            for tx in to_comms_txs.iter() {
                                tx.send(message.clone()).unwrap();
                            }
                        },
                        TargetedMessage {
                            target: Target::Node(i),
                            message
                        } => {
                            // Remote node indices start from 1.
                            assert!(i > 0);
                            // Convert node index to vector index.
                            let i = i - 1;

                            if i < to_comms_txs.len() {
                                to_comms_txs[i].send(message.clone())
                                    .unwrap();
                            }
                            else {
                                error!("Target {} does not exist", i);
                            }
                        }
                    }
                },
                recv(from_comms_rx, message) => {
                    for tx in to_algo_txs.iter() {
                        tx.send(message.clone()).unwrap();
                    }
                }
            }} // end of select_loop!
        });
    }
}
