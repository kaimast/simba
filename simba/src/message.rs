use crate::logic::{
    GossipMessage, NakamotoMessage, PbftMessage, SnowballMessage, SpeedTestMessage,
};

#[derive(PartialEq, Eq, Debug, derive_more::Display)]
pub enum MessageType {
    Block,
    Transaction,
    Other,
}

#[derive(Clone, Debug)]
pub enum Message {
    Nakamoto(NakamotoMessage),
    PracticalBFT(PbftMessage),
    Dummy(DummyMessage),
    SpeedTest(SpeedTestMessage),
    Gossip(GossipMessage),
    Snowball(SnowballMessage),
}

#[derive(Default, Debug, Clone)]
pub struct DummyMessage {}

impl From<NakamotoMessage> for Message {
    fn from(msg: NakamotoMessage) -> Self {
        Self::Nakamoto(msg)
    }
}

impl From<GossipMessage> for Message {
    fn from(msg: GossipMessage) -> Self {
        Self::Gossip(msg)
    }
}

impl From<PbftMessage> for Message {
    fn from(msg: PbftMessage) -> Self {
        Self::PracticalBFT(msg)
    }
}

impl From<SpeedTestMessage> for Message {
    fn from(msg: SpeedTestMessage) -> Self {
        Self::SpeedTest(msg)
    }
}

impl From<SnowballMessage> for Message {
    fn from(msg: SnowballMessage) -> Self {
        Self::Snowball(msg)
    }
}

impl From<DummyMessage> for Message {
    fn from(msg: DummyMessage) -> Self {
        Self::Dummy(msg)
    }
}

impl TryInto<GossipMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<GossipMessage, ()> {
        if let Self::Gossip(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl TryInto<NakamotoMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<NakamotoMessage, ()> {
        if let Self::Nakamoto(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl TryInto<SpeedTestMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<SpeedTestMessage, ()> {
        if let Self::SpeedTest(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl TryInto<PbftMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<PbftMessage, ()> {
        if let Self::PracticalBFT(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl TryInto<SnowballMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<SnowballMessage, ()> {
        if let Self::Snowball(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl TryInto<DummyMessage> for Message {
    type Error = ();

    fn try_into(self) -> Result<DummyMessage, ()> {
        if let Self::Dummy(inner) = self {
            Ok(inner)
        } else {
            Err(())
        }
    }
}

impl asim::network::NetworkMessage for Message {
    /// Returns the size of this message in bytes
    fn get_size(&self) -> u64 {
        match self {
            Self::Dummy(_) => 0,
            Self::Gossip(msg) => msg.get_size(),
            Self::Snowball(msg) => msg.get_size(),
            Self::Nakamoto(msg) => msg.get_size(),
            Self::PracticalBFT(msg) => msg.get_size(),
            Self::SpeedTest(msg) => msg.get_size(),
        }
    }
}

impl Message {
    pub fn get_type(&self) -> MessageType {
        match self {
            Self::SpeedTest(_) | Self::Dummy(_) => MessageType::Other,
            Self::Gossip(msg) => msg.get_type(),
            Self::Snowball(msg) => msg.get_type(),
            Self::Nakamoto(msg) => msg.get_type(),
            Self::PracticalBFT(msg) => msg.get_type(),
        }
    }
}
