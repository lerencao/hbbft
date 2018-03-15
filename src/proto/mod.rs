//! Construction of messages from protobuf buffers.
pub mod message;

use ring::digest::Algorithm;
use merkle::proof::{Proof, Lemma, Positioned};
//use protobuf::Message;
use self::message::*;
use protobuf::error::ProtobufResult;
use protobuf::core::parse_from_bytes;

/// Kinds of message sent by nodes participating in consensus.
pub enum Message<T> {
    Broadcast(BroadcastMessage<T>),
    Agreement(AgreementMessage)
}

/// The three kinds of message sent during the reliable broadcast stage of the
/// consensus algorithm.
pub enum BroadcastMessage<T> {
    Value(Proof<T>),
    Echo(Proof<T>),
    Ready(Vec<u8>)
}

/// Messages sent during the binary Byzantine agreement stage.
pub enum AgreementMessage {
    // TODO
}

impl<T> Message<T> {
    /// Translation from protobuf to the regular type.
    pub fn from_proto(algorithm: &'static Algorithm,
                      mut proto: message::MessageProto) -> Option<Self>
    where T: From<Vec<u8>>
    {
        if proto.has_broadcast() {
            BroadcastMessage::from_proto(proto.take_broadcast(), algorithm)
                .map(|b| Message::Broadcast(b))
        }
        else if proto.has_agreement() {
            AgreementMessage::from_proto(proto.take_agreement())
                .map(|a| Message::Agreement(a))
        }
        else {
            None
        }
    }

    pub fn into_proto(self) -> MessageProto
    where T: Into<Vec<u8>>
    {
        let mut m = MessageProto::new();
        match self {
            Message::Broadcast(b) => {
                m.set_broadcast(b.into_proto());
            },
            Message::Agreement(a) => {
                m.set_agreement(a.into_proto());
            }
        }
        m
    }
}

impl<T> BroadcastMessage<T> {
    pub fn into_proto(self) -> BroadcastProto
    where T: Into<Vec<u8>>
    {
        let mut b = BroadcastProto::new();
        match self {
            BroadcastMessage::Value(p) => {
                let mut v = ValueProto::new();
                v.set_proof(ProofProto::into_proto(p));
                b.set_value(v);
            },
            BroadcastMessage::Echo(p) => {
                let mut e = EchoProto::new();
                e.set_proof(ProofProto::into_proto(p));
                b.set_echo(e);
            },
            BroadcastMessage::Ready(h) => {
                let mut r = ReadyProto::new();
                r.set_root_hash(h);
            }
        }
        b
    }

    pub fn from_proto(mut mp: BroadcastProto,
                      algorithm: &'static Algorithm)
                      -> Option<Self>
    where T: From<Vec<u8>>
    {
        if mp.has_value() {
            mp.take_value().take_proof().from_proto(algorithm)
                .map(|p| BroadcastMessage::Value(p))
        }
        else if mp.has_echo() {
            mp.take_echo().take_proof().from_proto(algorithm)
                .map(|p| BroadcastMessage::Echo(p))
        }
        else if mp.has_ready() {
            let h = mp.take_ready().take_root_hash();
            Some(BroadcastMessage::Ready(h))
        }
        else {
            None
        }
    }
}

impl AgreementMessage {
    pub fn into_proto(self) -> AgreementProto
    {
        unimplemented!();
    }

    pub fn from_proto(mut mp: AgreementProto) -> Option<Self>
    {
        unimplemented!();
    }
}

/// Serialisation of `Proof` defined against its protobuf interface to work
/// around the restriction of not being allowed to extend the implementation of
/// `Proof` outside its crate.
impl ProofProto {
    pub fn into_proto<T>(proof: Proof<T>) -> Self
    where T: Into<Vec<u8>>
    {

        let mut proto = Self::new();

        match proof {
            Proof {
                root_hash,
                lemma,
                value,
                ..
            } => {
                proto.set_root_hash(root_hash);
                proto.set_lemma(LemmaProto::into_proto(lemma));
                proto.set_value(value.into());
            }
        }

        proto
    }

    pub fn from_proto<T>(mut self,
                         algorithm: &'static Algorithm)
                         -> Option<Proof<T>>
    where T: From<Vec<u8>>
    {
        if !self.has_lemma() {
            return None;
        }

        self.take_lemma().from_proto().map(|lemma| {
            Proof::new(
                algorithm,
                self.take_root_hash(),
                lemma,
                self.take_value().into(),
            )
        })
    }
}

impl LemmaProto {
    pub fn into_proto(lemma: Lemma) -> Self {
        let mut proto = Self::new();

        match lemma {
            Lemma {node_hash, sibling_hash, sub_lemma} => {
                proto.set_node_hash(node_hash);

                if let Some(sub_proto) = sub_lemma.map(
                    |l| Self::into_proto(*l))
                {
                    proto.set_sub_lemma(sub_proto);
                }

                match sibling_hash {
                    Some(Positioned::Left(hash)) =>
                        proto.set_left_sibling_hash(hash),

                    Some(Positioned::Right(hash)) =>
                        proto.set_right_sibling_hash(hash),

                    None => {}
                }
            }
        }

        proto
    }

    pub fn from_proto(mut self) -> Option<Lemma> {
        let node_hash = self.take_node_hash();

        let sibling_hash = if self.has_left_sibling_hash() {
            Some(Positioned::Left(self.take_left_sibling_hash()))
        } else if self.has_right_sibling_hash() {
            Some(Positioned::Right(self.take_right_sibling_hash()))
        } else {
            None
        };

        if self.has_sub_lemma() {
            // If a `sub_lemma` is present is the Protobuf,
            // then we expect it to unserialize to a valid `Lemma`,
            // otherwise we return `None`
            self.take_sub_lemma().from_proto().map(|sub_lemma| {
                Lemma {
                    node_hash: node_hash,
                    sibling_hash: sibling_hash,
                    sub_lemma: Some(Box::new(sub_lemma)),
                }
            })
        } else {
            // We might very well not have a sub_lemma,
            // in which case we just set it to `None`,
            // but still return a potentially valid `Lemma`.
            Some(Lemma {
                node_hash: node_hash,
                sibling_hash: sibling_hash,
                sub_lemma: None,
            })
        }
    }
}