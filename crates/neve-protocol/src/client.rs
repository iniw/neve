use rkyv::{Archive, Deserialize, Serialize, rancor::Error, util::AlignedVec};

#[derive(Archive, Serialize, Deserialize, Debug, PartialEq)]
#[rkyv(derive(Debug))]
pub enum Event {
    Register { name: String },
    Message { data: String },
}

pub fn to_bytes(event: Event) -> AlignedVec {
    rkyv::to_bytes::<Error>(&event).unwrap()
}

pub fn access(bytes: &[u8]) -> &ArchivedEvent {
    rkyv::access::<_, Error>(bytes).unwrap()
}
