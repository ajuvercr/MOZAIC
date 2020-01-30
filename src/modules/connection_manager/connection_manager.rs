use futures::future::FutureExt;
use futures::executor::ThreadPool;
use futures::task::SpawnExt;

use core_capnp::initialize;
use errors::{Consumable, Result};
use messaging::reactor::*;
use messaging::types::*;

use connection_capnp::{client_connected, client_disconnected, host_connected};
use core_capnp::{actor_joined, actors_joined, close, identify, drop};
use network_capnp::disconnected;

use runtime::BrokerHandle;

use runtime::TcpServer;
use std::net::SocketAddr;

use std::collections::HashMap;

use super::client_controller::CCReactor;
use modules::util::{Identifier, PlayerId};

type Closer = tokio::sync::oneshot::Sender<()>;

/// Main connection manager, creates handles for as many players as asked for
/// Handles disconnects, reconnects etc, host can always send messages to everybody
pub struct ConnectionManager {
    broker: BrokerHandle,
    threadpool: ThreadPool,

    ids: HashMap<Identifier, PlayerId>, // handle send to clients

    host: ReactorId,
    addr: SocketAddr,

    cc_count: usize,
    at_close: Option<Closer>,
}

use std::convert::TryInto;
impl ConnectionManager {
    pub fn params<C: Ctx>(
        broker: BrokerHandle,
        ids: HashMap<Identifier, PlayerId>,
        host: ReactorId,
        addr: SocketAddr,
        threadpool: ThreadPool,
    ) -> CoreParams<Self, C> {
        let cc_count = ids.len();
        let server_reactor = Self {
            broker,
            ids,
            host,
            addr,
            cc_count,
            at_close: None,
            threadpool,
        };

        let mut params = CoreParams::new(server_reactor);

        params.handler(initialize::Owned, CtxHandler::new(Self::handle_initialize));
        params.handler(
            actor_joined::Owned,
            CtxHandler::new(Self::handle_actor_joined),
        );
        params.handler(close::Owned, CtxHandler::new(Self::close));
        params.handler(drop::Owned, CtxHandler::new(Self::drop));

        return params;
    }

    /// Initialize by opening a link to the ip endpoint
    fn handle_initialize<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: initialize::Reader,
    ) -> Result<()> {
        handle.open_link(CreationLink::params(handle.id().clone()))?;
        handle.open_link(HostLink::params(self.host.clone()))?;

        // Create n ClientControllers
        let mut ids = Vec::new();
        for (key, id) in self.ids.drain() {
            let cc_id = handle.spawn(
                CCReactor::params(id, handle.id().clone(), self.host.clone()),
                "Client Controller",
            )?;
            ids.push(cc_id.clone());
            handle
                .open_link(ClientControllerLink::params(key, cc_id))
                .display();
        }

        // Send to host what ClientControllers are created
        let mut joined = MsgBuffer::<actors_joined::Owned>::new();
        joined.build(move |b| {
            let mut ids_builder = b.reborrow().init_ids(ids.len().try_into().unwrap());

            for (i, id) in ids.iter().enumerate() {
                ids_builder.set(i.try_into().unwrap(), id.bytes());
            }
        });

        handle.send_internal(joined)?;

        let (tx, rx) = tokio::sync::oneshot::channel();

        self.at_close = Some(tx);

        let broker = self.broker.clone();
        let id = handle.id().clone();
        let addr = self.addr.clone();
        self.threadpool.spawn(
            rx.then(
                move |_| {
                    TcpServer::new(
                        broker,
                        id,
                        addr,
                    )
                }
            ).map(|_| ()));

        Ok(())
    }

    /// Handle actor joined by opening ClientLink to him
    fn close<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        _: close::Reader,
    ) -> Result<()> {
        self.cc_count -= 1;

        if self.cc_count == 0 {
            handle.destroy()?;
        }

        Ok(())
    }

    /// Handle actor joined by opening ClientLink to him
    fn drop<C: Ctx>(
        &mut self,
        _: &mut ReactorHandle<C>,
        _: drop::Reader,
    ) -> Result<()> {
        use std::mem;

        let sender = mem::replace(&mut self.at_close, None);
        sender.map(|x| x.send(()));

        Ok(())
    }

    /// Handle actor joined by opening ClientLink to him
    fn handle_actor_joined<C: Ctx>(
        &mut self,
        handle: &mut ReactorHandle<C>,
        r: actor_joined::Reader,
    ) -> Result<()> {
        let id = r.get_id()?;
        handle.open_link(ClientLink::params(id.into()))?;

        Ok(())
    }
}

struct HostLink;
impl HostLink {
    fn params<C: Ctx>(host: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(host, HostLink);

        params.internal_handler(
            actors_joined::Owned,
            CtxHandler::new(Self::i_handle_actors_joined),
        );

        return params;
    }

    fn i_handle_actors_joined<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: actors_joined::Reader,
    ) -> Result<()> {
        let m = ::messaging::types::MsgBuffer::<actors_joined::Owned>::from_reader(r)?;
        handle.send_message(m)?;

        handle.close_link()?;

        Ok(())
    }
}

/// Creation link to pass through actor joined from hopefully self
/// This is used to 'inject' actor joined events when clients connect.
struct CreationLink;
impl CreationLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let mut params = LinkParams::new(foreign_id, CreationLink);

        params.external_handler(actor_joined::Owned, CtxHandler::new(actor_joined::e_to_i));

        return params;
    }
}

struct ClientControllerLink {
    key: Identifier,
}

impl ClientControllerLink {
    pub fn params<C: Ctx>(key: Identifier, remote_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self { key };

        let mut params = LinkParams::new(remote_id, me);

        params.internal_handler(
            client_connected::Owned,
            CtxHandler::new(Self::i_handle_connected),
        );

        params.internal_handler(
            client_disconnected::Owned,
            CtxHandler::new(Self::i_handle_disconnected),
        );

        params.external_handler(close::Owned, CtxHandler::new(Self::e_handle_close));

        return params;
    }

    fn e_handle_close<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: close::Reader,
    ) -> Result<()> {
        handle.close_link()?;

        let joined = MsgBuffer::<close::Owned>::new();
        handle.send_internal(joined)?;

        Ok(())
    }

    fn i_handle_connected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_connected::Reader,
    ) -> Result<()> {
        let key = r.get_client_key();
        let self_key: u64 = self.key.into();

        if key == self_key {
            let id = r.get_id()?;

            let mut joined = MsgBuffer::<actor_joined::Owned>::new();
            joined.build(|b| b.set_id(id));
            handle.send_message(joined).display();

            let mut host_joined = MsgBuffer::<host_connected::Owned>::new();
            host_joined.build(|b| {
                b.set_client_key(key);
                b.set_id(handle.remote_uuid().bytes());
            });
            handle.send_internal(host_joined).display();
        }

        Ok(())
    }

    fn i_handle_disconnected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: client_disconnected::Reader,
    ) -> Result<()> {
        let key = r.get_client_key();
        let self_key: u64 = self.key.into();

        if key == self_key {
            let joined = MsgBuffer::<client_disconnected::Owned>::new();
            handle.send_message(joined)?;
        }

        Ok(())
    }
}

/// Link with the client, passing though disconnects and messages
struct ClientLink {
    key: Option<Identifier>,
}

impl ClientLink {
    pub fn params<C: Ctx>(foreign_id: ReactorId) -> LinkParams<Self, C> {
        let me = Self { key: None };

        let mut params = LinkParams::new(foreign_id, me);

        params.external_handler(identify::Owned, CtxHandler::new(Self::e_handle_identify));

        params.external_handler(
            disconnected::Owned,
            CtxHandler::new(Self::e_handle_disconnect),
        );

        params.internal_handler(
            host_connected::Owned,
            CtxHandler::new(Self::i_handle_host_connected),
        );

        return params;
    }

    fn e_handle_identify<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: identify::Reader,
    ) -> Result<()> {
        let key = r.get_key();

        let mut joined = MsgBuffer::<client_connected::Owned>::new();
        joined.build(|b| {
            b.set_client_key(key);
            b.set_id(handle.remote_uuid().bytes());
        });
        handle.send_internal(joined)?;

        self.key = Some(Identifier::from(key));

        Ok(())
    }

    fn e_handle_disconnect<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        _: disconnected::Reader,
    ) -> Result<()> {
        // If not the client is not yet registered, so it doesn't matter
        if let Some(key) = self.key {
            let mut msg = MsgBuffer::<client_disconnected::Owned>::new();

            msg.build(|b| {
                b.set_client_key(key.into());
            });
            handle.send_internal(msg)?;

            // Don't try to close the link on the other side, because pipe is already broken
            handle.close_link_hard()?;
        }

        Ok(())
    }

    fn i_handle_host_connected<C: Ctx>(
        &mut self,
        handle: &mut LinkHandle<C>,
        r: host_connected::Reader,
    ) -> Result<()> {
        if let Some(key) = self.key {
            let self_key: u64 = key.into();
            let key = r.get_client_key();

            if key == self_key {
                let id = r.get_id()?;

                let mut joined = MsgBuffer::<actor_joined::Owned>::new();

                joined.build(|b| {
                    b.set_id(id);
                });
                handle.send_message(joined)?;
            }
        }

        Ok(())
    }
}
