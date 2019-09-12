#![allow(dead_code)]

extern crate bytes;
extern crate hex;

extern crate tokio_core;
extern crate tokio;
#[macro_use]
extern crate futures;
extern crate rand;

extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate error_chain;

extern crate serde_derive;

extern crate capnp;
extern crate capnp_futures;

#[macro_use]
extern crate mozaic_derive;

pub mod messaging;
pub mod net;

pub mod server;
pub mod client;

pub mod modules;

pub mod errors;

pub mod core_capnp {
    %%/core_capnp.rs%%
}

pub mod chat_capnp {
    %%/chat_capnp.rs%%
}

pub mod my_capnp {
    %%/my_capnp.rs%%
}

pub mod network_capnp {
    %%/network_capnp.rs%%
}

pub mod match_control_capnp {
    %%/match_control_capnp.rs%%
}

pub mod mozaic_cmd_capnp {
    %%/mozaic/cmd_capnp.rs%%
}

pub mod log_capnp {
    %%/mozaic/logging_capnp.rs%%
}

pub mod client_capnp {
    %%/mozaic/client_capnp.rs%%
}
