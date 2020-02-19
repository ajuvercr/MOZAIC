use crate::generic::*;
use crate::modules::net::types::*;
use crate::modules::types::*;

use std::sync::{Arc, Mutex};

use rand::random;

use std::any;
use std::collections::HashMap;

#[derive(Clone)]
pub struct RegisterGame {
    game: u64,
    players: Vec<(PlayerId, ReactorID)>,
}

#[derive(Clone)]
pub struct PlayerUUIDs {
    game: u64,
    ids: Vec<u64>,
}

pub type BoxSpawnPlayer = Arc<Mutex<Option<SpawnPlayer>>>;

pub struct SpawnPlayer {
    pub player: u64,
    pub builder: Box<
        dyn FnOnce(ReactorID, SenderHandle<any::TypeId, Message>) -> Sender<any::TypeId, Message>
            + Send
            + Sync
            + 'static,
    >,
}

impl SpawnPlayer {
    pub fn new<
        F: FnOnce(ReactorID, SenderHandle<any::TypeId, Message>) -> Sender<any::TypeId, Message>
            + 'static
            + Send
            + Sync,
    >(
        player: u64,
        f: F,
    ) -> BoxSpawnPlayer {
        Arc::new(Mutex::new(Some(Self {
            player,
            builder: Box::new(f),
        })))
    }
}

#[derive(Clone)]
pub struct RegisterEndpoint(ReactorID);

pub struct ClientManager {
    clients: HashMap<u64, (PlayerId, ReactorID)>,
    game_manager: ReactorID,
}

impl ClientManager {
    pub fn new(game_manager: ReactorID) -> CoreParams<Self, any::TypeId, Message> {
        let me = Self {
            game_manager,
            clients: HashMap::new(),
        };

        CoreParams::new(me)
            .handler(FunctionHandler::from(Self::handle_spawn_game))
            .handler(FunctionHandler::from(Self::handle_cc_close))
            .handler(FunctionHandler::from(Self::handle_register_endpoint))
            .handler(FunctionHandler::from(Self::handle_player_regiser))
    }

    fn handle_spawn_game(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        cs: &RegisterGame,
    ) {
        let new_cliets: HashMap<u64, (PlayerId, ReactorID)> = cs
            .players
            .iter()
            .cloned()
            .map(|id| (random(), id))
            .collect();
        let ids: Vec<u64> = new_cliets.keys().cloned().collect();
        self.clients.extend(new_cliets);

        handle.send_internal(
            PlayerUUIDs { game: cs.game, ids },
            TargetReactor::Link(self.game_manager),
        );

        for (_, cc) in &cs.players {
            let cc_params = LinkParams::new(())
                .internal_handler(FunctionHandler::from(i_to_e::<(), Accepted>()))
                .closer(|_, handle| {
                    handle.send_internal(*handle.target_id(), TargetReactor::Reactor);
                });
            handle.open_link(*cc, cc_params, false)
        }
    }

    fn handle_cc_close(&mut self, _: &mut ReactorHandle<any::TypeId, Message>, id: &ReactorID) {
        let orig_len = self.clients.len();
        self.clients.retain(|_, (_, x)| x != id);

        if orig_len == self.clients.len() {
            error!(%id, "Tried to remove client controller, but client controller was not there");
        } else if orig_len == self.clients.len() - 1 {
            trace!(%id, "Successfully removed client controller");
        } else {
            error!(%id, "Remove more then one client controller!");
        }
    }

    fn handle_register_endpoint(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        reg: &RegisterEndpoint,
    ) {
        let ep_link_params =
            LinkParams::new(()).external_handler(FunctionHandler::from(
                e_to_i::<(), BoxSpawnPlayer>(TargetReactor::Reactor),
            ));
        handle.open_link(reg.0, ep_link_params, false);
    }

    fn handle_player_regiser(
        &mut self,
        handle: &mut ReactorHandle<any::TypeId, Message>,
        reg: &BoxSpawnPlayer,
    ) {
        let mut reg = reg.lock().unwrap();
        let reg = std::mem::replace(&mut *reg, None);

        if let Some(SpawnPlayer { player, builder }) = reg {
            if let Some((player, cc)) = self.clients.get(&player) {
                let id = ReactorID::rand();
                handle.open_reactor_like(id, builder(id, handle.get(cc)));

                let accept = Accepted {
                    player: *player,
                    client_id: id,
                    contr_id: *cc,
                };
                handle.send_internal(accept.clone(), TargetReactor::Link(*cc));
            }
        }
    }
}

impl ReactorState<any::TypeId, Message> for ClientManager {
    const NAME: &'static str = "Client Manager";

    fn init<'a>(&mut self, handle: &mut ReactorHandle<'a, any::TypeId, Message>) {
        let gm_link_params = LinkParams::new(())
            .internal_handler(FunctionHandler::from(i_to_e::<(), PlayerUUIDs>()))
            .external_handler(FunctionHandler::from(e_to_i::<(), RegisterGame>(
                TargetReactor::Reactor,
            )))
            .external_handler(FunctionHandler::from(e_to_i::<(), RegisterEndpoint>(
                TargetReactor::Reactor,
            )));
        handle.open_link(self.game_manager, gm_link_params, false);
    }
}
