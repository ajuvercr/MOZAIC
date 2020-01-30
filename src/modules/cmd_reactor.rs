use core_capnp::initialize;
use errors::{self, Consumable};
use messaging::reactor::*;
use messaging::types::*;

use cmd_capnp::{cmd_input, cmd_return};

use std::io::{self, stdout, BufRead, Write};
use std::thread;

use runtime::BrokerHandle;

use futures::channel::mpsc;
use futures::stream::Stream;

/// Reactor to handle cmd input
pub struct CmdReactor {
    foreign_id: ReactorId,
    broker: BrokerHandle,
}

impl CmdReactor {
    pub fn new(broker: BrokerHandle, foreign_id: ReactorId) -> Self {
        CmdReactor { foreign_id, broker }
    }

    pub fn params<C: Ctx>(self) -> CoreParams<Self, C> {
        let mut params = CoreParams::new(self);
        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(cmd_return::Owned, CtxHandler::new(Self::handle_return));

        return params;
    }

    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<(), errors::Error> {
        handle.open_link(CmdLink.params(handle.id().clone()))?;

        handle.open_link(ForeignLink.params(self.foreign_id.clone()))?;

        setup_async_stdin(self.broker.clone(), handle.id().clone());

        self.new_line();
        Ok(())
    }

    fn handle_return<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        r: cmd_return::Reader,
    ) -> Result<(), errors::Error> {
        let msg = r.get_message()?;
        println!("< {}", msg);
        self.new_line();

        Ok(())
    }

    fn new_line(&self) {
        print!("> ");
        stdout().flush().unwrap();
    }
}

/// Link from the reactor to the cmd line, only handling incoming messages
struct CmdLink;

impl CmdLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.external_handler(cmd_input::Owned, CtxHandler::new(cmd_input::e_to_i));

        return params;
    }
}

/// Link from the cmd reactor to somewhere, sending through the cmd messages
/// Also listening for messages that have to return to the command line
struct ForeignLink;

impl ForeignLink {
    pub fn params<C: Ctx>(self, foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, self);
        params.internal_handler(cmd_input::Owned, CtxHandler::new(cmd_input::i_to_e));

        params.external_handler(cmd_return::Owned, CtxHandler::new(cmd_return::e_to_i));

        return params;
    }
}

/// Helper function to make stdin async
fn stdin() -> impl Stream<Item = String> {
    let (mut tx, rx) = mpsc::channel(1);
    thread::spawn(move || {
        let input = io::stdin();
        for line in input.lock().lines() {
            match line
                .map_err(|_| ())
                .and_then(|line| tx.try_send(line).map_err(|_| ()))
            {
                Ok(s) => {}
                Err(_) => break,
            }
        }
    });
    rx
}

use futures::{future, StreamExt};
/// Read stdin async, and send messages from it
/// Via broker, this is not preffered, but it's ok
fn setup_async_stdin(mut broker: BrokerHandle, id: ReactorId) {
    tokio::spawn(futures::future::lazy(move |_context| {
        stdin()
            .for_each(move |string| {
                broker
                    .send_message(&id, &id, cmd_input::Owned, move |b| {
                        let mut msg: cmd_input::Builder = b.init_as();
                        msg.set_input(&string);
                    })
                    .display();
                future::ready(())
            })
    }));
}
