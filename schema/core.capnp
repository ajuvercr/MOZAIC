@0xad560a5d7666face;

struct MozaicMessage {
    sender @0: Data;
    receiver @1: Data;
    typeId @2: UInt64;
    payload @3: AnyPointer;
}

struct TerminateStream {}

struct Initialize {}

# This message is sent when a reactor is about to be dropped
struct Drop {}

struct Message {
    typeId @0: UInt64;
    data @1: AnyPointer;
}

struct ActorJoined {
    id @0: Data;
}

struct ActorsJoined {
    ids @0: List(Data);
}

struct Identify {
    key @0: UInt64;
}

# You can send this message to indicate something it should close
struct Close {}

struct CloseLink {}
